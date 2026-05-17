use crate::{
    client::{ClientError, get_updates},
    config::Config,
    incoming::{GetUpdatesResult, Update},
};
use std::{future::Future, time::Duration};

const CONFLICT_BACKOFF: Duration = Duration::from_secs(5);

/// Recommended long-polling runtime.
///
/// Pulls updates via [`get_updates`] and dispatches each accepted [`Update`] to `handler`.
/// Survives the "Conflict: terminated by other getUpdates request" condition (e.g. during
/// a rolling deploy) by waiting and retrying. Any other [`get_updates`] failure is logged
/// and the loop continues from the same offset.
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
    loop {
        let updates = match get_updates(config, offset).await {
            Ok(updates) => updates,
            Err(err) if err.is_terminated_by_other_getupdates() => {
                tracing::info!("another getUpdates consumer is active; waiting");
                tokio::time::sleep(CONFLICT_BACKOFF).await;
                continue;
            }
            Err(err) => {
                tracing::error!(
                    error = ?err,
                    offset = offset,
                    "GetUpdates error: request payload getUpdates",
                );
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
