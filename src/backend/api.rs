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
        if self.url.is_empty() {
            return Err(BackendError::ApiUrlNotProvided);
        }

        // Use envelope_from, converting to string for API
        let sender = envelope_from.as_str();

        let mut url = Url::parse(&self.url).context("Failed to parse API URL")?;
        url.query_pairs_mut().append_pair("sender", sender);
        for recipient in envelope_to {
            url.query_pairs_mut()
                .append_pair("recipients", recipient.as_str());
        }

        // Send the request with ureq
        let response = ureq::post(url.as_str())
            .timeout(std::time::Duration::from_secs(30))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Content-Type", "message/rfc822")
            .send_string(raw_email);

        let (status, response_body) = match response {
            Ok(_) => {
                info!("API backend: message accepted for delivery");
                return Ok(());
            }
            Err(ureq::Error::Transport(e)) => {
                return Err(BackendError::NetworkError(anyhow::anyhow!(
                    "HTTP transport error: {}",
                    e
                )))
            }
            Err(ureq::Error::Status(code, resp)) => (code, resp.into_string().ok()),
        };

        debug!(
            "API backend: error with status={} and message={:?}",
            status, response_body
        );

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

        let mut error_msg = response_body.unwrap_or(error_msg_from_code.into());
        error_msg.truncate(100);

        return Err(BackendError::ApiBadRequest(error_msg));
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
