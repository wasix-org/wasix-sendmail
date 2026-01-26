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
        envelope_from: &Address,
        envelope_to: &[&Address],
        raw_email: &str,
    ) -> Result<(), Report> {
        let mut url = self.url.clone();
        url.query_pairs_mut()
            .append_pair("sender", envelope_from.as_ref());
        for recipient in envelope_to {
            url.query_pairs_mut()
                .append_pair("recipients", recipient.as_ref());
        }

        // Send the request with ureq
        let response = ureq::post(url.as_str())
            .timeout(std::time::Duration::from_secs(30))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Content-Type", "message/rfc822")
            .send_string(raw_email);

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
}
