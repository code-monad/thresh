mod config;
mod rpc;
mod utils;
use tokio;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
}
