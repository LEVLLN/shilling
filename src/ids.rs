use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

macro_rules! string_newtype {
    ($($t:ident),* $(,)?) => {
        $(
            #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
            #[serde(transparent)]
            pub struct $t(String);

            impl $t {
                #[must_use]
                pub fn new(value: String) -> Self {
                    Self(value)
                }

                #[must_use]
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }

            impl Deref for $t {
                type Target = str;
                fn deref(&self) -> &str {
                    &self.0
                }
            }

            impl AsRef<str> for $t {
                fn as_ref(&self) -> &str {
                    &self.0
                }
            }

            impl fmt::Display for $t {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str(&self.0)
                }
            }

            impl From<String> for $t {
                fn from(value: String) -> Self {
                    Self(value)
                }
            }

            impl From<&str> for $t {
                fn from(value: &str) -> Self {
                    Self(value.to_owned())
                }
            }

            impl From<$t> for String {
                fn from(value: $t) -> Self {
                    value.0
                }
            }
        )*
    };
}

macro_rules! integer_newtype {
    ($($t:ident($inner:ty)),* $(,)?) => {
        $(
            #[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
            #[serde(transparent)]
            pub struct $t($inner);

            impl $t {
                #[must_use]
                pub fn new(value: $inner) -> Self {
                    Self(value)
                }

                #[must_use]
                pub fn value(self) -> $inner {
                    self.0
                }
            }

            impl fmt::Display for $t {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.fmt(f)
                }
            }

            impl From<$inner> for $t {
                fn from(value: $inner) -> Self {
                    Self(value)
                }
            }

            impl From<$t> for $inner {
                fn from(value: $t) -> Self {
                    value.0
                }
            }
        )*
    };
}

string_newtype!(
    Title,
    FirstName,
    LastName,
    Username,
    FileId,
    FileUniqueId,
    CallbackQueryId
);

integer_newtype!(UserId(i64), ChatId(i64), MessageId(i64), UpdateId(u64));
