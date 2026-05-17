use serde::{Deserialize, Serialize};

use crate::ids::{ChatId, FileId, MessageId};

/// Link preview behavior for an outgoing text message.
///
/// See <https://core.telegram.org/bots/api#linkpreviewoptions>.
#[derive(Debug, Serialize, PartialEq, Clone)]
pub struct LinkPreviewOptions {
    pub is_disabled: bool,
}

/// Inline keyboard button. Only `text` and `callback_data` are supported.
///
/// See <https://core.telegram.org/bots/api#inlinekeyboardbutton>.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct InlineButton {
    pub text: String,
    pub callback_data: String,
}

/// Inline keyboard: rows of buttons attached to a message.
///
/// See <https://core.telegram.org/bots/api#inlinekeyboardmarkup>.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct InlineKeyboard {
    pub inline_keyboard: Vec<Vec<InlineButton>>,
}

/// Common header for outgoing message methods: destination chat, optional reply target, optional inline keyboard.
///
/// Shared by `sendMessage`, `sendPhoto`, and the other `send*` methods.
/// See <https://core.telegram.org/bots/api#sendmessage>.
#[derive(Debug, Serialize, PartialEq, Clone)]
pub struct MessageTarget {
    pub chat_id: ChatId,
    pub reply_to_message_id: Option<MessageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboard>,
}

/// Request body for the `send*` family of methods: text, photo, video, voice, sticker, and other media.
///
/// Each variant maps to one Bot API method (`sendMessage`, `sendPhoto`, `sendSticker`, ...).
/// See <https://core.telegram.org/bots/api#sendmessage>.
#[derive(Debug, Serialize, PartialEq, Clone)]
#[serde(untagged)]
pub enum OutgoingMessage {
    Text {
        #[serde(flatten)]
        base_body: MessageTarget,
        text: String,
        link_preview_options: LinkPreviewOptions,
    },
    Photo {
        #[serde(flatten)]
        base_body: MessageTarget,
        photo: FileId,
        #[serde(skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    Video {
        #[serde(flatten)]
        base_body: MessageTarget,
        video: FileId,
        #[serde(skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    Voice {
        #[serde(flatten)]
        base_body: MessageTarget,
        voice: FileId,
        #[serde(skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    Audio {
        #[serde(flatten)]
        base_body: MessageTarget,
        audio: FileId,
        #[serde(skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    Document {
        #[serde(flatten)]
        base_body: MessageTarget,
        document: FileId,
        #[serde(skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    Animation {
        #[serde(flatten)]
        base_body: MessageTarget,
        animation: FileId,
        #[serde(skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
    },
    VideoNote {
        #[serde(flatten)]
        base_body: MessageTarget,
        video_note: FileId,
    },
    Sticker {
        #[serde(flatten)]
        base_body: MessageTarget,
        sticker: FileId,
    },
}

impl OutgoingMessage {
    pub fn method_name(&self) -> &'static str {
        match self {
            OutgoingMessage::Text { .. } => "sendMessage",
            OutgoingMessage::Photo { .. } => "sendPhoto",
            OutgoingMessage::Video { .. } => "sendVideo",
            OutgoingMessage::Voice { .. } => "sendVoice",
            OutgoingMessage::Audio { .. } => "sendAudio",
            OutgoingMessage::Document { .. } => "sendDocument",
            OutgoingMessage::Animation { .. } => "sendAnimation",
            OutgoingMessage::VideoNote { .. } => "sendVideoNote",
            OutgoingMessage::Sticker { .. } => "sendSticker",
        }
    }

    pub fn chat_id(&self) -> ChatId {
        match self {
            OutgoingMessage::Text { base_body, .. }
            | OutgoingMessage::Photo { base_body, .. }
            | OutgoingMessage::Video { base_body, .. }
            | OutgoingMessage::Voice { base_body, .. }
            | OutgoingMessage::Audio { base_body, .. }
            | OutgoingMessage::Document { base_body, .. }
            | OutgoingMessage::Animation { base_body, .. }
            | OutgoingMessage::VideoNote { base_body, .. }
            | OutgoingMessage::Sticker { base_body, .. } => base_body.chat_id,
        }
    }

    pub fn new(
        text: String,
        chat_id: ChatId,
        reply_to_message_id: Option<MessageId>,
        reply_markup: Option<InlineKeyboard>,
    ) -> Self {
        OutgoingMessage::Text {
            base_body: MessageTarget {
                chat_id,
                reply_to_message_id,
                reply_markup,
            },
            text,
            link_preview_options: LinkPreviewOptions { is_disabled: false },
        }
    }

    pub fn text_message(text: String, chat_id: ChatId, reply_to_message_id: MessageId) -> Self {
        Self::new(text, chat_id, Some(reply_to_message_id), None)
    }

    pub fn text_message_without_reply_to_message(text: String, chat_id: ChatId) -> Self {
        Self::new(text, chat_id, None, None)
    }
}

/// Emoji reaction. Only the `emoji` reaction type is supported (no `custom_emoji` or `paid`).
///
/// See <https://core.telegram.org/bots/api#reactiontypeemoji>.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct EmojiReaction {
    #[serde(rename(serialize = "type"))]
    reaction_type: String,
    emoji: String,
}

impl EmojiReaction {
    fn new_emoji(emoji: String) -> Self {
        Self {
            reaction_type: "emoji".to_string(),
            emoji,
        }
    }
}

/// Request body for the `setMessageReaction` method.
///
/// See <https://core.telegram.org/bots/api#setmessagereaction>.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SetMessageReactionRequest {
    message_id: MessageId,
    chat_id: ChatId,
    reaction: Vec<EmojiReaction>,
}

impl SetMessageReactionRequest {
    pub fn new(message_id: MessageId, chat_id: ChatId, emoji: String) -> Self {
        SetMessageReactionRequest {
            message_id,
            chat_id,
            reaction: vec![EmojiReaction::new_emoji(emoji)],
        }
    }

    pub fn chat_id(&self) -> ChatId {
        self.chat_id
    }

    pub fn message_id(&self) -> MessageId {
        self.message_id
    }
}
