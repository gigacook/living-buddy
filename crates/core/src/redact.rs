//! Redaction for anything that might reach a log line or error message.

use regex::Regex;
use std::sync::LazyLock;

static RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    let r = |p: &str| Regex::new(p).expect("valid redaction regex");
    vec![
        // Share links and any token-like path segment after /share/ or /s/.
        (r(r"(/share/|/s/)[A-Za-z0-9_\-]{8,}"), "${1}[redacted]"),
        // Authorization headers and bearer tokens.
        (r(r"(?i)(authorization\s*[:=]\s*)(bearer|basic)?\s*[^\s,;]+"), "${1}[redacted]"),
        (r(r"(?i)\bbearer\s+[A-Za-z0-9._\-~+/=]{8,}"), "Bearer [redacted]"),
        // Key=value style secrets in query strings, JSON and env dumps.
        (
            r(
                r#"(?i)("?(?:access_token|refresh_token|id_token|client_secret|api[_-]?key|password|passwd|secret|token|code|cookie|session)"?\s*[:=]\s*"?)[^"&\s,;}]+"#,
            ),
            "${1}[redacted]",
        ),
        // Provider key formats.
        (r(r"sk-ant-[A-Za-z0-9_\-]{6,}"), "[redacted-anthropic-key]"),
        (r(r"\bsk-[A-Za-z0-9_\-]{16,}"), "[redacted-key]"),
        (r(r"\bxox[abprs]-[A-Za-z0-9\-]{8,}"), "[redacted-slack-token]"),
        (r(r"\bya29\.[A-Za-z0-9_\-.]{8,}"), "[redacted-google-token]"),
        (r(r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]*"), "[redacted-jwt]"),
        // Email addresses are personal data; keep only the domain.
        (r(r"\b[A-Za-z0-9._%+\-]+@([A-Za-z0-9.\-]+\.[A-Za-z]{2,})\b"), "[email]@${1}"),
    ]
});

pub fn redact(input: &str) -> String {
    let mut out = input.to_string();
    for (re, rep) in RULES.iter() {
        out = re.replace_all(&out, *rep).into_owned();
    }
    out
}

/// Shows a URL's scheme and host only; paths and queries may carry tokens.
pub fn url_for_display(raw: &str) -> String {
    match raw.split_once("://") {
        Some((scheme, rest)) => {
            let host = rest.split(['/', '?', '#']).next().unwrap_or("");
            let host = host.rsplit('@').next().unwrap_or(host);
            format!("{scheme}://{host}/…")
        }
        None => "[url]".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_secrets() {
        let cases = [
            ("GET /share/AbCdEfGhIjKlMnOp/calendar.ics", "/share/[redacted]/calendar.ics"),
            ("Authorization: Bearer abc.def.ghi", "Authorization: [redacted]"),
            ("refresh_token=1//0abcdef&x=1", "refresh_token=[redacted]&x=1"),
            ("{\"access_token\":\"ya29.secretvalue\"}", "\"access_token\":\"[redacted]\""),
            ("key sk-ant-api03-ABCDEFGHIJ", "[redacted-anthropic-key]"),
            ("slack xoxb-1234567890-abc", "[redacted-slack-token]"),
            ("from jane.doe@example.com", "[email]@example.com"),
            ("password: hunter2", "password: [redacted]"),
        ];
        for (input, expect) in cases {
            let out = redact(input);
            assert!(out.contains(expect), "{input} -> {out}");
        }
    }

    #[test]
    fn leaves_ordinary_text() {
        assert_eq!(redact("Took out the recycling at 7"), "Took out the recycling at 7");
    }

    #[test]
    fn url_display_drops_path_and_userinfo() {
        assert_eq!(url_for_display("https://user:pw@cal.example.com/private/abc123.ics?token=x"), "https://cal.example.com/…");
    }
}
