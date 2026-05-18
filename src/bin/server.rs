use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use std::error::Error;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::{Sender, channel};
use tokio_websockets::{Message, ServerBuilder, WebSocketStream};

async fn handle_connection(
    addr: SocketAddr,
    mut ws_stream: WebSocketStream<TcpStream>,
    bcast_tx: Sender<(SocketAddr, String)>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut bcast_rx = bcast_tx.subscribe();

    loop {
        tokio::select! {
            maybe_msg = ws_stream.next() => match maybe_msg {
                Some(Ok(msg)) if msg.is_text() => {
                    if let Some(text) = msg.as_text() {
                        println!("received from {addr}: {text}");
                        let _ = bcast_tx.send((addr, text.to_string()));
                    }
                }
                Some(Ok(msg)) if msg.is_close() => {
                    println!("client {addr} disconnected");
                    break;
                }
                Some(Ok(_)) => {}
                Some(Err(err)) => {
                    eprintln!("websocket error from {addr}: {err}");
                    break;
                }
                None => {
                    println!("connection closed by {addr}");
                    break;
                }
            },
            result = bcast_rx.recv() => match result {
                Ok((sender_addr, text)) => {
                    if sender_addr == addr {
                        continue;
                    }
                    // Prefix the broadcasted text with the sender's socket address
                    let tagged = format!("{}: {}", sender_addr, text);
                    ws_stream.send(Message::text(tagged)).await?;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (bcast_tx, _) = channel(16);

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("listening on port 8080");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {addr:?}");
        let bcast_tx = bcast_tx.clone();
        tokio::spawn(async move {
            // Wrap the raw TCP stream into a websocket.
            let (_req, ws_stream) = ServerBuilder::new().accept(socket).await?;

            handle_connection(addr, ws_stream, bcast_tx).await
        });
    }
}