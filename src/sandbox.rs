use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use url::Url;

use crate::config::Render as RenderCfg;
use crate::contract::{MonoSource, Theme};
use crate::discover::{self, Candidate, Location, Page, Scope};
use crate::render;
use crate::vector::{self, Vector, VectorOrigin};

pub const MAX_REQUEST: usize = 4 * 1024 * 1024;
pub const MAX_RESPONSE: usize = 8 * 1024 * 1024;
const MAX_CANDIDATES: usize = 256;
const STATUS_OK: u8 = 0;
const STATUS_ERR: u8 = 1;
const SIGSYS: i32 = 31;

#[derive(Debug, Serialize, Deserialize)]
pub struct Job {
    pub cfg: RenderCfg,
    pub task: Task,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Task {
    Render,
    Mono { source: MonoSource },
    Html { url: String, scope: Scope },
    Manifest { url: String, scope: Scope },
}

pub enum Output {
    Vector(Vector),
    Page(Page),
}

#[derive(Serialize, Deserialize)]
struct WireCandidate {
    url: Option<String>,
    inline: Option<usize>,
    declared: u32,
    svg: bool,
    theme: Option<Theme>,
    #[serde(default)]
    mono: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum WireMeta {
    Vector {
        origin: VectorOrigin,
    },
    Page {
        icons: Vec<WireCandidate>,
        manifest: Option<String>,
    },
}

pub fn encode_request(job: &Job, payload: &[u8]) -> anyhow::Result<Vec<u8>> {
    let header = serde_json::to_vec(job)?;
    let mut out = Vec::with_capacity(header.len() + payload.len() + 8);
    put_frame(&mut out, &header)?;
    put_frame(&mut out, payload)?;
    Ok(out)
}

pub fn decode_request(mut buf: &[u8]) -> anyhow::Result<(Job, &[u8])> {
    let header = take_frame(&mut buf).context("missing header")?;
    let payload = take_frame(&mut buf).context("missing payload")?;
    anyhow::ensure!(buf.is_empty(), "trailing bytes in request");
    Ok((serde_json::from_slice(header)?, payload))
}

pub fn encode_response(result: &anyhow::Result<Output>) -> Vec<u8> {
    let encoded = result
        .as_ref()
        .map_err(|e| format!("{e:#}"))
        .and_then(|output| {
            let (meta, blobs): (WireMeta, Vec<&[u8]>) = match output {
                Output::Vector(v) => (
                    WireMeta::Vector { origin: v.origin },
                    vec![v.svg.as_bytes()],
                ),
                Output::Page(page) => {
                    let mut blobs = Vec::new();
                    let icons = page
                        .icons
                        .iter()
                        .map(|c| {
                            let (url, inline) = match &c.location {
                                Location::Remote(u) => (Some(u.to_string()), None),
                                Location::Inline(b) => {
                                    blobs.push(b.as_slice());
                                    (None, Some(blobs.len() - 1))
                                }
                            };
                            WireCandidate {
                                url,
                                inline,
                                declared: c.declared,
                                svg: c.svg,
                                theme: c.theme,
                                mono: c.mono,
                            }
                        })
                        .collect();
                    let manifest = page.manifest.as_ref().map(Url::to_string);
                    (WireMeta::Page { icons, manifest }, blobs)
                }
            };
            let mut out = vec![STATUS_OK];
            let meta = serde_json::to_vec(&meta).map_err(|e| e.to_string())?;
            put_frame(&mut out, &meta).map_err(|e| e.to_string())?;
            out.extend_from_slice(&(blobs.len() as u32).to_le_bytes());
            for b in blobs {
                put_frame(&mut out, b).map_err(|e| e.to_string())?;
            }
            Ok(out)
        });
    encoded.unwrap_or_else(|msg| {
        let mut out = vec![STATUS_ERR];
        out.extend_from_slice(msg.as_bytes());
        out
    })
}

fn decode_frames(buf: &[u8]) -> anyhow::Result<(WireMeta, Vec<&[u8]>)> {
    let (&status, mut rest) = buf.split_first().context("empty worker response")?;
    if status == STATUS_ERR {
        bail!("{}", String::from_utf8_lossy(&rest[..rest.len().min(512)]));
    }
    anyhow::ensure!(status == STATUS_OK, "bad worker status");
    let meta = serde_json::from_slice(take_frame(&mut rest).context("missing meta")?)?;
    let count = take_u32(&mut rest).context("missing blob count")? as usize;
    anyhow::ensure!(count <= MAX_CANDIDATES, "too many blobs");
    let mut blobs = Vec::with_capacity(count);
    for _ in 0..count {
        blobs.push(take_frame(&mut rest).context("missing blob")?);
    }
    anyhow::ensure!(rest.is_empty(), "trailing bytes in response");
    Ok((meta, blobs))
}

pub fn decode_vector(buf: &[u8], max_bytes: usize) -> anyhow::Result<Vector> {
    let (WireMeta::Vector { origin }, blobs) = decode_frames(buf)? else {
        bail!("worker returned a page for a render task");
    };
    let [svg] = blobs.as_slice() else {
        bail!("expected exactly one svg");
    };
    let svg = std::str::from_utf8(svg).context("svg is not utf-8")?;
    vector::verify(svg, max_bytes).context("worker returned an unsafe svg")?;
    Ok(Vector {
        svg: svg.to_string(),
        origin,
    })
}

pub fn decode_page(buf: &[u8], scope: &Scope) -> anyhow::Result<Page> {
    let (WireMeta::Page { icons, manifest }, blobs) = decode_frames(buf)? else {
        bail!("worker returned variants for a parse task");
    };
    anyhow::ensure!(icons.len() <= MAX_CANDIDATES, "too many candidates");
    let manifest = manifest
        .and_then(|m| Url::parse(&m).ok())
        .filter(|u| scope.allows(u));
    let icons = icons
        .into_iter()
        .filter_map(|w| {
            let location = match (w.url, w.inline) {
                (Some(u), None) => {
                    Location::Remote(Url::parse(&u).ok().filter(|u| scope.allows(u))?)
                }
                (None, Some(i)) => {
                    let data = blobs.get(i)?;
                    (data.len() <= scope.max_inline).then(|| Location::Inline(data.to_vec()))?
                }
                _ => return None,
            };
            Some(Candidate {
                location,
                declared: w.declared,
                svg: w.svg,
                theme: w.theme,
                mono: w.mono,
            })
        })
        .collect();
    Ok(Page { icons, manifest })
}

fn put_frame(out: &mut Vec<u8>, bytes: &[u8]) -> anyhow::Result<()> {
    let len = u32::try_from(bytes.len()).context("frame too large")?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn take_u32(buf: &mut &[u8]) -> Option<u32> {
    let (head, rest) = buf.split_first_chunk::<4>()?;
    *buf = rest;
    Some(u32::from_le_bytes(*head))
}

fn take_frame<'a>(buf: &mut &'a [u8]) -> Option<&'a [u8]> {
    let len = take_u32(buf)? as usize;
    if buf.len() < len {
        return None;
    }
    let (frame, rest) = buf.split_at(len);
    *buf = rest;
    Some(frame)
}

pub fn execute(job: &Job, payload: &[u8]) -> anyhow::Result<Output> {
    match &job.task {
        Task::Render => render::vectorize(payload, &job.cfg).map(Output::Vector),
        Task::Mono { source } => {
            render::vectorize_mono(payload, &job.cfg, *source).map(Output::Vector)
        }
        Task::Html { url, scope } => {
            let url = Url::parse(url)?;
            Ok(Output::Page(discover::parse_html(payload, &url, scope)))
        }
        Task::Manifest { url, scope } => {
            let url = Url::parse(url)?;
            let icons = if discover::looks_like_json_object(payload) {
                discover::parse_manifest(payload, &url, scope)
            } else {
                vec![]
            };
            Ok(Output::Page(Page {
                icons,
                manifest: None,
            }))
        }
    }
}

pub fn worker_main() -> ExitCode {
    let mut input = Vec::new();
    let read = std::io::stdin()
        .take(MAX_REQUEST as u64 + 1)
        .read_to_end(&mut input);
    let result = match read {
        Ok(_) if input.len() > MAX_REQUEST => Err(anyhow::anyhow!("request too large")),
        Ok(_) => decode_request(&input).and_then(|(job, payload)| {
            confine(&job.cfg).context("sandbox setup failed")?;
            execute(&job, payload)
        }),
        Err(e) => Err(e.into()),
    };
    let out = encode_response(&result);
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&out)
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return ExitCode::from(3);
    }
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn confine(cfg: &RenderCfg) -> anyhow::Result<()> {
    use rustix::process::{Resource, Rlimit, setrlimit};
    let limit = |r: Resource, v: u64| {
        setrlimit(
            r,
            Rlimit {
                current: Some(v),
                maximum: Some(v),
            },
        )
        .with_context(|| format!("setrlimit {r:?}"))
    };
    let cpu_secs = cfg.decode_timeout_ms.div_ceil(1000).max(1);
    limit(Resource::Cpu, cpu_secs)?;
    limit(Resource::Core, 0)?;
    #[cfg(target_os = "linux")]
    {
        limit(Resource::As, cfg.worker_memory_bytes)?;
        limit(Resource::Nofile, 0)?;
        limit(Resource::Nproc, 0)?;
        rustix::thread::set_no_new_privs(true).context("no_new_privs")?;
        seccomp::install()?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
mod seccomp {
    use std::collections::BTreeMap;

    use anyhow::Context;
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};

    const ALLOWED: &[libc::c_long] = &[
        libc::SYS_write,
        libc::SYS_writev,
        libc::SYS_close,
        libc::SYS_brk,
        libc::SYS_mmap,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_madvise,
        libc::SYS_mprotect,
        libc::SYS_futex,
        libc::SYS_getrandom,
        libc::SYS_clock_gettime,
        libc::SYS_sched_yield,
        libc::SYS_rt_sigreturn,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigaction,
        libc::SYS_sigaltstack,
        libc::SYS_gettid,
        libc::SYS_getpid,
        libc::SYS_tgkill,
        libc::SYS_restart_syscall,
        libc::SYS_exit,
        libc::SYS_exit_group,
    ];

    pub fn install() -> anyhow::Result<()> {
        let rules = ALLOWED
            .iter()
            .map(|s| (*s, vec![]))
            .collect::<BTreeMap<_, _>>();
        let arch = TargetArch::try_from(std::env::consts::ARCH).context("unsupported arch")?;
        let filter = SeccompFilter::new(
            rules,
            SeccompAction::KillProcess,
            SeccompAction::Allow,
            arch,
        )
        .context("building seccomp filter")?;
        let program: BpfProgram = filter.try_into().context("compiling seccomp filter")?;
        seccompiler::apply_filter(&program).context("installing seccomp filter")?;
        Ok(())
    }
}

pub fn selftest(probe: &str) -> ExitCode {
    if std::env::var_os(ALLOC_ENV.0).is_none() {
        use std::os::unix::process::CommandExt;
        let err = std::env::current_exe()
            .map(|exe| {
                std::process::Command::new(exe)
                    .args(["sandbox-selftest", probe])
                    .env(ALLOC_ENV.0, ALLOC_ENV.1)
                    .exec()
            })
            .unwrap_or_else(|e| e);
        eprintln!("re-exec failed: {err}");
        return ExitCode::from(2);
    }
    let cfg = RenderCfg {
        decode_timeout_ms: 120_000,
        ..RenderCfg::default()
    };
    let jobs = if probe == "render" {
        match sample_jobs(&cfg) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("building samples failed: {e:#}");
                return ExitCode::from(2);
            }
        }
    } else {
        vec![]
    };
    if let Err(e) = confine(&cfg) {
        eprintln!("sandbox setup failed: {e:#}");
        return ExitCode::from(2);
    }
    let escaped = match probe {
        "fs" => std::fs::File::open("/etc/hostname").is_ok() || std::fs::File::open("/").is_ok(),
        "write" => std::fs::write("/tmp/favicons-sandbox-probe", b"x").is_ok(),
        "net" => std::net::TcpStream::connect(("1.1.1.1", 443)).is_ok(),
        "exec" => std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("true")
            .status()
            .is_ok(),
        "render" => {
            let mut failed = false;
            for (name, job, payload) in &jobs {
                let ok = match execute(job, payload) {
                    Ok(Output::Vector(v)) => {
                        vector::verify(&v.svg, job.cfg.max_output_svg_bytes).is_ok()
                    }
                    Ok(Output::Page(p)) => !p.icons.is_empty(),
                    Err(e) => {
                        eprintln!("{name}: {e:#}");
                        false
                    }
                };
                eprintln!("{name}: {}", if ok { "ok" } else { "FAILED" });
                failed |= !ok;
            }
            return if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
        other => {
            eprintln!("unknown probe {other}; use fs | write | net | exec | render");
            return ExitCode::from(2);
        }
    };
    if escaped {
        eprintln!("SANDBOX ESCAPE: probe {probe} succeeded");
        ExitCode::FAILURE
    } else {
        eprintln!("probe {probe} denied");
        ExitCode::SUCCESS
    }
}

