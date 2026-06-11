use lettre::Address;
use log::{debug, info};
use rootcause::prelude::*;
use url::Url;

use super::EmailBackend;

#[derive(Debug)]
pub struct ApiBackend {
    url: Url,
    default_sender: Address,
    token: String,
}

impl ApiBackend {
    pub fn new(url: String, sender: Address, token: String) -> Result<Self, Report> {
        let url = Url::parse(&url)
            .map_err(|e| report!("Failed to parse API URL: {e}").attach(format!("URL: '{url}'")))?;
        Ok(Self {
            url,
            default_sender: sender,
            token,
        })
    }
}

impl EmailBackend for ApiBackend {
    fn send(
        &self,
        _envelope_from: &Address,
        envelope_to: &[&Address],
        raw_email: &str,
    ) -> Result<(), Report> {
        let sender = &self.default_sender;

        let mut url = self.url.clone();
        url.query_pairs_mut().append_pair("sender", sender.as_ref());
        for recipient in envelope_to {
            url.query_pairs_mut()
                .append_pair("recipients", recipient.as_ref());
        }

        let raw_email = rewrite_from_header(raw_email, sender);
        let raw_email = normalize_rfc822_line_endings(&raw_email);

        // Send the request with ureq
        let response = ureq::post(url.as_str())
            .timeout(std::time::Duration::from_secs(120))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Content-Type", "message/rfc822")
            .send_string(raw_email.as_str());

        let (content_type, status, response_body) = match response {
            Ok(_response) => {
                info!("API backend: message accepted for delivery");
                return Ok(());
            }
            Err(ureq::Error::Transport(e)) => {
                return Err(
                    report!("HTTP transport error: {e}").attach(format!("URL: {}", url.as_str()))
                );
            }
            Err(ureq::Error::Status(code, resp)) => (
                resp.content_type().to_string(),
                code,
                resp.into_string().ok(),
            ),
        };

        debug!("API backend: error with status={status} and message={response_body:?}");

        let error_msg_from_code = match status {
            200..=299 => "Ok",
            400 => "Invalid request",
            401 => "Unauthorized",
            402 => "Quota exceeded",
            403 => "Forbidden",
            413 => "Message too large",
            500..=599 => "Server error",
            _ => "Unknown error",
        };
        let error_msg_from_code = format!("{status} {error_msg_from_code}");

        let error_msg = match content_type.as_str() {
            "text/plain" => {
                if let Some(response_body) = response_body {
                    let mut message = response_body
                        .lines()
                        .next()
                        .unwrap_or(error_msg_from_code.as_str())
                        .to_string();
                    message.truncate(100);
                    message
                } else {
                    error_msg_from_code
                }
            }
            _ => error_msg_from_code,
        };

        Err(report!("API request failed: {error_msg}")
            .attach(format!("Status code: {status}"))
            .attach(format!("Content type: {content_type}"))
            .into_dynamic())
    }

    fn default_sender(&self) -> Address {
        self.default_sender.clone()
    }
}

fn rewrite_from_header(raw_email: &str, sender: &Address) -> String {
    let (headers, separator, body) = if let Some(pos) = raw_email.find("\r\n\r\n") {
        (&raw_email[..pos], "\r\n\r\n", &raw_email[pos + 4..])
    } else if let Some(pos) = raw_email.find("\n\n") {
        (&raw_email[..pos], "\n\n", &raw_email[pos + 2..])
    } else {
        (raw_email, "", "")
    };

    let newline = if headers.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let replacement = format!("From: {sender}");
    let mut rewritten = Vec::new();
    let mut found_from = false;
    let mut skipping_from_continuation = false;

    for line in headers.lines() {
        let is_continuation = line.starts_with(' ') || line.starts_with('\t');

        if skipping_from_continuation && is_continuation {
            continue;
        }
        skipping_from_continuation = false;

        if !is_continuation
            && let Some((name, _)) = line.split_once(':')
            && !found_from
            && name.eq_ignore_ascii_case("From")
        {
            rewritten.push(replacement.as_str());
            found_from = true;
            skipping_from_continuation = true;
            continue;
        }

        rewritten.push(line);
    }

    if !found_from {
        rewritten.insert(0, replacement.as_str());
    }

    let headers = rewritten.join(newline);
    format!("{headers}{separator}{body}")
}

