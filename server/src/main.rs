use protocol::Request;

use futures::{SinkExt, StreamExt};
use macros::{request, rpc};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use tokio::sync::Notify;
use tracing_subscriber::{EnvFilter, Layer};

use std::sync::Arc;

use bytes::Bytes;

use tracing::info_span;
use tracing::{debug, error, info};

use bincode::config::BigEndian;
const BINCODE_CONFIG: bincode::config::Configuration<BigEndian> =
    bincode::config::standard().with_big_endian();

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bincode decode error: {0}")]
    BincodeDecode(#[from] bincode::error::DecodeError),

    #[error("bincode encode error: {0}")]
    BincodeEncode(#[from] bincode::error::EncodeError),

    #[error("Unexpected request format")]
    InvalidRequest,
}

type Result<T, E = Error> = ::core::result::Result<T, E>;

use std::sync::atomic::{AtomicU32, Ordering};
static CONNECTION_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

#[tokio::main]
async fn main() -> Result<()> {
    #[rpc(response = "AppResponse")]
    enum AppRequest {
        Ping(Ping),
        Pong(Pong),
        Add(Add),
    }

    #[request]
    fn Add(lhs: i32, rhs: i32) -> i32 {
        lhs + rhs
    }

    #[request]
    fn Ping() -> String {
        "You have been pinged".into()
    }

    #[request]
    fn Pong() -> String {
        "The pong has been sent".into()
    }

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let (writer, _stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .compact()
        .with_filter(EnvFilter::from_default_env());

    let logfile = tracing_appender::rolling::hourly("logs", "app.log");
    let (writer, _file_guard) = tracing_appender::non_blocking(logfile);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .json()
        .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .init();

    let addr = "127.0.0.1:8080";

    let listener = TcpListener::bind(addr)
        .await
        .inspect_err(|e| error!(%e, %addr, "failed to start server"))?;
    info!(%addr, "started server");

    let shutdown = Arc::new(Notify::new());

    {
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.unwrap_or_else(|e| {
                error!(%e, "failed to listen for ctrl+c");
            });
            info!("Received shutdown signal, shutting down gracefully...");
            shutdown.notify_one();
        });
    }

    loop {
        tokio::select! {
            Ok((socket, peer_addr)) = listener.accept() => {
                let shutdown = shutdown.clone();
                let connection_id = CONNECTION_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
                let span = info_span!("connection", %peer_addr, %connection_id);
                tokio::spawn(async move {
                    let _enter = span.enter();
                    info!("connection opened");
                    if handle_connection::<AppRequest>(socket, shutdown).await.is_err() {
                        debug!("connection task ended with error");
                    }
                    info!("connection closed");
                });
            }

            _ = shutdown.notified() => {
                info!("Shutting down server...");
                break;
            }
        }
    }

    Ok(())
}

pub async fn handle_connection<Req: Request>(
    socket: impl AsyncRead + AsyncWrite + Unpin,
    shutdown: Arc<Notify>,
) -> Result<()> {
    let codec = LengthDelimitedCodec::new();
    let mut framed = Framed::new(socket, codec);

    loop {
        tokio::select! {
            maybe_segment = framed.next() => {
                match maybe_segment.transpose().inspect_err(|e| {
                    error!(%e, "failed to get next segment")
                })? {
                    Some(segment) => {
                        let resp_bytes = handle_request::<Req>(&segment).await.inspect_err(|e| {
                            error!(%e, "failed to handle request");
                        })?;

                        framed.send(Bytes::from(resp_bytes)).await.map_err(|e| {
                            error!(%e, "failed to send response");
                            Error::Io(e)
                        })?;
                    }
                    None => { break; }
                }
            }


            _ = shutdown.notified() => {
                info!("Received shutdown signal, closing connection...");
                break;
            }
        }
    }

    framed.get_mut().shutdown().await.map_err(|e| {
        error!(%e, "error shutting down socket");
        Error::Io(e)
    })?;

    Ok(())
}

pub async fn handle_request<Req: Request>(req_bytes: &[u8]) -> Result<Vec<u8>> {
    let req: Req = bincode::decode_from_slice(req_bytes, BINCODE_CONFIG)
        .map(|(val, _)| val)
        .inspect_err(|e| error!(%e, len = req_bytes.len(), "failed to decode request"))?;
    debug!(len = req_bytes.len(), "decoded request");

    debug!(?req, "received request");
    let resp = req.handle().await;
    debug!(?resp, "sending response");

    let resp_bytes = bincode::encode_to_vec(resp, BINCODE_CONFIG)
        .inspect_err(|e| error!(%e, "failed to encode response"))?;
    debug!(len = resp_bytes.len(), "encoded response");

    Ok(resp_bytes)
}