fn sample_jobs(cfg: &RenderCfg) -> anyhow::Result<Vec<(String, Job, Vec<u8>)>> {
    let scope = Scope {
        allow_http: true,
        max_inline: 1024,
    };
    let mut jobs: Vec<(String, Job, Vec<u8>)> = sample_images()?
        .into_iter()
        .map(|(name, bytes)| {
            let job = Job {
                cfg: cfg.clone(),
                task: Task::Render,
            };
            (format!("render {name}"), job, bytes)
        })
        .collect();
    jobs.push((
        "mono mask-icon".into(),
        Job { cfg: cfg.clone(), task: Task::Mono { source: MonoSource::MaskIcon } },
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M2 2h12v12H2z"/></svg>"##.to_vec(),
    ));
    jobs.push((
        "parse html".into(),
        Job {
            cfg: cfg.clone(),
            task: Task::Html { url: "https://example.com/".into(), scope: scope.clone() },
        },
        br#"<!doctype html><html><head><title>x</title><link rel="icon" href="/a.png" sizes="32x32"></head><body></body></html>"#.to_vec(),
    ));
    jobs.push((
        "parse manifest".into(),
        Job {
            cfg: cfg.clone(),
            task: Task::Manifest {
                url: "https://example.com/m.json".into(),
                scope,
            },
        },
        br#"{"icons":[{"src":"/i.png","sizes":"192x192"}]}"#.to_vec(),
    ));
    Ok(jobs)
}