fn normalize_rfc822_line_endings(raw_email: &str) -> String {
    raw_email
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_api_backend_creation() {
        let backend = ApiBackend::new(
            "https://api.example.com/v1/mail".to_string(),
            Address::from_str("default@example.com").unwrap(),
            "test-token".to_string(),
        )
        .unwrap();
        assert_eq!(backend.url.as_str(), "https://api.example.com/v1/mail");
        assert_eq!(
            backend.default_sender,
            Address::from_str("default@example.com").unwrap()
        );
        assert_eq!(backend.token, "test-token");
    }

    #[test]
    fn test_api_backend_default_sender() {
        let backend = ApiBackend::new(
            "https://api.example.com/v1/mail".to_string(),
            Address::from_str("custom@example.com").unwrap(),
            "test-token".to_string(),
        )
        .unwrap();
        let default_sender = backend.default_sender();
        assert_eq!(&default_sender.to_string(), "custom@example.com");
    }

    #[test]
    fn rewrite_from_header_replaces_case_insensitive_from() {
        let sender = Address::from_str("provisioned@example.com").unwrap();
        let raw_email = "fRoM: Old Name <old@example.com>\nSubject: Test\n\nBody";

        let rewritten = rewrite_from_header(raw_email, &sender);

        assert_eq!(
            rewritten,
            "From: provisioned@example.com\nSubject: Test\n\nBody"
        );
    }

    #[test]
    fn rewrite_from_header_removes_folded_from_continuations() {
        let sender = Address::from_str("provisioned@example.com").unwrap();
        let raw_email = "From: Old Name\n <old@example.com>\nSubject: Test\n\nBody";

        let rewritten = rewrite_from_header(raw_email, &sender);

        assert_eq!(
            rewritten,
            "From: provisioned@example.com\nSubject: Test\n\nBody"
        );
        assert!(!rewritten.contains("old@example.com"));
    }

    #[test]
    fn rewrite_from_header_does_not_touch_body_from_text() {
        let sender = Address::from_str("provisioned@example.com").unwrap();
        let raw_email = "Subject: Test\n\nFrom: this is body text";

        let rewritten = rewrite_from_header(raw_email, &sender);

        assert_eq!(
            rewritten,
            "From: provisioned@example.com\nSubject: Test\n\nFrom: this is body text"
        );
    }

    #[test]
    fn rewrite_from_header_preserves_headers_with_mixed_line_endings() {
        let sender = Address::from_str("provisioned@example.com").unwrap();
        let raw_email = "Message-ID: <x@example.com>\r\nFrom: WordPress <wordpress@site.example>\nTo: recipient@example.com\nSubject: Test\n\nEmail Body";

        let rewritten = rewrite_from_header(raw_email, &sender);

        assert!(rewritten.contains("From: provisioned@example.com"));
        assert!(!rewritten.contains("wordpress@site.example"));
        assert!(rewritten.contains("To: recipient@example.com"));
        assert!(rewritten.contains("Subject: Test"));
        assert!(rewritten.contains("Email Body"));
    }

    #[test]
    fn normalize_rfc822_line_endings_converts_lf_to_crlf() {
        let raw = "From: a@example.com\nSubject: Test\n\nBody\n";
        let normalized = normalize_rfc822_line_endings(raw);
        assert_eq!(
            normalized,
            "From: a@example.com\r\nSubject: Test\r\n\r\nBody\r\n"
        );
    }

    #[test]
    fn normalize_rfc822_line_endings_does_not_double_existing_crlf() {
        let raw = "From: a@example.com\r\nSubject: Test\r\n\r\nBody\r\n";
        let normalized = normalize_rfc822_line_endings(raw);
        assert_eq!(normalized, raw);
    }

    #[test]
    fn normalize_rfc822_line_endings_handles_mixed_endings() {
        let raw = "From: a@example.com\r\nSubject: Test\n\nBody\r";
        let normalized = normalize_rfc822_line_endings(raw);
        assert_eq!(
            normalized,
            "From: a@example.com\r\nSubject: Test\r\n\r\nBody\r\n"
        );
    }
}
