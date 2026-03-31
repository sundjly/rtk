//! Credential redaction for tracking and tee output.
//!
//! Strips secrets from command strings and raw output before persistence.
//! Used by `tracking.rs` (original_cmd, raw_command) and `tee.rs` (raw output).

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // Bearer / Authorization header tokens
    static ref BEARER_RE: Regex =
        Regex::new(r"(?i)(bearer\s+)\S+").unwrap();

    // URL userinfo: https://user:pass@host → https://***@host
    static ref URL_USERINFO_RE: Regex =
        Regex::new(r"://[^@/\s]+@").unwrap();

    // Password-style CLI flags: -p 'xxx', --password=xxx, --password xxx, -p xxx
    static ref PASSWORD_FLAG_RE: Regex =
        Regex::new(r#"(?i)(--?(?:password|passwd|pwd|p\b))[= ]+('[^']*'|"[^"]*"|\S+)"#).unwrap();

    // Known provider token patterns (prefix-based)
    // sk-..., ghp_..., github_pat_..., gho_..., ghu_..., ghs_..., ghr_...
    // xoxb-..., xoxp-..., xoxa-..., xapp-...
    // glpat-..., AKIA...(AWS access key ID, 20 chars)
    static ref PROVIDER_TOKEN_RE: Regex =
        Regex::new(r"(?i)\b(sk-[a-zA-Z0-9_-]{20,}|ghp_[a-zA-Z0-9]{36,}|github_pat_[a-zA-Z0-9_]{20,}|gh[osur]_[a-zA-Z0-9]{36,}|xox[bpas]-[a-zA-Z0-9\-]{10,}|xapp-[a-zA-Z0-9\-]{10,}|glpat-[a-zA-Z0-9\-]{20,}|AKIA[0-9A-Z]{16})\b").unwrap();

    // Generic key=value for env-style secrets: API_KEY=xxx, SECRET_TOKEN=xxx
    static ref ENV_SECRET_RE: Regex =
        Regex::new(r"(?i)\b([A-Z_]*(?:KEY|SECRET|TOKEN|PASSWORD|CREDENTIAL|AUTH)[A-Z_]*)=(\S+)").unwrap();
}

/// Redact credentials from a command string or text.
///
/// Applied to `original_cmd` before SQLite INSERT and to tee raw output.
/// Replaces detected secrets with `***` while preserving structure.
pub fn redact_credentials(text: &str) -> String {
    let mut result = text.to_string();

    // 1. Bearer tokens
    result = BEARER_RE.replace_all(&result, "${1}***").to_string();

    // 2. URL userinfo
    result = URL_USERINFO_RE.replace_all(&result, "://***@").to_string();

    // 3. Password flags
    result = PASSWORD_FLAG_RE
        .replace_all(&result, "${1}=***")
        .to_string();

    // 4. Provider tokens — replace entirely with short prefix + ***
    result = PROVIDER_TOKEN_RE
        .replace_all(&result, |caps: &regex::Captures| {
            let token = &caps[1];
            // Keep only the well-known prefix (e.g. "ghp_", "sk-", "xoxb-", "AKIA")
            let prefix_len = if token.starts_with("github_pat_") {
                11
            } else if token.starts_with("AKIA") {
                4
            } else if let Some(pos) = token.find(|c: char| c == '-' || c == '_') {
                pos + 1
            } else {
                3.min(token.len())
            };
            format!("{}***", &token[..prefix_len])
        })
        .to_string();

    // 5. Env-style secrets
    result = ENV_SECRET_RE.replace_all(&result, "${1}=***").to_string();

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bearer_token() {
        let input = r#"curl -H "Authorization: Bearer sk-abc123def456""#;
        let output = redact_credentials(input);
        assert!(output.contains("Bearer ***"));
        assert!(!output.contains("abc123"));
    }

    #[test]
    fn test_url_userinfo() {
        let input = "git clone https://github_pat_abc123@github.com/org/repo.git";
        let output = redact_credentials(input);
        assert!(output.contains("://***@github.com"));
        assert!(!output.contains("github_pat_abc123@"));
    }

    #[test]
    fn test_password_flag() {
        let r1 = redact_credentials("psql -p 'MyPass123'");
        assert!(!r1.contains("MyPass123"), "password leaked: {}", r1);
        assert!(r1.contains("***"), "no redaction: {}", r1);

        let r2 = redact_credentials("mysql --password=secret");
        assert!(r2.contains("--password=***"), "got: {}", r2);
        assert!(!r2.contains("secret"), "password leaked: {}", r2);
    }

    #[test]
    fn test_provider_tokens() {
        // GitHub PAT
        let input = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let output = redact_credentials(input);
        assert!(output.starts_with("ghp_***"), "got: {}", output);
        assert!(!output.contains("ABCDEFGH"));

        // Slack token
        let input = "xoxb-1234567890-abcdefghij";
        let output = redact_credentials(input);
        assert!(output.starts_with("xoxb-***"), "got: {}", output);
        assert!(!output.contains("1234567890"));

        // AWS access key
        let input = "AKIAIOSFODNN7EXAMPLE";
        let output = redact_credentials(input);
        assert!(output.starts_with("AKIA***"), "got: {}", output);
    }

    #[test]
    fn test_env_secret_pattern() {
        let input = "API_KEY=sk_live_123456789 OTHER=safe";
        let output = redact_credentials(input);
        assert!(output.contains("API_KEY=***"));
        assert!(output.contains("OTHER=safe"));
    }

    #[test]
    fn test_no_false_positives() {
        // Normal commands should pass through unchanged
        let input = "git status --short";
        assert_eq!(redact_credentials(input), input);

        let input = "cargo test my_test_function";
        assert_eq!(redact_credentials(input), input);

        let input = "ls -la /home/user/project";
        assert_eq!(redact_credentials(input), input);
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(redact_credentials(""), "");
    }

    #[test]
    fn test_multiple_secrets_in_one_line() {
        let input = "curl -H 'Authorization: Bearer sk-abc123' https://user:pass@api.example.com";
        let output = redact_credentials(input);
        assert!(output.contains("Bearer ***"));
        assert!(output.contains("://***@api.example.com"));
        assert!(!output.contains("sk-abc123"));
        assert!(!output.contains("user:pass"));
    }
}
