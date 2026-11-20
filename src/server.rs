use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::ConnectInfo;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};
use tokio_rustls::TlsAcceptor;
use tower::Service;

use crate::config::Server as ServerCfg;

pub async fn serve(
    router: Router,
    addr: SocketAddr,
    cfg: &ServerCfg,
    tls: Option<TlsAcceptor>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, tls = tls.is_some(), "listening");
    let slots = Arc::new(Semaphore::new(cfg.max_connections));
    let header_timeout = Duration::from_millis(cfg.header_read_timeout_ms);

    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "accept failed");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
            _ = shutdown.changed() => break,
        };
        let Ok(permit) = slots.clone().try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let _ = stream.set_nodelay(true);
        let router = router.clone();
        let shutdown = shutdown.clone();
        let tls = tls.clone();
        tokio::spawn(async move {
            let _permit = permit;
            match tls {
                Some(acceptor) => {
                    match tokio::time::timeout(header_timeout, acceptor.accept(stream)).await {
                        Ok(Ok(stream)) => {
                            serve_io(stream, router, peer, header_timeout, shutdown).await
                        }
                        Ok(Err(e)) => tracing::debug!(error = %e, "tls handshake failed"),
                        Err(_) => tracing::debug!("tls handshake timed out"),
                    }
                }
                None => serve_io(stream, router, peer, header_timeout, shutdown).await,
            }
        });
    }

    let grace = Duration::from_millis(cfg.shutdown_grace_ms);
    let all = u32::try_from(cfg.max_connections).unwrap_or(u32::MAX);
    let _ = tokio::time::timeout(grace, slots.acquire_many(all)).await;
    Ok(())
}

async fn serve_io<I>(
    io: I,
    router: Router,
    peer: SocketAddr,
    header_timeout: Duration,
    mut shutdown: watch::Receiver<bool>,
) where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let svc = hyper::service::service_fn(move |mut req: hyper::Request<Incoming>| {
        req.extensions_mut().insert(ConnectInfo::<SocketAddr>(peer));
        let mut router = router.clone();
        async move { router.call(req).await }
    });
    let mut builder = auto::Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(header_timeout)
        .max_buf_size(32 * 1024)
        .keep_alive(true);
    builder
        .http2()
        .timer(TokioTimer::new())
        .max_concurrent_streams(64)
        .max_header_list_size(16 * 1024)
        .keep_alive_interval(Some(Duration::from_secs(30)))
        .keep_alive_timeout(Duration::from_secs(10));
    let conn = builder.serve_connection(TokioIo::new(io), svc);
    tokio::pin!(conn);
    tokio::select! {
        res = conn.as_mut() => {
            if let Err(e) = res {
                tracing::debug!(error = %e, "connection closed with error");
            }
        }
        _ = shutdown.changed() => {
            conn.as_mut().graceful_shutdown();
            let _ = conn.await;
        }
    }
}
