use onlinerpg_shared::messages::LocalizedMessage;

#[derive(Clone)]
pub(crate) struct PlayerMessage {
    pub message: String,
    pub localization: Option<LocalizedMessage>,
}

impl From<&str> for PlayerMessage {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

impl From<String> for PlayerMessage {
    fn from(message: String) -> Self {
        Self {
            message,
            localization: None,
        }
    }
}

impl From<&String> for PlayerMessage {
    fn from(message: &String) -> Self {
        message.as_str().into()
    }
}

pub(super) fn localized(code: &str, message: impl Into<String>) -> PlayerMessage {
    PlayerMessage {
        message: message.into(),
        localization: Some(LocalizedMessage::new(code)),
    }
}

impl PlayerMessage {
    pub(super) fn with_param(mut self, key: &str, value: String) -> Self {
        if let Some(localization) = &mut self.localization {
            localization.params.insert(key.to_string(), value);
        }
        self
    }
}

/// 1g = 100s = 10,000c; mirrors `formatGold` in client/src/lib/utils/currency.ts.
pub(super) fn format_copper(copper: i64) -> String {
    let (gold, rest) = (copper / 10_000, copper % 10_000);
    let (silver, copper) = (rest / 100, rest % 100);
    let mut parts = Vec::new();
    if gold > 0 {
        parts.push(format!("{gold}g"));
    }
    if silver > 0 {
        parts.push(format!("{silver}s"));
    }
    if copper > 0 || parts.is_empty() {
        parts.push(format!("{copper}c"));
    }
    parts.join(" ")
}
