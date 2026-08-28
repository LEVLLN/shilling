use std::fmt;

use url::Url;

const DEFAULT_BASE_URL: &str = "https://api.telegram.org";
const DEFAULT_GET_UPDATES_TIMEOUT: i8 = 50;
const DEFAULT_GET_UPDATES_LIMIT: i32 = 100;
const DEFAULT_ALLOWED_UPDATES: &[&str] = &["message", "edited_message", "callback_query"];

/// Stands in for the bot token wherever a URL or a config is rendered for humans.
const REDACTED_TOKEN: &str = "REDACTED";

#[derive(Debug, thiserror::Error)]
pub enum TelegramConfigError {
    #[error("environment variable {0} is not set")]
    MissingEnv(&'static str),
    #[error("telegram config field {0} is empty")]
    EmptyField(&'static str),
    #[error("invalid telegram base URL: {0}")]
    InvalidBaseUrl(#[from] url::ParseError),
    #[error("get_updates timeout out of range [0, 50]: {0}")]
    InvalidGetUpdatesTimeout(i8),
    #[error("get_updates limit out of range [1, 100]: {0}")]
    InvalidGetUpdatesLimit(i32),
    #[error("allowed_updates must not be empty")]
    InvalidAllowedUpdates,
}

#[derive(Clone)]
pub struct Config {
    url: Url,
    redacted_url: Url,
    get_updates_timeout: i8,
    get_updates_limit: i32,
    allowed_updates: Vec<String>,
}

/// Renders the token-free URL, so `{config:?}` in a log line cannot leak credentials.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("url", &self.redacted_url.as_str())
            .field("get_updates_timeout", &self.get_updates_timeout)
            .field("get_updates_limit", &self.get_updates_limit)
            .field("allowed_updates", &self.allowed_updates)
            .finish()
    }
}

impl Config {
    /// Base URL including the bot token. Never log this; use [`Config::redacted_url`] instead.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Same URL as [`Config::url`] with the bot token replaced by a placeholder.
    ///
    /// Safe to log, and structurally identical to the real URL, so it can stand in for it in
    /// error messages without losing the scheme, host, port, or bot id.
    #[must_use]
    pub fn redacted_url(&self) -> &Url {
        &self.redacted_url
    }

    #[must_use]
    pub fn get_updates_timeout(&self) -> i8 {
        self.get_updates_timeout
    }

    #[must_use]
    pub fn get_updates_limit(&self) -> i32 {
        self.get_updates_limit
    }

    #[must_use]
    pub fn allowed_updates(&self) -> &[String] {
        &self.allowed_updates
    }
}

#[derive(Clone)]
struct BuilderState {
    bot_id: String,
    bot_token: String,
    base_url: String,
    get_updates_timeout: i8,
    get_updates_limit: i32,
    allowed_updates: Vec<String>,
}

impl fmt::Debug for BuilderState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuilderState")
            .field("bot_id", &self.bot_id)
            .field("bot_token", &REDACTED_TOKEN)
            .field("base_url", &self.base_url)
            .field("get_updates_timeout", &self.get_updates_timeout)
            .field("get_updates_limit", &self.get_updates_limit)
            .field("allowed_updates", &self.allowed_updates)
            .finish()
    }
}

