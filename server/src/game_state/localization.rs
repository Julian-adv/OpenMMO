use onlinerpg_shared::messages::LocalizedMessage;

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
