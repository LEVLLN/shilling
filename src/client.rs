use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use rand::random_range;
use reqwest::{Client, Url};
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Serialize};
use tracing::Instrument;

use crate::config::Config;
use crate::incoming::GetUpdatesResult;
use crate::outgoing::{OutgoingMessage, SetMessageReactionRequest};

const POOL_IDLE_PER_HOST: usize = 16;
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);

pub static TELEGRAM_CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .tcp_keepalive(TCP_KEEPALIVE)
        .pool_max_idle_per_host(POOL_IDLE_PER_HOST)
        .build()
        .unwrap_or_else(|_| Client::new())
});

const SEND_TIMEOUT: Duration = Duration::from_secs(10);
const LONG_POLL_BUFFER: Duration = Duration::from_secs(5);
const MAX_ATTEMPTS: u8 = 3;
const BASE_BACKOFF: Duration = Duration::from_millis(200);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

const TERMINATED_BY_OTHER_GETUPDATES: &str = "Conflict: terminated by other getUpdates request";
const WEBHOOK_IS_ACTIVE: &str = "can't use getUpdates method while webhook is active";

/// Reason why `getUpdates` is rejected with a Bot API conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetUpdatesConflict {
    /// Another `getUpdates` consumer is polling the same bot.
    OtherConsumer,
    /// A webhook is registered, so `getUpdates` is unavailable until it is deleted.
    WebhookActive,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("url build failed for {method}")]
    Url {
        method: &'static str,
        #[source]
        source: url::ParseError,
    },
    #[error("transport failed for {method}")]
    Transport {
        method: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("http {status} for {method}: {body}")]
    Http {
        method: &'static str,
        status: u16,
        body: String,
    },
    #[error("bot api error {code} on {method}: {description}")]
    Api {
        method: &'static str,
        code: i32,
        description: String,
        retry_after: Option<u32>,
    },
}

impl ClientError {
    /// Which Bot API conflict blocks `getUpdates`, if any.
    ///
    /// Matches by description substring rather than HTTP code 409, since the code alone
    /// does not tell a competing consumer apart from a registered webhook.
    #[must_use]
    pub fn get_updates_conflict(&self) -> Option<GetUpdatesConflict> {
        match self {
            ClientError::Api { description, .. }
                if description.contains(TERMINATED_BY_OTHER_GETUPDATES) =>
            {
                Some(GetUpdatesConflict::OtherConsumer)
            }
            ClientError::Api { description, .. } if description.contains(WEBHOOK_IS_ACTIVE) => {
                Some(GetUpdatesConflict::WebhookActive)
            }
            _ => None,
        }
    }

    /// Whether this error is the Telegram Bot API conflict signaling that another
    /// `getUpdates` consumer is currently active for the same bot.
    #[must_use]
    pub fn is_terminated_by_other_getupdates(&self) -> bool {
        matches!(
            self.get_updates_conflict(),
            Some(GetUpdatesConflict::OtherConsumer)
        )
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    ok: bool,
    result: Option<T>,
    error_code: Option<i32>,
    description: Option<String>,
    parameters: Option<ResponseParameters>,
}

#[derive(Debug, Deserialize)]
struct ResponseParameters {
    retry_after: Option<u32>,
}

#[derive(Debug, Serialize)]
struct GetUpdatesBody<'a> {
    offset: u64,
    limit: i32,
    timeout: i8,
    allowed_updates: &'a [String],
}

/// Fetch updates via long-polling starting from `offset` (last seen `update_id` + 1).
///
/// # Errors
/// Returns [`ClientError`] on URL build failure, transport error after retries, non-2xx HTTP
/// outside the retry policy, or Bot API error response.
pub async fn get_updates(
    config: &Config,
    offset: u64,
) -> Result<Vec<GetUpdatesResult>, ClientError> {
    let body = GetUpdatesBody {
        offset,
        limit: config.get_updates_limit(),
        timeout: config.get_updates_timeout(),
        allowed_updates: config.allowed_updates(),
    };
    let timeout = long_poll_timeout(config.get_updates_timeout());
    let span = tracing::info_span!("tg_request", method = "getUpdates", offset);
    request(config, "getUpdates", &body, timeout)
        .instrument(span)
        .await
}