fn sample_images() -> anyhow::Result<Vec<(&'static str, Vec<u8>)>> {
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    let rgba = RgbaImage::from_fn(48, 48, |x, y| {
        Rgba([(x * 5) as u8, (y * 5) as u8, 128, 200])
    });
    let img = DynamicImage::ImageRgba8(rgba);
    let mut out = Vec::new();
    for (name, format) in [
        ("png", ImageFormat::Png),
        ("gif", ImageFormat::Gif),
        ("webp", ImageFormat::WebP),
        ("bmp", ImageFormat::Bmp),
        ("ico", ImageFormat::Ico),
    ] {
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, format)?;
        out.push((name, buf.into_inner()));
    }
    let mut jpeg = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(img.to_rgb8()).write_to(&mut jpeg, ImageFormat::Jpeg)?;
    out.push(("jpeg", jpeg.into_inner()));
    out.push((
        "svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><defs><linearGradient id="g"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient><filter id="b"><feGaussianBlur stdDeviation="1"/></filter></defs><circle cx="5" cy="5" r="4" fill="url(#g)" filter="url(#b)"/></svg>"##.to_vec(),
    ));
    Ok(out)
}

pub const ALLOC_ENV: (&str, &str) = ("MIMALLOC_ARENA_RESERVE", "8MiB");

