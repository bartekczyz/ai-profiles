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
        map.serialize_entry("message", &self.to_string())?;
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
        assert!(json.contains(r#""message":"validation error: bad name""#));
    }
}
