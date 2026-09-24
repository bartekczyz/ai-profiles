//! Accepting connections: drop peers outside `allow_from` before any TLS,
//! then speak HTTP/1.1 over TLS 1.3 to the rest. However many connections
//! are opened, only so many are kept, and one that sends nothing is closed.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;

use crate::routes::{router, PeerAddr, ServerState};

/// How long a peer gets to finish the TLS handshake.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a connection gets to send a request's headers, including while
/// it waits between requests: an idle connection is closed after this.
const HEADER_TIMEOUT: Duration = Duration::from_secs(60);
/// How many connections are open at once, at most; more are closed at once.
/// The Mac opens a handful.
const MAX_CONNECTIONS: usize = 256;
/// How long to wait before accepting again after accepting failed, as it
/// does when the server is out of file descriptors.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// Serve until `shutdown` resolves. Connections already open finish their
/// current request.
pub async fn serve(
    state: Arc<ServerState>,
    listener: TcpListener,
    tls: Arc<rustls::ServerConfig>,
    shutdown: impl std::future::Future<Output = ()>,
) -> io::Result<()> {
    let app = router(state.clone());
    let acceptor = TlsAcceptor::from(tls);
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    tokio::pin!(shutdown);
    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(accepted) => accepted,
                Err(error) => {
                    eprintln!("accept failed: {error}");
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                    continue;
                }
            },
            () = &mut shutdown => return Ok(()),
        };
        if !state.config.allows(peer.ip()) {
            // Not even a TLS alert: to them the port may as well be closed.
            drop(stream);
            continue;
        }
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            drop(stream);
            continue;
        };
        let acceptor = acceptor.clone();
        let service = TowerToHyperService::new(app.clone().layer(Extension(PeerAddr(peer))));
        tokio::spawn(async move {
            let _slot = slot;
            let Ok(Ok(tls_stream)) =
                tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await
            else {
                return;
            };
            let _ = hyper::server::conn::http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(HEADER_TIMEOUT)
                .serve_connection(TokioIo::new(tls_stream), service)
                .await;
        });
    }
}