pub async fn run_worker(exe: &Path, request: &[u8], timeout: Duration) -> anyhow::Result<Vec<u8>> {
    let mut child = tokio::process::Command::new(exe)
        .arg("render-worker")
        .env_clear()
        .env(ALLOC_ENV.0, ALLOC_ENV.1)
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("spawning render worker")?;
    let mut stdin = child.stdin.take().context("worker stdin")?;
    let mut stdout = child.stdout.take().context("worker stdout")?;
    let exchange = async {
        let write = async {
            stdin.write_all(request).await?;
            drop(stdin);
            Ok::<_, std::io::Error>(())
        };
        let read = async {
            let mut out = Vec::new();
            (&mut stdout)
                .take(MAX_RESPONSE as u64 + 1)
                .read_to_end(&mut out)
                .await?;
            Ok::<_, std::io::Error>(out)
        };
        let ((), out) = tokio::try_join!(write, read)?;
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status, out))
    };
    let (status, out) = match tokio::time::timeout(timeout, exchange).await {
        Ok(r) => r.context("talking to render worker")?,
        Err(_) => bail!("render worker timed out and was killed"),
    };
    if out.len() > MAX_RESPONSE {
        bail!("render worker response too large");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if status.signal() == Some(SIGSYS) {
            bail!("render worker killed by seccomp (forbidden syscall)");
        }
        if let Some(sig) = status.signal() {
            bail!("render worker killed by signal {sig}");
        }
    }
    Ok(out)
}

