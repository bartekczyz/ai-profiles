use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),

    /// A tool the app needs isn't installed: a lasting state the user fixes
    /// by installing it, not a failure that trying again gets past.
    #[error("not installed: {0}")]
    NotInstalled(String),
}

impl AppError {
    /// What the error says, without the prefix naming its kind: what the
    /// user is shown, alone or quoted in another error.
    pub fn message(&self) -> String {
        match self {
            AppError::Io(error) => error.to_string(),
            AppError::Json(error) => error.to_string(),
            AppError::Validation(message)
            | AppError::NotFound(message)
            | AppError::NotInstalled(message) => message.clone(),
        }
    }
}

impl AppError {
    /// The error with its message rewritten by `rewrite`, of the same kind:
    /// an I/O error keeps its [`std::io::ErrorKind`].
    pub fn map_message(self, rewrite: impl FnOnce(String) -> String) -> AppError {
        let message = rewrite(self.message());
        match self {
            AppError::Io(error) => AppError::Io(std::io::Error::new(error.kind(), message)),
            AppError::Json(_) => AppError::Json(serde::de::Error::custom(message)),
            AppError::Validation(_) => AppError::Validation(message),
            AppError::NotFound(_) => AppError::NotFound(message),
            AppError::NotInstalled(_) => AppError::NotInstalled(message),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let kind = match self {
            AppError::Io(_) => "Io",
            AppError::Json(_) => "Json",
            AppError::Validation(_) => "Validation",
            AppError::NotFound(_) => "NotFound",
            AppError::NotInstalled(_) => "NotInstalled",
        };
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("kind", kind)?;
        map.serialize_entry("message", &self.message())?;
        map.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_error_serializes_to_kind_and_message() {
        let error = AppError::Validation("bad name".to_string());
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains(r#""kind":"Validation""#));
        assert!(json.contains(r#""message":"bad name""#));
    }

    #[test]
    fn an_error_says_more_and_keeps_its_kind() {
        let io = AppError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ))
        .map_message(|message| format!("{message}. See /backup"));
        let not_found = AppError::NotFound("gone".to_string()).map_message(|message| message + "!");

        assert!(
            matches!(&io, AppError::Io(error) if error.kind() == std::io::ErrorKind::PermissionDenied),
            "{io:?}"
        );
        assert_eq!(io.message(), "denied. See /backup");
        assert!(matches!(&not_found, AppError::NotFound(message) if message == "gone!"));
    }

    #[test]
    fn the_message_of_an_error_leaves_out_its_kind() {
        let io = AppError::Io(std::io::Error::other("disk full"));

        assert_eq!(io.message(), "disk full");
        assert_eq!(
            AppError::NotFound("no such profile".to_string()).message(),
            "no such profile"
        );
        assert_eq!(
            serde_json::to_value(&io).unwrap(),
            serde_json::json!({ "kind": "Io", "message": "disk full" })
        );
    }
}
