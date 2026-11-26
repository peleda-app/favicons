#[path = "../../src/config.rs"]
mod config;
#[path = "../../src/contract.rs"]
mod contract;
#[path = "../../src/discover.rs"]
mod discover;
#[path = "../../src/dns.rs"]
mod dns;
#[path = "../../src/domain.rs"]
mod domain;
#[path = "../../src/net.rs"]
mod net;
#[path = "../../src/palette.rs"]
mod palette;
#[path = "../../src/plate.rs"]
mod plate;
#[path = "../../src/policy.rs"]
mod policy;
#[path = "../../src/render.rs"]
mod render;
#[path = "../../src/sandbox.rs"]
mod sandbox;
#[path = "../../src/slugs.rs"]
mod slugs;
#[path = "../../src/trace.rs"]
mod trace;
#[path = "../../src/vector.rs"]
mod vector;

fn fuzz_scope() -> discover::Scope {
    discover::Scope {
        urls: net::UrlPolicy {
            allow_http: false,
            allow_ip_hosts: false,
            ports: vec![443],
        },
        max_inline: 64 * 1024,
    }
}

fn fuzz_job(task: sandbox::Task) -> sandbox::Job {
    sandbox::Job {
        cfg: config::Render::default(),
        task,
    }
}