#[derive(Clone)]
pub enum Renderer {
    InProcess,
    Process { exe: Arc<PathBuf> },
    Socket { path: Arc<PathBuf> },
}

impl Renderer {
    pub async fn render(&self, cfg: &RenderCfg, image: Vec<u8>) -> anyhow::Result<Vector> {
        let job = Job {
            cfg: cfg.clone(),
            task: Task::Render,
        };
        decode_vector(&self.exchange(job, image).await?, cfg.max_output_svg_bytes)
    }

    pub async fn mono(
        &self,
        cfg: &RenderCfg,
        source: MonoSource,
        image: Vec<u8>,
    ) -> anyhow::Result<Vector> {
        let job = Job {
            cfg: cfg.clone(),
            task: Task::Mono { source },
        };
        decode_vector(&self.exchange(job, image).await?, cfg.max_output_svg_bytes)
    }

    pub async fn parse_html(
        &self,
        cfg: &RenderCfg,
        url: &Url,
        scope: &Scope,
        html: Vec<u8>,
    ) -> anyhow::Result<Page> {
        let task = Task::Html {
            url: url.to_string(),
            scope: scope.clone(),
        };
        let out = self
            .exchange(
                Job {
                    cfg: cfg.clone(),
                    task,
                },
                html,
            )
            .await?;
        decode_page(&out, scope)
    }

    pub async fn parse_manifest(
        &self,
        cfg: &RenderCfg,
        url: &Url,
        scope: &Scope,
        json: Vec<u8>,
    ) -> anyhow::Result<Vec<Candidate>> {
        let task = Task::Manifest {
            url: url.to_string(),
            scope: scope.clone(),
        };
        let out = self
            .exchange(
                Job {
                    cfg: cfg.clone(),
                    task,
                },
                json,
            )
            .await?;
        Ok(decode_page(&out, scope)?.icons)
    }

    async fn exchange(&self, job: Job, payload: Vec<u8>) -> anyhow::Result<Vec<u8>> {
        let timeout = Duration::from_millis(job.cfg.decode_timeout_ms + 1_000);
        match self {
            Renderer::InProcess => {
                let task =
                    tokio::task::spawn_blocking(move || encode_response(&execute(&job, &payload)));
                match tokio::time::timeout(timeout, task).await {
                    Ok(Ok(out)) => Ok(out),
                    Ok(Err(e)) => bail!("in-process decoder panicked: {e}"),
                    Err(_) => bail!("in-process decoder timed out"),
                }
            }
            Renderer::Process { exe } => {
                run_worker(exe, &encode_request(&job, &payload)?, timeout).await
            }
            Renderer::Socket { path } => {
                let request = encode_request(&job, &payload)?;
                let exchange = async {
                    let mut stream = tokio::net::UnixStream::connect(path.as_path())
                        .await
                        .context("connecting to render server")?;
                    stream.write_all(&request).await?;
                    stream.shutdown().await?;
                    let mut out = Vec::new();
                    (&mut stream)
                        .take(MAX_RESPONSE as u64 + 1)
                        .read_to_end(&mut out)
                        .await?;
                    anyhow::ensure!(
                        out.len() <= MAX_RESPONSE,
                        "render server response too large"
                    );
                    Ok(out)
                };
                tokio::time::timeout(timeout + Duration::from_secs(1), exchange)
                    .await
                    .context("render server timed out")?
            }
        }
    }
}

