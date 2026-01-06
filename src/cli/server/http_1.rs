use std::sync::Arc;

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::server_config::ServerConfig;
use crate::core::async_graphql_hyper::{GraphQLBatchRequest, GraphQLRequest};
use crate::core::http::handle_request;

pub async fn start_http_1(
    sc: Arc<ServerConfig>,
    server_up_sender: Option<oneshot::Sender<()>>,
) -> anyhow::Result<()> {
    let addr = sc.addr();
    let listener = TcpListener::bind(&addr).await?;

    super::log_launch(sc.as_ref());

    if let Some(sender) = server_up_sender {
        sender
            .send(())
            .or(Err(anyhow::anyhow!("Failed to send message")))?;
    }

    let enable_batch = sc.app_ctx.blueprint.server.enable_batch_requests;
    let pipeline_flush = sc.app_ctx.blueprint.server.pipeline_flush;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let app_ctx = sc.app_ctx.clone();

        tokio::task::spawn(async move {
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

            let mut conn = http1::Builder::new();
            conn.pipeline_flush(pipeline_flush);

            if let Err(err) = conn.serve_connection(io, service).await {
                tracing::debug!("Error serving connection: {:?}", err);
            }
        });
    }
}
