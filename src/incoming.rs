use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{
    CallbackQueryId, ChatId, FileId, FileUniqueId, FirstName, LastName, MessageId, Title, UpdateId,
    UserId, Username,
};

/// Heuristic classifier of a message sender by account shape.
///
/// Not part of the Telegram Bot API.
#[derive(PartialEq)]
pub enum UserAccountType {
    /// Common account.
    Person,
    /// Account in comments of channel.
    ChannelGroup,
    /// Account of channel posts.
    ChannelPublisher,
    /// Account in comments or messages from others channels accounts
    AnotherChannel,
    /// Bot account
    Bot,
}

impl From<&User> for UserAccountType {
    fn from(value: &User) -> Self {
        match (
            value.first_name.as_deref(),
            value.username.as_deref(),
            value.is_bot,
        ) {
            (Some("Telegram"), None, false) => Self::ChannelPublisher,
            (Some("Group"), Some("GroupAnonymousBot"), true) => Self::ChannelGroup,
            (Some("Channel"), Some("Channel_Bot"), true) => Self::AnotherChannel,
            (_, _, true) => Self::Bot,
            (_, _, false) => Self::Person,
        }
    }
}

/// Telegram user.
///
/// See <https://core.telegram.org/bots/api#user>.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: UserId,
    pub is_bot: bool,
    pub first_name: Option<FirstName>,
    pub last_name: Option<LastName>,
    pub username: Option<Username>,
}

/// Type of a chat: private, group, supergroup, or channel.
///
/// Corresponds to the `type` field of [`Chat`].
/// See <https://core.telegram.org/bots/api#chat>.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ChatType {
    Private,
    Group,
    SuperGroup,
    Channel,
}

/// Chat: private chat, group, supergroup, or channel.
///
/// See <https://core.telegram.org/bots/api#chat>.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Chat {
    pub id: ChatId,
    pub title: Option<Title>,
    pub first_name: Option<FirstName>,
    pub last_name: Option<LastName>,
    pub username: Option<Username>,
    #[serde(alias = "type")]
    pub chat_type: ChatType,
}

/// File reference (`file_id` + `file_unique_id`) shared by all media payloads.
///
/// See e.g. <https://core.telegram.org/bots/api#photosize>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FileRef {
    pub file_id: FileId,
    pub file_unique_id: FileUniqueId,
}

/// Message metadata: id, sender, chat, and forward chain.
///
/// Subset of the [`Message`] object fields. See <https://core.telegram.org/bots/api#message>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageEnvelope {
    pub message_id: MessageId,
    pub from: User,
    pub chat: Chat,
    pub sender_chat: Option<Chat>,
    pub forward_from: Option<User>,
    pub forward_from_chat: Option<Chat>,
    #[serde(default)]
    pub is_automatic_forward: bool,
}

/// Message payload: text, photo, video, voice, sticker, and other media variants.
///
/// Media-bearing fields of the [`Message`] object. See <https://core.telegram.org/bots/api#message>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MessagePayload {
    Photo {
        photo: Vec<FileRef>,
        caption: Option<String>,
    },
    Text {
        text: String,
    },
    Video {
        video: FileRef,
        caption: Option<String>,
    },
    Voice {
        voice: FileRef,
        caption: Option<String>,
    },
    VideoNote {
        video_note: FileRef,
    },
    Sticker {
        sticker: FileRef,
    },
    Animation {
        animation: FileRef,
        caption: Option<String>,
    },
    Document {
        document: FileRef,
        caption: Option<String>,
    },
    Audio {
        audio: FileRef,
        caption: Option<String>,
    },
}

impl MessagePayload {
    pub fn raw_text(&self) -> Option<&str> {
        use MessagePayload::{
            Animation, Audio, Document, Photo, Sticker, Text, Video, VideoNote, Voice,
        };
        match &self {
            Photo { caption, .. }
            | Video { caption, .. }
            | Voice { caption, .. }
            | Animation { caption, .. }
            | Audio { caption, .. }
            | Document { caption, .. } => caption.as_deref(),
            Text { text, .. } => Some(text),
            VideoNote { .. } | Sticker { .. } => None,
        }
    }
}

