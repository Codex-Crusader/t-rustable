use crate::types::ApiError;
use serde::de::DeserializeOwned;

/// Makes an [`ApiError`] with a code from `docs/errors.md` and a message.
pub fn api_error(code: &str, message: impl Into<String>) -> ApiError {
    ApiError {
        code: code.to_string(),
        message: message.into(),
        hint: None,
        details: None,
        command_index: None,
    }
}

/// Parses the JSON `args` of an action or a batch job.
pub fn parse_args<T: DeserializeOwned>(args: &str) -> Result<T, ApiError> {
    let args = if args.trim().is_empty() { "null" } else { args };
    serde_json::from_str(args).map_err(|e| {
        api_error(
            "invalid-argument",
            format!("The arguments are not valid: {e}."),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Args {
        degrees: i32,
    }

    #[test]
    fn parses_args() {
        assert_eq!(
            parse_args::<Args>(r#"{"degrees": 90}"#).unwrap(),
            Args { degrees: 90 }
        );
        assert_eq!(parse_args::<Option<Args>>("").unwrap(), None);
        assert_eq!(
            parse_args::<Args>("{").unwrap_err().code,
            "invalid-argument"
        );
    }
}
