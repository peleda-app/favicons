//! favicons: a privacy-first website icon service that serves any site's icon as a clean,
//! sandboxed SVG with light and dark theme colours. See `README.md` and `docs/`.

mod age;
mod app;
mod catalog;
mod config;
mod contract;
mod discover;
mod disk;
mod dns;
mod domain;
mod http;
mod limiter;
mod metrics;
mod net;
mod openapi;
mod pack;
mod palette;
mod plate;
mod policy;
mod queue;
mod render;
mod sandbox;
mod server;
mod site;
mod slugs;
mod store;
mod tls;
mod trace;
mod vector;

use std::path::PathBuf;
use std::process::ExitCode;

use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1).peekable();
    match args.peek().map(String::as_str) {
        Some("render-worker") => return sandbox::worker_main(),
        Some("sandbox-selftest") => {
            let probe = args.nth(1).unwrap_or_default();
            return sandbox::selftest(&probe);
        }
        _ => {}
    }
    match run_main(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_main(mut args: impl Iterator<Item = String>) -> anyhow::Result<()> {
    let mut render_server = false;
    let mut prerender = false;
    let mut domain_list: Option<PathBuf> = None;
    let mut pack: Option<(PathBuf, PathBuf)> = None;
    let mut config_path = std::env::var_os("FAVICONS_CONFIG").map(PathBuf::from);
    let mut check_only = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => config_path = args.next().map(PathBuf::from),
            "--check-config" => check_only = true,
            "render-server" => render_server = true,
            "prerender" => prerender = true,
            "pack" => {
                let (Some(dir), Some(out)) = (args.next(), args.next()) else {
                    anyhow::bail!("usage: favicons pack DIR OUT.pack");
                };
                pack = Some((dir.into(), out.into()));
            }
            "prerender-domains" => {
                domain_list = Some(args.next().map(PathBuf::from).ok_or_else(|| {
                    anyhow::anyhow!("prerender-domains needs a file with one domain per line")
                })?)
            }
            "-V" | "--version" => {
                println!("favicons {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other => anyhow::bail!(
                "unknown argument {other}; usage: favicons [render-server | prerender | prerender-domains FILE | pack DIR OUT | render-worker | sandbox-selftest PROBE] [--config PATH] [--check-config]"
            ),
        }
    }

    if let Some((dir, out)) = pack {
        let n = pack::write(&dir, store::LAYOUT_VERSION, &out)?;
        println!("packed {n} files into {}", out.display());
        return Ok(());
    }

    let filter = EnvFilter::try_from_env("FAVICONS_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    if std::env::var("FAVICONS_LOG_FORMAT").is_ok_and(|v| v == "json") {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let _ = rustls::crypto::ring::default_provider().install_default();
    let cfg = config::Config::load(config_path.as_deref())?;
    if check_only {
        println!("config ok");
        return Ok(());
    }
    if render_server {
        let render = cfg.render.clone();
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(sandbox::serve(&render.socket_path.clone(), render));
    }

    let mut runtime = tokio::runtime::Builder::new_multi_thread();
    if let Some(n) = cfg.server.worker_threads {
        runtime.worker_threads(n);
    }
    runtime
        .enable_all()
        .thread_stack_size(8 * 1024 * 1024)
        .max_blocking_threads(cfg.render.decode_concurrency * 2 + cfg.fetch.workers * 2 + 16)
        .build()?
        .block_on(async move {
            if let Some(list) = domain_list {
                prerender_domains(cfg, &list).await
            } else if prerender {
                prerender_catalogs(cfg).await
            } else {
                run(cfg).await
            }
        })
}

async fn prerender_domains(cfg: config::Config, list: &std::path::Path) -> anyhow::Result<()> {
    let app = app::App::new(cfg)?;
    anyhow::ensure!(
        app.store.has_backend(),
        "prerender-domains needs cache.storage to write into"
    );
    let text = std::fs::read_to_string(list)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", list.display()))?;
    let domains = app.normalize_list(&text);
    anyhow::ensure!(!domains.is_empty(), "{} lists no domains", list.display());
    if let Some(catalog) = &app.catalog {
        catalog.reload(&app.list_client_ref()).await;
        anyhow::ensure!(catalog.len() > 0, "thesvg catalog did not load");
    }
    let parallel = app
        .cfg
        .fetch
        .workers
        .max(app.cfg.render.decode_concurrency * 4);
    app.prerender_domains(domains, parallel).await;
    Ok(())
}

async fn prerender_catalogs(cfg: config::Config) -> anyhow::Result<()> {
    let app = app::App::new(cfg)?;
    anyhow::ensure!(
        app.store.has_backend(),
        "prerender needs cache.storage to write into"
    );
    let jobs: Vec<_> = app
        .slugs
        .all()
        .iter()
        .cloned()
        .map(|catalog| {
            let app = app.clone();
            let parallel = app.cfg.render.decode_concurrency * 2;
            tokio::spawn(async move {
                catalog.reload(&app.list_client_ref()).await;
                anyhow::ensure!(catalog.is_loaded(), "catalog {} did not load", catalog.id());
                app.prerender(&catalog, true, parallel).await;
                Ok(())
            })
        })
        .collect();
    for job in jobs {
        job.await??;
    }
    Ok(())
}

async fn run(cfg: config::Config) -> anyhow::Result<()> {
    let app = app::App::new(cfg)?;
    if !app.store.has_backend() {
        tracing::warn!(
            "cache.storage = none: icons are kept only in memory and refetched after restart"
        );
    }
    match app.cfg.render.sandbox {
        config::SandboxMode::Off => {
            tracing::warn!("render.sandbox = off: untrusted images are decoded in-process")
        }
        config::SandboxMode::Process if !cfg!(target_os = "linux") => tracing::warn!(
            "seccomp is Linux-only: render workers here are isolated by process and rlimits only"
        ),
        _ => {}
    }
    app.spawn_policy_reloader();
    app.spawn_catalog_reloader();
    app.spawn_slug_reloaders();
    app.spawn_workers();
    app.spawn_disk_sweeper();

    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutting down");
        let _ = tx.send(true);
    });

    if let Some(addr) = app.cfg.server.admin_listen {
        let admin = http::admin_router(app.clone());
        let mut rx = rx.clone();
        tokio::spawn(async move {
            match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => {
                    tracing::info!(%addr, "admin listening");
                    let _ = axum::serve(l, admin)
                        .with_graceful_shutdown(async move {
                            let _ = rx.changed().await;
                        })
                        .await;
                }
                Err(e) => tracing::error!(%addr, error = %e, "admin bind failed"),
            }
        });
    }

    let server_cfg = app.cfg.server.clone();
    if app.cfg.tls.enabled {
        let tls_cfg = app.cfg.tls.clone();
        let tls = tls::Tls::load(&tls_cfg, app.challenges.clone())?;
        let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(tls::server_config(
            tls.certs.clone(),
        )?));
        if tls_cfg.acme() {
            tls.spawn_acme(tls_cfg.clone(), rx.clone());
        }
        if let Some(addr) = tls_cfg.http_listen {
            let router = http::challenge_router(app.clone());
            let cfg = server_cfg.clone();
            let rx = rx.clone();
            tokio::spawn(async move {
                if let Err(e) = server::serve(router, addr, &cfg, None, rx).await {
                    tracing::error!(%addr, error = %e, "http listener failed");
                }
            });
        }
        let router = http::router(app.clone());
        let cfg = server_cfg.clone();
        let rx_tls = rx.clone();
        tokio::spawn(async move {
            if let Err(e) =
                server::serve(router, tls_cfg.listen, &cfg, Some(acceptor), rx_tls).await
            {
                tracing::error!(addr = %tls_cfg.listen, error = %e, "https listener failed");
            }
        });
    }
    server::serve(http::router(app), server_cfg.listen, &server_cfg, None, rx).await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}
