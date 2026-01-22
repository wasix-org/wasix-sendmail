use anyhow::Context;
use log::{debug, info, trace};
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use super::{BackendError, EmailBackend};

pub struct ApiBackend {
    url: String,
    sender: String,
    token: String,
}

impl ApiBackend {
    pub fn new(url: String, sender: String, token: String) -> Self {
        Self { url, sender, token }
    }
}

impl EmailBackend for ApiBackend {
    fn send(
        &self,
        envelope_from: &str,
        envelope_to: &[&str],
        raw_email: &str,
    ) -> Result<(), BackendError> {
        info!(
            "API backend: sending via {} ({} recipient(s))",
            self.url,
            envelope_to.len()
        );
        debug!("API backend: envelope-from={}", envelope_from);
        debug!("API backend: default sender={}", self.sender);
        trace!("API backend: raw_email_bytes={}", raw_email.len());

        if self.url.is_empty() {
            return Err(BackendError::ApiUrlNotProvided);
        }
        if envelope_to.is_empty() {
            debug!("API backend: empty recipient list; nothing to send");
            return Ok(());
        }

        // Use envelope_from if provided, otherwise use default sender
        let sender = if !envelope_from.is_empty() {
            envelope_from
        } else {
            &self.sender
        };

        // Build the API request
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        // Build query parameters
        let mut url = reqwest::Url::parse(&self.url).context("Invalid API URL")?;
        url.query_pairs_mut().append_pair("sender", sender);

        for recipient in envelope_to {
            url.query_pairs_mut().append_pair("recipients", recipient);
        }

        debug!("API backend: POST {}", url);
        trace!("API backend: Authorization: Bearer [REDACTED]");

        // Send the request
        let response = client
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(CONTENT_TYPE, "message/rfc822")
            .body(raw_email.to_string())
            .send()
            .context("Failed to send HTTP request")?;

        let status = response.status();
        debug!("API backend: response status={}", status);

        match status.as_u16() {
            202 => {
                info!("API backend: message accepted for delivery");
                Ok(())
            }
            400 => {
                let body = response
                    .text()
                    .unwrap_or_else(|_| "Invalid request".to_string());
                let error_msg = if body.len() <= 100 {
                    body
                } else {
                    body[..100].to_string()
                };
                Err(BackendError::ApiBadRequest(error_msg))
            }
            401 => {
                let body = response
                    .text()
                    .unwrap_or_else(|_| "Unauthorized".to_string());
                let error_msg = if body.len() <= 100 {
                    body
                } else {
                    body[..100].to_string()
                };
                Err(BackendError::ApiUnauthorized(error_msg))
            }
            402 => {
                let body = response
                    .text()
                    .unwrap_or_else(|_| "Quota exceeded".to_string());
                let error_msg = if body.len() <= 100 {
                    body
                } else {
                    body[..100].to_string()
                };
                Err(BackendError::ApiQuotaExceeded(error_msg))
            }
            403 => {
                let body = response.text().unwrap_or_else(|_| "Forbidden".to_string());
                let error_msg = if body.len() <= 100 {
                    body
                } else {
                    body[..100].to_string()
                };
                Err(BackendError::ApiForbidden(error_msg))
            }
            413 => {
                let body = response
                    .text()
                    .unwrap_or_else(|_| "Message too large".to_string());
                let error_msg = if body.len() <= 100 {
                    body
                } else {
                    body[..100].to_string()
                };
                Err(BackendError::ApiMessageTooLarge(error_msg))
            }
            500..=599 => {
                let body = response
                    .text()
                    .unwrap_or_else(|_| "Server error".to_string());
                let error_msg = if body.len() <= 100 {
                    body
                } else {
                    body[..100].to_string()
                };
                Err(BackendError::ApiServerError(status.as_u16(), error_msg))
            }
            _ => {
                let body = response
                    .text()
                    .unwrap_or_else(|_| "Unexpected error".to_string());
                let error_msg = if body.len() <= 100 {
                    body
                } else {
                    body[..100].to_string()
                };
                Err(BackendError::ApiUnexpectedStatus(
                    status.as_u16(),
                    error_msg,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_backend_creation() {
        let backend = ApiBackend::new(
            "https://api.example.com/v1/mail".to_string(),
            "default@example.com".to_string(),
            "test-token".to_string(),
        );
        assert_eq!(backend.url, "https://api.example.com/v1/mail");
        assert_eq!(backend.sender, "default@example.com");
        assert_eq!(backend.token, "test-token");
    }
}