pub async fn serve(socket: &Path, cfg: RenderCfg) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let exe = std::env::current_exe().context("locating own executable")?;
    let _ = std::fs::remove_file(socket);
    let listener = tokio::net::UnixListener::bind(socket)
        .with_context(|| format!("binding {}", socket.display()))?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!(socket = %socket.display(), "render server listening");
    let slots = Arc::new(Semaphore::new(cfg.decode_concurrency));
    let cfg = Arc::new(cfg);
    let timeout = Duration::from_millis(cfg.decode_timeout_ms + 1_000);
    loop {
        let (mut stream, _) = listener.accept().await?;
        let (slots, cfg, exe) = (slots.clone(), cfg.clone(), exe.clone());
        tokio::spawn(async move {
            let Ok(_permit) = slots.acquire_owned().await else {
                return;
            };
            let handle = async {
                let mut request = Vec::new();
                (&mut stream)
                    .take(MAX_REQUEST as u64 + 1)
                    .read_to_end(&mut request)
                    .await?;
                let response = match decode_request(&request) {
                    Ok(_) if request.len() > MAX_REQUEST => {
                        encode_response(&Err(anyhow::anyhow!("request too large")))
                    }
                    Ok((job, payload)) => {
                        let local = Job {
                            cfg: (*cfg).clone(),
                            task: job.task,
                        };
                        match run_worker(&exe, &encode_request(&local, payload)?, timeout).await {
                            Ok(out) => out,
                            Err(e) => encode_response(&Err(e)),
                        }
                    }
                    Err(e) => encode_response(&Err(e)),
                };
                stream.write_all(&response).await?;
                stream.shutdown().await?;
                Ok::<_, anyhow::Error>(())
            };
            if let Err(e) = tokio::time::timeout(timeout + Duration::from_secs(2), handle).await {
                tracing::warn!(error = %e, "render request timed out");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        Scope {
            allow_http: true,
            max_inline: 8,
        }
    }

    #[test]
    fn round_trips_render_protocol() {
        let job = Job {
            cfg: RenderCfg::default(),
            task: Task::Render,
        };
        let req = encode_request(&job, b"img").unwrap();
        let (back, payload) = decode_request(&req).unwrap();
        assert_eq!(payload, b"img");
        assert!(matches!(back.task, Task::Render));

        let svg = crate::vector::warning(false);
        let ok = encode_response(&Ok(Output::Vector(Vector {
            svg: svg.clone(),
            origin: VectorOrigin::Traced,
        })));
        let v = decode_vector(&ok, 1 << 20).unwrap();
        assert_eq!((v.svg, v.origin), (svg, VectorOrigin::Traced));

        let err = encode_response(&Err(anyhow::anyhow!("boom")));
        assert_eq!(
            decode_vector(&err, 1 << 20).unwrap_err().to_string(),
            "boom"
        );
    }

    #[test]
    fn parent_revalidates_parsed_page() {
        let hostile = Page {
            icons: vec![
                Candidate {
                    location: Location::Remote(Url::parse("https://example.com/ok.png").unwrap()),
                    declared: 32,
                    svg: false,
                    theme: None,
                    mono: false,
                },
                Candidate {
                    location: Location::Remote(Url::parse("https://evil.net/x.png").unwrap()),
                    declared: 512,
                    svg: false,
                    theme: None,
                    mono: false,
                },
                Candidate {
                    location: Location::Remote(Url::parse("http://10.0.0.1/x.png").unwrap()),
                    declared: 512,
                    svg: false,
                    theme: None,
                    mono: false,
                },
                Candidate {
                    location: Location::Inline(vec![0; 64]),
                    declared: 512,
                    svg: false,
                    theme: None,
                    mono: false,
                },
                Candidate {
                    location: Location::Inline(vec![1, 2]),
                    declared: 16,
                    svg: false,
                    theme: None,
                    mono: false,
                },
            ],
            manifest: Some(Url::parse("https://evil.net/m.json").unwrap()),
        };
        let page = decode_page(&encode_response(&Ok(Output::Page(hostile))), &scope()).unwrap();
        assert_eq!(page.icons.len(), 3);
        assert_eq!(page.icons[2].location, Location::Inline(vec![1, 2]));
        assert!(page.manifest.is_some());
    }

    #[test]
    fn rejects_malformed_frames() {
        assert!(decode_request(&[]).is_err());
        assert!(decode_request(&[255, 255, 255, 255, 1]).is_err());
        let page = encode_response(&Ok(Output::Page(Page::default())));
        assert!(decode_vector(&page, 1 << 20).is_err());
        let hostile = encode_response(&Ok(Output::Vector(Vector {
            svg: r#"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"><script>alert(1)</script></svg>"#.into(),
            origin: VectorOrigin::Original,
        })));
        assert!(decode_vector(&hostile, 1 << 20).is_err());
        let mut many = vec![STATUS_OK];
        many.extend_from_slice(&2u32.to_le_bytes());
        many.extend_from_slice(b"{}");
        assert!(decode_vector(&many, 1 << 20).is_err());
    }
}
