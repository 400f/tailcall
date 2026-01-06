#![allow(clippy::too_many_arguments)]
use std::sync::Arc;

use hyper::server::conn::http2;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls_pki_types::CertificateDer;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_rustls::TlsAcceptor;

use super::server_config::ServerConfig;
use crate::core::async_graphql_hyper::{GraphQLBatchRequest, GraphQLRequest};
use crate::core::config::PrivateKey;
use crate::core::http::handle_request;

pub async fn start_http_2(
    sc: Arc<ServerConfig>,
    cert: Vec<CertificateDer<'static>>,
    key: PrivateKey,
    server_up_sender: Option<oneshot::Sender<()>>,
) -> anyhow::Result<()> {
    // Install the ring crypto provider for TLS
    let _ = rustls::crypto::ring::default_provider().install_default();

    let addr = sc.addr();
    let listener = TcpListener::bind(&addr).await?;

    // Create TLS config
    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert, key.into_inner())?;
    tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));

    super::log_launch(sc.as_ref());

    if let Some(sender) = server_up_sender {
        sender
            .send(())
            .or(Err(anyhow::anyhow!("Failed to send message")))?;
    }

    let enable_batch = sc.app_ctx.blueprint.server.enable_batch_requests;

    loop {
        let (stream, _) = listener.accept().await?;
        let tls_acceptor = tls_acceptor.clone();
        let app_ctx = sc.app_ctx.clone();

        tokio::task::spawn(async move {
            let tls_stream = match tls_acceptor.accept(stream).await {
                Ok(stream) => stream,
                Err(err) => {
                    tracing::error!("TLS handshake error: {:?}", err);
                    return;
                }
            };

            let io = TokioIo::new(tls_stream);

            let service = service_fn(move |req| {
                let app_ctx = app_ctx.clone();
                async move {
                    if enable_batch {
                        handle_request::<GraphQLBatchRequest, _>(req, app_ctx).await
                    } else {
                        handle_request::<GraphQLRequest, _>(req, app_ctx).await
                    }
                }
            });

            if let Err(err) = http2::Builder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await
            {
                tracing::debug!("Error serving connection: {:?}", err);
            }
        });
    }
}
