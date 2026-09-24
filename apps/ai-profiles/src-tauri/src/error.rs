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

    /// A remote host refused or couldn't be reached. `code` is the server's
    /// (`unauthorized`, `not_found`, …) or the client's own (`offline`,
    /// `cert_mismatch`); `message` is a sentence for people.
    #[error("{message}")]
    Remote { code: String, message: String },
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
            AppError::Remote { .. } => "Remote",
        };
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("kind", kind)?;
        map.serialize_entry("message", &self.to_string())?;
        if let AppError::Remote { code, .. } = self {
            map.serialize_entry("code", code)?;
        }
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

    #[test]
    fn remote_error_carries_its_code_and_a_bare_message() {
        let error = AppError::Remote {
            code: "offline".into(),
            message: "xjopa1 can't be reached.".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&error).unwrap();
        assert_eq!(json["kind"], "Remote");
        assert_eq!(json["code"], "offline");
        assert_eq!(json["message"], "xjopa1 can't be reached.");
    }
}