impl Default for BuilderState {
    fn default() -> Self {
        Self {
            bot_id: String::new(),
            bot_token: String::new(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            get_updates_timeout: DEFAULT_GET_UPDATES_TIMEOUT,
            get_updates_limit: DEFAULT_GET_UPDATES_LIMIT,
            allowed_updates: DEFAULT_ALLOWED_UPDATES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        }
    }
}

#[derive(Default)]
pub struct ConfigBuilder {
    state: BuilderState,
}

impl ConfigBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    #[must_use]
    pub fn bot_id(mut self, value: impl Into<String>) -> Self {
        self.state.bot_id = value.into();
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn bot_token(mut self, value: impl Into<String>) -> Self {
        self.state.bot_token = value.into();
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn base_url(mut self, value: impl Into<String>) -> Self {
        self.state.base_url = value.into();
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn get_updates_timeout(mut self, value: i8) -> Self {
        self.state.get_updates_timeout = value;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn get_updates_limit(mut self, value: i32) -> Self {
        self.state.get_updates_limit = value;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn allowed_updates<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.state.allowed_updates = values.into_iter().map(Into::into).collect();
        self
    }

    /// # Errors
    /// Returns [`TelegramConfigError::MissingEnv`] when `TELEGRAM_BOT_ID` or `TELEGRAM_BOT_TOKEN`
    /// is not present in the process environment.
    pub fn try_credentials_from_env(mut self) -> Result<Self, TelegramConfigError> {
        self.state.bot_id = std::env::var("TELEGRAM_BOT_ID")
            .map_err(|_| TelegramConfigError::MissingEnv("TELEGRAM_BOT_ID"))?;
        self.state.bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| TelegramConfigError::MissingEnv("TELEGRAM_BOT_TOKEN"))?;
        Ok(self)
    }

    /// # Errors
    /// Returns a [`TelegramConfigError`] variant when any field fails validation: missing
    /// credentials, malformed base URL, out-of-range get_updates limits, or empty
    /// `allowed_updates`.
    pub fn build(self) -> Result<Config, TelegramConfigError> {
        let BuilderState {
            bot_id,
            bot_token,
            base_url,
            get_updates_timeout,
            get_updates_limit,
            allowed_updates,
        } = self.state;
        if bot_id.is_empty() {
            return Err(TelegramConfigError::EmptyField("bot_id"));
        }
        if bot_token.is_empty() {
            return Err(TelegramConfigError::EmptyField("bot_token"));
        }
        if !(0..=50).contains(&get_updates_timeout) {
            return Err(TelegramConfigError::InvalidGetUpdatesTimeout(
                get_updates_timeout,
            ));
        }
        if !(1..=100).contains(&get_updates_limit) {
            return Err(TelegramConfigError::InvalidGetUpdatesLimit(
                get_updates_limit,
            ));
        }
        if allowed_updates.is_empty() {
            return Err(TelegramConfigError::InvalidAllowedUpdates);
        }
        let base = Url::parse(&base_url).map_err(TelegramConfigError::InvalidBaseUrl)?;
        let url = base
            .join(&format!("/{bot_id}:{bot_token}/"))
            .map_err(TelegramConfigError::InvalidBaseUrl)?;
        let redacted_url = base
            .join(&format!("/{bot_id}:{REDACTED_TOKEN}/"))
            .map_err(TelegramConfigError::InvalidBaseUrl)?;
        Ok(Config {
            url,
            redacted_url,
            get_updates_timeout,
            get_updates_limit,
            allowed_updates,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::ConfigBuilder;
    use insta::assert_debug_snapshot;
    use rstest::rstest;

    fn valid() -> ConfigBuilder {
        ConfigBuilder::new().bot_id("12345").bot_token("AAAA:bbbbb")
    }

    #[rstest]
    #[case("empty_bot_id", ConfigBuilder::new().bot_token("AAAA:bbbbb"))]
    #[case("empty_bot_token", ConfigBuilder::new().bot_id("12345"))]
    #[case("valid_defaults", valid())]
    #[case("timeout_negative", valid().get_updates_timeout(-1))]
    #[case("timeout_above_max", valid().get_updates_timeout(51))]
    #[case("timeout_lower_bound", valid().get_updates_timeout(0))]
    #[case("timeout_upper_bound", valid().get_updates_timeout(50))]
    #[case("limit_zero", valid().get_updates_limit(0))]
    #[case("limit_above_max", valid().get_updates_limit(101))]
    #[case("limit_lower_bound", valid().get_updates_limit(1))]
    #[case("limit_upper_bound", valid().get_updates_limit(100))]
    #[case("empty_allowed_updates", valid().allowed_updates(Vec::<String>::new()))]
    #[case("invalid_base_url", valid().base_url("not a url"))]
    #[case(
        "valid_full",
        valid()
            .base_url("https://example.com")
            .get_updates_timeout(30)
            .get_updates_limit(50)
            .allowed_updates(["message", "callback_query"])
    )]
    fn build_cases(#[case] name: &str, #[case] builder: ConfigBuilder) {
        assert_debug_snapshot!(name, builder.build());
    }

    #[test]
    fn debug_hides_the_bot_token_but_keeps_the_url_usable() {
        let config = ConfigBuilder::new()
            .bot_id("12345")
            .bot_token("s3cr3t-bot-token")
            .build()
            .unwrap();

        let rendered = format!("{config:?}");
        assert!(!rendered.contains("s3cr3t-bot-token"), "{rendered}");
        assert_eq!(
            config.redacted_url().as_str(),
            "https://api.telegram.org/12345:REDACTED/"
        );
        assert!(config.url().as_str().contains("s3cr3t-bot-token"));
    }
}
