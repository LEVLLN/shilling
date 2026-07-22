use shilling::config::ConfigBuilder;
use shilling::incoming::Update;
use shilling::long_polling::run_long_polling;

#[must_use]
async fn handle_update(update: Update, message: &str) {
    println!("Hello from update: {:?}", update);
    println!("External data from params: {}", message);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConfigBuilder::new().try_credentials_from_env()?.build()?;
    let message = "Hello, World!";
    run_long_polling(&config, |update| async {
        let _ = handle_update(update, message).await;
    })
    .await?;
    Ok(())
}
