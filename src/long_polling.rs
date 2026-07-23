use crate::{
    client::{ClientError, GetUpdatesConflict, get_updates},
    config::Config,
    incoming::{GetUpdatesResult, Update},
};
use std::{future::Future, time::Duration};

const CONFLICT_BACKOFF: Duration = Duration::from_secs(5);
const ERROR_BACKOFF_BASE: Duration = Duration::from_secs(1);
const ERROR_BACKOFF_MAX: Duration = Duration::from_secs(30);
const ERROR_BACKOFF_MAX_SHIFT: u32 = 8;

/// Fatal condition that terminates [`run_long_polling`].
#[derive(Debug, thiserror::Error)]
pub enum LongPollingError {
    /// A webhook is registered for the bot, so `getUpdates` is refused.
    ///
    /// This runtime never registers a webhook, so an active one means updates are being
    /// delivered to an endpoint outside this process: treat the token as leaked. Retrying
    /// is pointless and would hide the incident, so polling stops instead.
    #[error(
        "getUpdates is blocked by an active webhook that this runtime never registers: \
         treat the bot token as compromised. Revoke it with @BotFather, remove the webhook \
         with deleteWebhook, then restart with the new token"
    )]
    WebhookActive {
        #[source]
        source: ClientError,
    },
}

fn error_backoff(consecutive_failures: u32) -> Duration {
    let shift = consecutive_failures
        .saturating_sub(1)
        .min(ERROR_BACKOFF_MAX_SHIFT);
    ERROR_BACKOFF_BASE
        .saturating_mul(1u32.checked_shl(shift).unwrap_or(1))
        .min(ERROR_BACKOFF_MAX)
}

/// Recommended long-polling runtime.
///
/// Pulls updates via [`get_updates`] and dispatches each accepted [`Update`] to `handler`.
/// Survives a competing `getUpdates` consumer (e.g. during a rolling deploy) by waiting five
/// seconds and retrying. Any other [`get_updates`] failure is logged and retried from the
/// same offset after an exponential backoff, so a permanently failing request cannot spin
/// the loop.
///
/// `handler` returns a future, so both async and sync user code are supported:
///
/// - async: `|update| async move { do_async(update).await }`
/// - sync:  `|update| async move { do_sync(update) }`
///
/// # Errors
/// Returns [`LongPollingError::WebhookActive`] and stops polling when the Bot API reports a
/// registered webhook — a token this runtime controls should never have one, so the loop
/// surfaces the leak instead of retrying past it.
pub async fn run_long_polling<F, Fut>(
    config: &Config,
    mut handler: F,
) -> Result<(), LongPollingError>
where
    F: FnMut(Update) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut offset: u64 = 0;
    let mut consecutive_failures: u32 = 0;
    loop {
        let updates = match get_updates(config, offset).await {
            Ok(updates) => {
                consecutive_failures = 0;
                updates
            }
            Err(err) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let backoff = match err.get_updates_conflict() {
                    Some(GetUpdatesConflict::OtherConsumer) => {
                        tracing::warn!(
                            consecutive_failures = consecutive_failures,
                            "another getUpdates consumer is active; waiting",
                        );
                        CONFLICT_BACKOFF
                    }
                    Some(GetUpdatesConflict::WebhookActive) => {
                        let fatal = LongPollingError::WebhookActive { source: err };
                        tracing::error!(offset = offset, "{fatal}");
                        return Err(fatal);
                    }
                    None => {
                        tracing::error!(
                            error = ?err,
                            offset = offset,
                            consecutive_failures = consecutive_failures,
                            "GetUpdates error: request payload getUpdates",
                        );
                        error_backoff(consecutive_failures)
                    }
                };
                tokio::time::sleep(backoff).await;
                continue;
            }
        };
        for item in updates {
            match item {
                GetUpdatesResult::Unknown { update_id, extra } => {
                    tracing::warn!(
                        update_id = %update_id,
                        "Unknown message: {:?}",
                        serde_json::to_string(&extra)
                            .unwrap_or("Cannot read json".to_string())
                    );
                    offset = u64::from(update_id) + 1;
                }
                GetUpdatesResult::Accepted { body } => {
                    tracing::debug!("Message: {:?}", body);
                    let next_offset = u64::from(body.update_id()) + 1;
                    handler(*body).await;
                    offset = next_offset;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ERROR_BACKOFF_BASE, ERROR_BACKOFF_MAX, LongPollingError, error_backoff};
    use crate::client::ClientError;
    use rstest::rstest;
    use std::time::Duration;

    #[rstest]
    #[case::first_failure(1, ERROR_BACKOFF_BASE)]
    #[case::second_failure(2, Duration::from_secs(2))]
    #[case::third_failure(3, Duration::from_secs(4))]
    #[case::capped(6, ERROR_BACKOFF_MAX)]
    #[case::capped_far(u32::MAX, ERROR_BACKOFF_MAX)]
    fn error_backoff_grows_and_caps(#[case] failures: u32, #[case] expected: Duration) {
        assert_eq!(error_backoff(failures), expected);
    }

    #[test]
    fn webhook_active_error_names_the_remediation() {
        let error = LongPollingError::WebhookActive {
            source: ClientError::Api {
                method: "getUpdates",
                code: 409,
                description: "Conflict: can't use getUpdates method while webhook is active"
                    .to_string(),
                retry_after: None,
            },
        };
        let message = error.to_string();
        assert!(message.contains("compromised"), "{message}");
        assert!(message.contains("@BotFather"), "{message}");
        assert!(message.contains("deleteWebhook"), "{message}");
    }
}