/// Set an emoji reaction on a message.
///
/// # Errors
/// Returns [`ClientError`] on URL build failure, transport error after retries, non-2xx HTTP
/// outside the retry policy, or Bot API error response.
pub async fn set_message_reaction(
    config: &Config,
    body: SetMessageReactionRequest,
) -> Result<(), ClientError> {
    let span = tracing::info_span!(
        "tg_request",
        method = "setMessageReaction",
        chat_id = %body.chat_id(),
        message_id = %body.message_id(),
    );
    async move {
        let _: IgnoredAny = request(config, "setMessageReaction", &body, SEND_TIMEOUT).await?;
        Ok(())
    }
    .instrument(span)
    .await
}

/// Send a single outgoing message of any media type.
///
/// # Errors
/// Returns [`ClientError`] on URL build failure, transport error after retries, non-2xx HTTP
/// outside the retry policy, or Bot API error response.
pub async fn send_from_response_message(
    config: &Config,
    response_message: &OutgoingMessage,
) -> Result<(), ClientError> {
    let method = response_message.method_name();
    let span = tracing::info_span!(
        "tg_request",
        method = method,
        chat_id = %response_message.chat_id(),
    );
    async move {
        let _: IgnoredAny = request(config, method, response_message, SEND_TIMEOUT).await?;
        Ok(())
    }
    .instrument(span)
    .await
}

pub async fn send_response_messages_sequence(
    config: &Config,
    response_messages: Vec<OutgoingMessage>,
) {
    for response_message in response_messages {
        if let Err(err) = send_from_response_message(config, &response_message).await {
            tracing::error!(?err, "send outgoing message failed");
        }
    }
}

