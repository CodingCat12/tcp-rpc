use protocol::{Request, Response};

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use rustyline::Editor;
use rustyline::error::ReadlineError;

use bincode::config::BigEndian;
const BINCODE_CONFIG: bincode::config::Configuration<BigEndian> =
    bincode::config::standard().with_big_endian();

type Result<T, E = anyhow::Error> = core::result::Result<T, E>;

use macros::{request, rpc};

#[rpc(response = "AppResponse")]
#[serde(tag = "type")]
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

#[tokio::main]
async fn main() -> Result<()> {
    let addr = "127.0.0.1:8080";
    let stream = TcpStream::connect(addr).await?;
    let codec = LengthDelimitedCodec::new();
    let mut framed = Framed::new(stream, codec);

    let mut rl = Editor::<(), _>::new()?;

    loop {
        let input_line = match rl.readline(">> ") {
            Ok(line) => {
                rl.add_history_entry(&line)?;
                line
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                break;
            }
            Err(e) => {
                println!("Error reading line: {e}");
                break;
            }
        };

        let req: AppRequest = match json5::from_str(input_line.trim()) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse JSON input: {e}");
                continue;
            }
        };
        let req_bytes = bincode::encode_to_vec(req, BINCODE_CONFIG)?;

        framed.send(req_bytes.into()).await?;

        if let Some(resp_bytes) = framed.next().await {
            let resp_bytes = resp_bytes?;

            let resp: AppResponse =
                bincode::decode_from_slice(&resp_bytes, BINCODE_CONFIG).map(|(val, _)| val)?;
            let resp_str = json5::to_string(&resp)?;
            println!("{resp_str}");
        } else {
            println!("Server closed connection or no response received.");
            break;
        }
    }

    Ok(())
}
