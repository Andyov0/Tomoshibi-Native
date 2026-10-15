// Connects to a WebSocket that refuses the upgrade and reports the error, so
// that whatever is listening can record what else the SDK asked for.
//   cargo run --example validate -- ws://127.0.0.1:18799
use livekit::prelude::*;

#[tokio::main]
async fn main() {
    let url = std::env::args().nth(1).unwrap_or("ws://127.0.0.1:18799".into());
    let mut options = RoomOptions::default();
    options.join_retries = 0;
    match Room::connect(&url, "not-a-token.but.shaped-like-one", options).await {
        Ok(_) => println!("connected?"),
        Err(e) => println!("refused: {e}"),
    }
}
