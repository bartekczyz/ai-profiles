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
}

impl AppError {
    /// What the error says, without the prefix naming its kind: what the
    /// user is shown, alone or quoted in another error.
    pub fn message(&self) -> String {
        match self {
            AppError::Io(error) => error.to_string(),
            AppError::Json(error) => error.to_string(),
            AppError::Validation(message) | AppError::NotFound(message) => message.clone(),
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