async fn request<Req, Resp>(
    config: &Config,
    method: &'static str,
    body: &Req,
    timeout: Duration,
) -> Result<Resp, ClientError>
where
    Req: Serialize + ?Sized,
    Resp: DeserializeOwned,
{
    let url = config
        .url()
        .join(method)
        .map_err(|source| ClientError::Url { method, source })?;

    let start = Instant::now();
    let mut attempt: u8 = 0;
    loop {
        attempt += 1;
        let result = send_once::<Resp>(&url, body, timeout)
            .await
            .map_err(|err| err.into_client_error(method));

        match result {
            Ok(value) => {
                tracing::info!(
                    attempt,
                    duration_ms = elapsed_ms(start),
                    "telegram request ok"
                );
                return Ok(value);
            }
            Err(err) => match retry_delay(&err, attempt) {
                Some(delay) if attempt < MAX_ATTEMPTS => {
                    tracing::warn!(
                        attempt,
                        delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                        "telegram request retry: {err}"
                    );
                    tokio::time::sleep(delay).await;
                }
                _ => {
                    tracing::error!(
                        attempt,
                        duration_ms = elapsed_ms(start),
                        "telegram request failed: {err}"
                    );
                    return Err(err);
                }
            },
        }
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

enum AttemptError {
    Transport(reqwest::Error),
    Http {
        status: u16,
        body: String,
    },
    Api {
        code: i32,
        description: String,
        retry_after: Option<u32>,
    },
}

impl AttemptError {
    fn into_client_error(self, method: &'static str) -> ClientError {
        match self {
            AttemptError::Transport(source) => ClientError::Transport { method, source },
            AttemptError::Http { status, body } => ClientError::Http {
                method,
                status,
                body,
            },
            AttemptError::Api {
                code,
                description,
                retry_after,
            } => ClientError::Api {
                method,
                code,
                description,
                retry_after,
            },
        }
    }
}

async fn send_once<Resp>(
    url: &Url,
    body: &(impl Serialize + ?Sized),
    timeout: Duration,
) -> Result<Resp, AttemptError>
where
    Resp: DeserializeOwned,
{
    let response = TELEGRAM_CLIENT
        .post(url.clone())
        .json(body)
        .timeout(timeout)
        .send()
        .await
        .map_err(AttemptError::Transport)?;
    let status = response.status().as_u16();
    let bytes = response.bytes().await.map_err(AttemptError::Transport)?;

    match serde_json::from_slice::<ApiResponse<Resp>>(&bytes) {
        Ok(envelope) if envelope.ok => envelope.result.ok_or(AttemptError::Api {
            code: 0,
            description: "bot api returned ok=true without result".to_string(),
            retry_after: None,
        }),
        Ok(envelope) => Err(AttemptError::Api {
            code: envelope.error_code.unwrap_or(i32::from(status)),
            description: envelope.description.unwrap_or_default(),
            retry_after: envelope.parameters.and_then(|p| p.retry_after),
        }),
        Err(_) => Err(AttemptError::Http {
            status,
            body: String::from_utf8_lossy(&bytes).into_owned(),
        }),
    }
}

fn retry_delay(err: &ClientError, attempt: u8) -> Option<Duration> {
    match err {
        ClientError::Transport { .. } => Some(backoff(attempt)),
        ClientError::Http { status, .. } => {
            if (500..600).contains(status) {
                Some(backoff(attempt))
            } else {
                None
            }
        }
        ClientError::Api {
            code, retry_after, ..
        } => {
            if *code == 429 {
                Some(
                    retry_after
                        .map_or_else(|| backoff(attempt), |s| Duration::from_secs(u64::from(s))),
                )
            } else if (500..600).contains(code) {
                Some(backoff(attempt))
            } else {
                None
            }
        }
        ClientError::Url { .. } => None,
    }
}

fn backoff(attempt: u8) -> Duration {
    let shift = attempt.saturating_sub(1).min(8);
    let multiplier = 1u32.checked_shl(u32::from(shift)).unwrap_or(1);
    let exp = BASE_BACKOFF.saturating_mul(multiplier);
    let capped = exp.min(MAX_BACKOFF);
    let capped_ms = u64::try_from(capped.as_millis()).unwrap_or(u64::MAX);
    let jitter_cap = capped_ms / 2 + 1;
    let jitter = random_range(0..jitter_cap);
    capped + Duration::from_millis(jitter)
}

fn long_poll_timeout(get_updates_timeout: i8) -> Duration {
    let secs = u64::try_from(get_updates_timeout).unwrap_or(0);
    Duration::from_secs(secs) + LONG_POLL_BUFFER
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, 200, 301)]
    #[case(1, 200, 301)]
    #[case(2, 400, 601)]
    #[case(3, 800, 1201)]
    #[case(4, 1600, 2401)]
    #[case(5, 3200, 4801)]
    #[case(6, 5000, 7501)]
    #[case(9, 5000, 7501)]
    #[case(u8::MAX, 5000, 7501)]
    fn backoff_within_expected_bounds(
        #[case] attempt: u8,
        #[case] min_ms: u64,
        #[case] max_exclusive_ms: u64,
    ) {
        let min = Duration::from_millis(min_ms);
        let max_exclusive = Duration::from_millis(max_exclusive_ms);
        for _ in 0..1024 {
            let delay = backoff(attempt);
            assert!(delay >= min, "attempt={attempt}: {delay:?} < {min:?}");
            assert!(
                delay < max_exclusive,
                "attempt={attempt}: {delay:?} >= {max_exclusive:?}"
            );
        }
    }

    fn assert_attempt_1_backoff(delay: Duration) {
        assert!(delay >= Duration::from_millis(200), "{delay:?} < 200ms");
        assert!(delay < Duration::from_millis(301), "{delay:?} >= 301ms");
    }

    #[rstest]
    #[case(400)]
    #[case(404)]
    #[case(418)]
    #[case(499)]
    fn retry_delay_http_4xx_returns_none(#[case] status: u16) {
        let err = ClientError::Http {
            method: "test",
            status,
            body: String::new(),
        };
        assert!(retry_delay(&err, 1).is_none());
    }

    #[rstest]
    #[case(500)]
    #[case(502)]
    #[case(503)]
    #[case(599)]
    fn retry_delay_http_5xx_uses_backoff(#[case] status: u16) {
        let err = ClientError::Http {
            method: "test",
            status,
            body: String::new(),
        };
        assert_attempt_1_backoff(retry_delay(&err, 1).unwrap());
    }

    #[rstest]
    #[case(400)]
    #[case(401)]
    #[case(403)]
    #[case(404)]
    fn retry_delay_api_4xx_returns_none(#[case] code: i32) {
        let err = ClientError::Api {
            method: "test",
            code,
            description: String::new(),
            retry_after: None,
        };
        assert!(retry_delay(&err, 1).is_none());
    }

    #[rstest]
    #[case(500)]
    #[case(502)]
    #[case(503)]
    #[case(599)]
    fn retry_delay_api_5xx_uses_backoff(#[case] code: i32) {
        let err = ClientError::Api {
            method: "test",
            code,
            description: String::new(),
            retry_after: None,
        };
        assert_attempt_1_backoff(retry_delay(&err, 1).unwrap());
    }

    #[rstest]
    #[case(1, 1_000)]
    #[case(5, 5_000)]
    #[case(42, 42_000)]
    fn retry_delay_api_429_honors_retry_after(#[case] seconds: u32, #[case] expected_ms: u64) {
        let err = ClientError::Api {
            method: "test",
            code: 429,
            description: String::new(),
            retry_after: Some(seconds),
        };
        assert_eq!(
            retry_delay(&err, 1),
            Some(Duration::from_millis(expected_ms))
        );
    }

    #[test]
    fn retry_delay_api_429_without_retry_after_uses_backoff() {
        let err = ClientError::Api {
            method: "test",
            code: 429,
            description: String::new(),
            retry_after: None,
        };
        assert_attempt_1_backoff(retry_delay(&err, 1).unwrap());
    }

    #[test]
    fn retry_delay_url_returns_none() {
        let source = Url::parse("not a url").unwrap_err();
        let err = ClientError::Url {
            method: "test",
            source,
        };
        assert!(retry_delay(&err, 1).is_none());
    }

    #[rstest]
    #[case::full_message(
        "Conflict: terminated by other getUpdates request; make sure that only one bot instance is running",
        true
    )]
    #[case::core_phrase("Conflict: terminated by other getUpdates request", true)]
    #[case::other_409("Conflict: can't use getUpdates method while webhook is active", false)]
    #[case::empty("", false)]
    fn is_terminated_by_other_getupdates_matches_description(
        #[case] description: &str,
        #[case] expected: bool,
    ) {
        let err = ClientError::Api {
            method: "getUpdates",
            code: 409,
            description: description.to_string(),
            retry_after: None,
        };
        assert_eq!(err.is_terminated_by_other_getupdates(), expected);
    }

    #[rstest]
    #[case::other_consumer(
        "Conflict: terminated by other getUpdates request; make sure that only one bot instance is running",
        Some(GetUpdatesConflict::OtherConsumer)
    )]
    #[case::webhook(
        "Conflict: can't use getUpdates method while webhook is active; use deleteWebhook to delete the webhook first",
        Some(GetUpdatesConflict::WebhookActive)
    )]
    #[case::unauthorized("Unauthorized", None)]
    #[case::empty("", None)]
    fn get_updates_conflict_matches_description(
        #[case] description: &str,
        #[case] expected: Option<GetUpdatesConflict>,
    ) {
        let err = ClientError::Api {
            method: "getUpdates",
            code: 409,
            description: description.to_string(),
            retry_after: None,
        };
        assert_eq!(err.get_updates_conflict(), expected);
    }

    #[test]
    fn get_updates_conflict_ignores_non_api_errors() {
        let err = ClientError::Http {
            method: "getUpdates",
            status: 409,
            body: "Conflict: can't use getUpdates method while webhook is active".to_string(),
        };
        assert_eq!(err.get_updates_conflict(), None);
    }

    #[test]
    fn retry_delay_transport_uses_backoff() {
        let source = Client::new().get("not a url").build().unwrap_err();
        let err = ClientError::Transport {
            method: "test",
            source,
        };
        assert_attempt_1_backoff(retry_delay(&err, 1).unwrap());
    }
}
