use shilling::client::send_from_response_message;
use shilling::config::ConfigBuilder;
use shilling::ids::ChatId;
use shilling::outgoing::OutgoingMessage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConfigBuilder::new().try_credentials_from_env()?.build()?;
    let chat_id = ChatId::new(std::env::var("TELEGRAM_CHAT_ID")?.parse::<i64>()?);
    let message = OutgoingMessage::text_message_without_reply_to_message(
        "hello from shilling".to_string(),
        chat_id,
    );
    send_from_response_message(&config, &message).await?;
    Ok(())
}
