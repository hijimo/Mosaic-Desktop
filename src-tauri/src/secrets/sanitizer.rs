use regex::Regex;
use std::sync::LazyLock;

static OPENAI_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| compile_regex(r"sk-[A-Za-z0-9]{20,}"));
static AWS_ACCESS_KEY_ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| compile_regex(r"\bAKIA[0-9A-Z]{16}\b"));
static BEARER_TOKEN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| compile_regex(r"(?i)\bBearer\s+[A-Za-z0-9._\-]{16,}\b"));
static SECRET_ASSIGNMENT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    compile_regex(r#"(?i)\b(api[_-]?key|token|secret|password)\b(\s*[:=]\s*)(["']?)[^\s"']{8,}"#)
});

/// Remove secrets and keys from a String on a best-effort basis
/// using well-known regex patterns.
pub fn redact_secrets(input: String) -> String {
    let redacted = OPENAI_KEY_REGEX.replace_all(&input, "[REDACTED_SECRET]");
    let redacted = AWS_ACCESS_KEY_ID_REGEX.replace_all(&redacted, "[REDACTED_SECRET]");
    let redacted = BEARER_TOKEN_REGEX.replace_all(&redacted, "Bearer [REDACTED_SECRET]");
    let redacted = SECRET_ASSIGNMENT_REGEX.replace_all(&redacted, "$1$2$3[REDACTED_SECRET]");

    redacted.to_string()
}

fn compile_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(err) => panic!("invalid regex pattern `{pattern}`: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_regex() {
        // Compile all regex patterns to verify they are valid
        let _ = redact_secrets("secret".to_string());
    }

    #[test]
    fn redacts_openai_key() {
        let input = "key is sk-abcdefghijklmnopqrstuvwxyz".to_string();
        let result = redact_secrets(input);
        assert!(result.contains("[REDACTED_SECRET]"));
        assert!(!result.contains("sk-abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn redacts_aws_key() {
        let input = "aws key AKIAIOSFODNN7EXAMPLE".to_string();
        let result = redact_secrets(input);
        assert!(result.contains("[REDACTED_SECRET]"));
    }

    #[test]
    fn redacts_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.test".to_string();
        let result = redact_secrets(input);
        assert!(result.contains("Bearer [REDACTED_SECRET]"));
    }

    #[test]
    fn redacts_secret_assignment() {
        let input = "api_key = 'my_super_secret_value_here'".to_string();
        let result = redact_secrets(input);
        assert!(result.contains("[REDACTED_SECRET]"));
    }

    #[test]
    fn preserves_non_secret_text() {
        let input = "hello world, this is normal text".to_string();
        let result = redact_secrets(input.clone());
        assert_eq!(result, input);
    }
}
