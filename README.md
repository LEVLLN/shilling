# shilling

A minimal, typed Telegram Bot API client in Rust focused on long-polling, structured updates, and reliable outgoing calls.

## Philosophy

`shilling` does not try to be a flagship Telegram Bot API binding — those already exist and cover the full API surface excellently. My take is different. The Bot API has been piled high with features over the years, and the libraries chasing that surface grow accordingly. This crate settles on the minimal, time-tested core: pull a message off the long-polling stream, do something with it, send a reply. That's the loop and that's the library.

Bot commands (`/start`, `/help`, …) are intentionally not modeled. Nothing beats letting the user type natural text; if you need command routing, write it in your handler.

## Name

The crate is named after **Pavel Lvovich Schilling** (1786–1837), a Russian scientist and diplomat who built the first practical **electromagnetic telegraph** in 1832 — the earliest device to transmit coded messages over a wire and the conceptual ancestor of every messaging protocol that followed, Telegram included.

## Features

- Long-polling via `getUpdates` with bounded timeouts and a configurable allowed-updates filter.
- Strongly-typed incoming model: `Update`, `Message`, `CallbackQuery`, media payloads, inline keyboards.
- Strongly-typed outgoing model: `sendMessage`, `sendPhoto`, `sendVideo`, `sendVoice`, `sendAudio`, `sendDocument`, `sendAnimation`, `sendVideoNote`, `sendSticker`, `setMessageReaction`.
- Newtype IDs (`ChatId`, `MessageId`, `UserId`, `UpdateId`, `FileId`, `Username`, …) — no stringly-typed identifiers.
- Heuristic sender classification (`UserAccountType`: person, bot, channel publisher, channel group, foreign channel).
- Retry policy with exponential backoff, jitter, and Bot API `retry_after` handling for transport errors, 5xx, and 429.
- Per-request `reqwest::Client` tuned for HTTP keep-alive and idle-pool reuse, shared via `once_cell`.
- `tracing` spans on every Bot API call (`method`, `chat_id`, attempt, duration).
- Builder-validated `Config` — invalid combinations are rejected before any I/O.

## Requirements

- Rust stable, edition 2024.
- Tokio multi-thread runtime.
- A Telegram bot token from [@BotFather](https://t.me/BotFather).

## Configuration

The client reads credentials from the environment:

| Variable              | Required | Description                                              |
|-----------------------|----------|----------------------------------------------------------|
| `TELEGRAM_BOT_ID`     | yes      | Numeric bot id (the part before `:` in the bot token).   |
| `TELEGRAM_BOT_TOKEN`  | yes      | Bot token (the part after `:`).                          |

Defaults applied by `ConfigBuilder`:

| Field                  | Default                                          |
|------------------------|--------------------------------------------------|
| `base_url`             | `https://api.telegram.org`                       |
| `get_updates_timeout`  | `50` seconds (range `0..=50`)                    |
| `get_updates_limit`    | `100` (range `1..=100`)                          |
| `allowed_updates`      | `["message", "edited_message", "callback_query"]`|

## Usage

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
shilling = { git = "https://github.com/<owner>/shilling" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Setting a reaction:

```rust
use shilling::client::set_message_reaction;
use shilling::outgoing::SetMessageReactionRequest;

set_message_reaction(
    &config,
    SetMessageReactionRequest::new(message_id, chat_id, "👍".to_string()),
)
.await?;
```

## Examples

Runnable examples live in [`examples/`](examples). All of them call
`ConfigBuilder::try_credentials_from_env()`, which reads the bot credentials
from the process environment.

### Environment variables

| Variable             | `send_message` | `long_polling` | Description                                            |
|----------------------|:--------------:|:--------------:|--------------------------------------------------------|
| `TELEGRAM_BOT_ID`    |       ✓        |       ✓        | Numeric bot id (the part before `:` in the bot token). |
| `TELEGRAM_BOT_TOKEN` |       ✓        |       ✓        | Bot token (the part after `:`).                        |
| `TELEGRAM_CHAT_ID`   |       ✓        |                | Destination chat id for the outgoing message.          |

### Available examples

| Example                                              | What it does                                                                                                  | Run                                       |
|------------------------------------------------------|---------------------------------------------------------------------------------------------------------------|-------------------------------------------|
| [`send_message`](examples/send_message.rs)           | Sends one text message to `TELEGRAM_CHAT_ID`.                                                                  | `cargo run --example send_message`        |
| [`long_polling`](examples/long_polling.rs)           | Runs the `run_long_polling` runtime with a placeholder handler — replace its body with your own sync/async code. | `cargo run --example long_polling`        |

### Running

```sh
export TELEGRAM_BOT_ID=123456
export TELEGRAM_BOT_TOKEN=XXXX

# long_polling: only the two variables above are needed
cargo run --example long_polling

# send_message: additionally requires the destination chat id
export TELEGRAM_CHAT_ID=987654321
cargo run --example send_message
```

## Reliability

- Every outgoing call has an explicit timeout (`10s` for send/reactions; `get_updates_timeout + 5s` for long-polling).
- Up to `3` attempts per request.
- Backoff is exponential (`200ms` base, capped at `5s`) with up to 50% jitter.
- `429` responses honor `parameters.retry_after`.
- `4xx` (HTTP or Bot API) are non-retryable and propagated to the caller.

## Errors

All public client functions return `Result<_, ClientError>`. `ClientError` is exhaustive:

| Variant      | When it is returned                                    |
|--------------|--------------------------------------------------------|
| `Url`        | URL building failed for the method.                    |
| `Transport`  | Network/transport failure after retries.               |
| `Http`       | Non-2xx HTTP outside the retry policy.                 |
| `Api`        | Bot API returned `ok = false` (carries code and text). |

`ConfigBuilder::build` returns `TelegramConfigError` for missing credentials, malformed base URL, or out-of-range polling parameters.

## Project layout

```
src/
  client.rs    long-polling, retries, request/response envelope
  config.rs    typed config + validating builder
  ids.rs       newtype IDs (string and integer)
  incoming.rs  Update / Message / CallbackQuery / media payloads
  outgoing.rs  send* request bodies and setMessageReaction
  lib.rs       public re-exports
```

## Testing

```sh
cargo test
```

Tests use `rstest` for parameterization and `insta` for snapshot assertions (config validation, payload shapes, retry-policy boundaries).

## License

MIT — see [LICENSE](LICENSE).
