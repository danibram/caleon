#[tokio::main]
async fn main() {
    if let Err(e) = caleon::run().await {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
