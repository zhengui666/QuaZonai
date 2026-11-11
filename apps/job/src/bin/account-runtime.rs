#[tokio::main(flavor = "current_thread")]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("build-identity") {
        println!("{}", job::build_identity());
        return;
    }
    if let Err(error) = job::account_runtime::run().await {
        eprintln!("account native runtime stopped: {error}");
        std::process::exit(1);
    }
}
