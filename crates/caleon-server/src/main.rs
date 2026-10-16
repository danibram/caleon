#[tokio::main]
async fn main() {
    if let Err(e) = caleon_server::run().await {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