/// Full message: metadata combined with payload.
///
/// See <https://core.telegram.org/bots/api#message>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageBody {
    #[serde(flatten)]
    pub base: MessageEnvelope,
    #[serde(flatten)]
    pub payload: MessagePayload,
}

/// Inline keyboard button. Only `text` and `callback_data` are supported.
///
/// See <https://core.telegram.org/bots/api#inlinekeyboardbutton>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplyMarkupButton {
    pub text: String,
    pub callback_data: String,
}

/// Inline keyboard: rows of buttons attached to a message.
///
/// See <https://core.telegram.org/bots/api#inlinekeyboardmarkup>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplyMarkup {
    pub inline_keyboard: Vec<Vec<ReplyMarkupButton>>,
}

/// Message wrapper distinguishing direct messages from replies, with optional inline keyboard.
///
/// Captures `reply_to_message` and `reply_markup` of the [`MessageBody`].
/// See <https://core.telegram.org/bots/api#message>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Message {
    Replied {
        #[serde(flatten)]
        direct: MessageBody,
        #[serde(alias = "reply_to_message")]
        reply: Box<MessageBody>,
        reply_markup: Option<ReplyMarkup>,
    },
    Common {
        #[serde(flatten)]
        direct: MessageBody,
        reply_markup: Option<ReplyMarkup>,
    },
}

impl Message {
    pub fn direct(&self) -> &MessageBody {
        match &self {
            Message::Common { direct, .. } | Message::Replied { direct, .. } => direct,
        }
    }
    pub fn reply(&self) -> Option<&MessageBody> {
        if let Message::Replied { reply, .. } = self {
            Some(reply)
        } else {
            None
        }
    }

    pub fn reply_markup(&self) -> &Option<ReplyMarkup> {
        match &self {
            Message::Common { reply_markup, .. } | Message::Replied { reply_markup, .. } => {
                reply_markup
            }
        }
    }
}

/// Callback produced by a press on an inline keyboard button.
///
/// See <https://core.telegram.org/bots/api#callbackquery>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CallbackQuery {
    pub id: CallbackQueryId,
    pub from: User,
    pub message: Message,
}

/// Single long-polling update: a new or edited message, or a callback query.
///
/// See <https://core.telegram.org/bots/api#update>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Update {
    Edited {
        update_id: UpdateId,
        edited_message: Message,
    },
    Origin {
        update_id: UpdateId,
        message: Message,
    },
    Callback {
        update_id: UpdateId,
        callback_query: CallbackQuery,
    },
}

impl Update {
    pub fn update_id(&self) -> UpdateId {
        match self {
            Update::Edited { update_id, .. }
            | Update::Origin { update_id, .. }
            | Update::Callback { update_id, .. } => *update_id,
        }
    }
    pub fn any_message(&self) -> &Message {
        match self {
            Update::Edited { edited_message, .. } => edited_message,
            Update::Origin { message, .. } => message,
            Update::Callback { callback_query, .. } => &callback_query.message,
        }
    }

    pub fn origin_message(&self) -> Option<&Message> {
        match self {
            Update::Origin { message, .. } => Some(message),
            _ => None,
        }
    }

    pub fn callback_query_user(&self) -> Option<&User> {
        if let Update::Callback { callback_query, .. } = self {
            Some(&callback_query.from)
        } else {
            None
        }
    }
}

/// One element of the `getUpdates` result array: a parsed update or an unknown-shape fallback.
///
/// See <https://core.telegram.org/bots/api#update>.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum GetUpdatesResult {
    Accepted {
        #[serde(flatten)]
        body: Box<Update>,
    },
    Unknown {
        update_id: UpdateId,
        #[serde(flatten)]
        extra: HashMap<String, Value>,
    },
}
