use anyhow::Context;
use log::{debug, info, trace};
use url::Url;

use super::{BackendError, EmailBackend};
use crate::parser::EmailAddress;

pub struct ApiBackend {
    url: String,
    default_sender: EmailAddress,
    token: String,
}

impl ApiBackend {
    pub fn new(url: String, sender: EmailAddress, token: String) -> Self {
        Self {
            url,
            default_sender: sender,
            token,
        }
    }
}

impl EmailBackend for ApiBackend {
    fn send(
        &self,
        envelope_from: &EmailAddress,
        envelope_to: &[&EmailAddress],
        raw_email: &str,
    ) -> Result<(), BackendError> {
        info!(
            "API backend: sending via {} ({} recipient(s))",
            self.url,
            envelope_to.len()
        );
        debug!("API backend: envelope-from={}", envelope_from.as_str());
        debug!("API backend: default sender={}", self.default_sender);
        trace!("API backend: raw_email_bytes={}", raw_email.len());

        if self.url.is_empty() {
            return Err(BackendError::ApiUrlNotProvided);
        }
        if envelope_to.is_empty() {
            debug!("API backend: empty recipient list; nothing to send");
            return Ok(());
        }

        // Use envelope_from, converting to string for API
        let sender = envelope_from.as_str();

        // Build query parameters
        let mut url = Url::parse(&self.url).context("Failed to parse API URL")?;
        url.query_pairs_mut().append_pair("sender", sender);

        for recipient in envelope_to {
            url.query_pairs_mut()
                .append_pair("recipients", recipient.as_str());
        }

        debug!("API backend: POST {}", url);
        trace!("API backend: Authorization: Bearer [REDACTED]");

        // Send the request with ureq
        let response = ureq::post(url.as_str())
            .timeout(std::time::Duration::from_secs(30))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Content-Type", "message/rfc822")
            .send_string(raw_email);

        let (status, response_body) = match response {
            Ok(resp) => (resp.status(), resp.into_string().ok()),
            Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().ok()),
            Err(ureq::Error::Transport(e)) => {
                return Err(BackendError::NetworkError(anyhow::anyhow!(
                    "HTTP transport error: {}",
                    e
                )))
            }
        };

        debug!("API backend: response status={}", status);

        let get_error_msg = |body: Option<String>, default: &str| {
            let body = body.unwrap_or_else(|| default.to_string());
            if body.len() <= 100 {
                body
            } else {
                // Truncate safely at a valid UTF-8 boundary near 100 bytes
                let mut idx = 100;
                while !body.is_char_boundary(idx) {
                    idx -= 1;
                }
                body[..idx].to_string()
            }
        };

        match status {
            202 => {
                info!("API backend: message accepted for delivery");
                Ok(())
            }
            400 => {
                let error_msg = get_error_msg(response_body, "Invalid request");
                Err(BackendError::ApiBadRequest(error_msg))
            }
            401 => {
                let error_msg = get_error_msg(response_body, "Unauthorized");
                Err(BackendError::ApiUnauthorized(error_msg))
            }
            402 => {
                let error_msg = get_error_msg(response_body, "Quota exceeded");
                Err(BackendError::ApiQuotaExceeded(error_msg))
            }
            403 => {
                let error_msg = get_error_msg(response_body, "Forbidden");
                Err(BackendError::ApiForbidden(error_msg))
            }
            413 => {
                let error_msg = get_error_msg(response_body, "Message too large");
                Err(BackendError::ApiMessageTooLarge(error_msg))
            }
            500..=599 => {
                let error_msg = get_error_msg(response_body, "Server error");
                Err(BackendError::ApiServerError(status, error_msg))
            }
            _ => {
                let error_msg = get_error_msg(response_body, "Unexpected error");
                Err(BackendError::ApiUnexpectedStatus(status, error_msg))
            }
        }
    }

    fn default_sender(&self) -> EmailAddress {
        self.default_sender.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_api_backend_creation() {
        let backend = ApiBackend::new(
            "https://api.example.com/v1/mail".to_string(),
            EmailAddress::from_str("default@example.com").unwrap(),
            "test-token".to_string(),
        );
        assert_eq!(backend.url, "https://api.example.com/v1/mail");
        assert_eq!(
            backend.default_sender,
            EmailAddress::from_str("default@example.com").unwrap()
        );
        assert_eq!(backend.token, "test-token");
    }

    #[test]
    fn test_api_backend_default_sender() {
        let backend = ApiBackend::new(
            "https://api.example.com/v1/mail".to_string(),
            EmailAddress::from_str("custom@example.com").unwrap(),
            "test-token".to_string(),
        );
        let default_sender = backend.default_sender();
        assert_eq!(default_sender.as_str(), "custom@example.com");
    }
}
