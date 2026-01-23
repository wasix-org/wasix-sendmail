pub mod api;
pub mod file;
pub mod smtp;

use std::path::PathBuf;
use std::str::FromStr;

pub use api::ApiBackend;
pub use file::FileBackend;
pub use smtp::SmtpBackend;

use crate::parser::EmailAddress;
use crate::{args::BackendConfig, backend::smtp::TlsMode};
use log::{debug, info, warn};

#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("Host not provided")]
    HostNotProvided,
    #[error("From address not provided")]
    FromNotProvided,
    #[error("Username and password must be provided together")]
    OnlyUsernameOrPasswordProvided,
    #[error("API URL not provided")]
    ApiUrlNotProvided,
    #[error("API configuration incomplete: all three variables (SENDMAIL_API_URL, SENDMAIL_API_SENDER, SENDMAIL_API_TOKEN) must be set")]
    ApiConfigIncomplete,
    #[error("Invalid default sender address: {0}")]
    ApiInvalidEmailAddress(String),
    #[error("No backend configured. Please set one of: SENDMAIL_FILE_PATH, SENDMAIL_RELAY_HOST, or SENDMAIL_API_URL")]
    NoBackendConfigured,
    #[error("API request failed (400 Bad Request): {0}")]
    ApiBadRequest(String),
    #[error("API request failed (401 Unauthorized): {0}")]
    ApiUnauthorized(String),
    #[error("API request failed (402 Payment Required): {0}")]
    ApiQuotaExceeded(String),
    #[error("API request failed (403 Forbidden): {0}")]
    ApiForbidden(String),
    #[error("API request failed (413 Payload Too Large): {0}")]
    ApiMessageTooLarge(String),
    #[error("API request failed ({0} Server Error): {1}")]
    ApiServerError(u16, String),
    #[error("API request failed ({0}): {1}")]
    ApiUnexpectedStatus(u16, String),
    #[error("{0}")]
    NetworkError(#[from] anyhow::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Backend trait mirroring POSIX sendmail interface.
///
/// The backend receives:
/// - Raw email content (headers + body as received from stdin)
/// - Envelope recipients (from command line or extracted from headers)
/// - Envelope sender (from -f flag or From header)
pub trait EmailBackend: Send + Sync {
    /// Send email with envelope information.
    ///
    /// # Arguments
    /// * `envelope_from` - Envelope sender address (from -f flag or From header)
    /// * `envelope_to` - Envelope recipient addresses (from command line or headers)
    /// * `raw_email` - Raw email content as read from stdin (headers + body)
    fn send(
        &self,
        envelope_from: &EmailAddress,
        envelope_to: &[&EmailAddress],
        raw_email: &str,
    ) -> Result<(), BackendError>;

    /// Get the default sender address for this backend.
    ///
    /// Returns the default sender email address. For most backends this is
    /// `username@localhost`, but for API backends it returns the configured sender.
    fn default_sender(&self) -> EmailAddress {
        // TODO: Get the username from the system without using whoami, because that introduces a bunch of weird dependencies.
        let username = "nobody";
        let sender_str = format!("{}@localhost", username);
        EmailAddress::from_str(&sender_str)
            .expect("username@localhost should be a valid email address")
    }
}

/// Create a backend instance based on configuration.
///
/// Backend selection priority order:
/// 1. File backend (if SENDMAIL_FILE_PATH is set)
/// 2. SMTP relay (if SENDMAIL_RELAY_HOST is set)
/// 3. Backend/REST API (if SENDMAIL_API_URL is set)
///
/// If no backend is configured, returns an error.
/// If sending with the selected backend fails, sendmail fails - no fallback to other backends.
pub fn create_from_config(config: &BackendConfig) -> Result<Box<dyn EmailBackend>, BackendError> {
    // Priority 1: File backend
    if let Some(file_path) = &config.file.file_path {
        let path = PathBuf::from(file_path);
        info!("Using file backend to {}", path.display());
        return Ok(Box::new(FileBackend::new(path)?));
    }

    // Priority 2: SMTP relay
    if let Some(relay_host) = &config.smtp_relay.relay_host {
        info!("Using SMTP relay backend");
        let port = config.smtp_relay.relay_port.unwrap_or(587);
        let proto = config.smtp_relay.relay_proto.clone();
        let username = config.smtp_relay.relay_user.clone();
        let password = config.smtp_relay.relay_pass.clone();

        debug!("SMTP relay: host={} port={}", relay_host, port);
        if let Some(p) = &proto {
            debug!("SMTP relay: protocol={}", p);
        }

        // Validate authentication credentials
        if username.is_some() != password.is_some() {
            warn!("SMTP relay credentials misconfigured: only one of SENDMAIL_RELAY_USER/SENDMAIL_RELAY_PASS is set");
            return Err(BackendError::OnlyUsernameOrPasswordProvided);
        }

        return Ok(Box::new(SmtpBackend::new(
            relay_host.clone(),
            port,
            TlsMode::StartTlsIfAvailable,
            username,
            password,
        )?));
    }

    // Priority 3: Backend/REST API
    let api_url_set = config.api.api_url.is_some();
    let api_sender_set = config.api.api_sender.is_some();
    let api_token_set = config.api.api_token.is_some();

    if api_url_set || api_sender_set || api_token_set {
        // Check if all three are set
        if !api_url_set || !api_sender_set || !api_token_set {
            return Err(BackendError::ApiConfigIncomplete);
        }

        info!("Using REST API backend");
        let url = config.api.api_url.as_ref().unwrap().clone();
        let sender = config.api.api_sender.as_ref().unwrap();
        let Ok(sender_email) = EmailAddress::from_str(sender) else {
            return Err(BackendError::ApiInvalidEmailAddress(sender.clone()));
        };
        let token = config.api.api_token.as_ref().unwrap().clone();

        debug!("API backend: url={}", url);
        debug!("API backend: default sender={}", sender_email);

        return Ok(Box::new(ApiBackend::new(url, sender_email, token)));
    }

    // No backend configured - return error
    Err(BackendError::NoBackendConfigured)
}
