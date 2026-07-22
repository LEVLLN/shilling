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
/// Survives a [`GetUpdatesConflict`] (a competing consumer during a rolling deploy, or a
/// registered webhook) by waiting five seconds and retrying. Any other
/// [`get_updates`] failure is logged and retried from the same offset after an exponential
/// backoff, so a permanently failing request cannot spin the loop.
///
/// `handler` returns a future, so both async and sync user code are supported:
///
/// - async: `|update| async move { do_async(update).await }`
/// - sync:  `|update| async move { do_sync(update) }`
///
/// # Errors
/// Currently never returns `Err` — the function loops forever. The `Result` return type is
/// reserved for future fatal conditions.
pub async fn run_long_polling<F, Fut>(config: &Config, mut handler: F) -> Result<(), ClientError>
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
                        tracing::warn!(
                            consecutive_failures = consecutive_failures,
                            "webhook is active, getUpdates is unavailable; call deleteWebhook",
                        );
                        CONFLICT_BACKOFF
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
    use super::{ERROR_BACKOFF_BASE, ERROR_BACKOFF_MAX, error_backoff};
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
}
