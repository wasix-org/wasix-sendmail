pub mod file;
pub mod smtp;

pub use file::FileBackend;
pub use smtp::SmtpBackend;

use log::{debug, info, warn};

#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("Host not provided")]
    HostNotProvided,
    #[error("From address not provided")]
    FromNotProvided,
    #[error("Username and password must be provided together")]
    OnlyUsernameOrPasswordProvided,
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
        envelope_from: &str,
        envelope_to: &[&str],
        raw_email: &str,
    ) -> Result<(), BackendError>;
}

/// Create a backend instance based on environment variables.
///
/// Reads `SENDMAIL_BACKEND` to determine the backend type:
/// - `"smtp"` - Creates an SMTP backend (requires `SMTP_HOST`, `SMTP_PORT`, optionally `SMTP_USERNAME`/`SMTP_PASSWORD`)
/// - `"file"` or any other value - Creates a file backend (uses `SENDMAIL_FILE_PATH` or defaults to `/tmp/sendmail_output.txt`)
pub fn create_from_env(envs: &[(String, String)]) -> Box<dyn EmailBackend> {
    let backend_type = envs
        .iter()
        .find(|(key, _)| key == "SENDMAIL_BACKEND")
        .map(|(_, value)| value.as_str())
        .unwrap_or("file");

    debug!(
        "Selecting backend via env SENDMAIL_BACKEND={}",
        backend_type
    );

    match backend_type {
        "smtp" => {
            info!("Using SMTP backend");
            let host = envs
                .iter()
                .find(|(key, _)| key == "SMTP_HOST")
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| "localhost".to_string());
            let port = envs
                .iter()
                .find(|(key, _)| key == "SMTP_PORT")
                .and_then(|(_, value)| value.parse().ok())
                .unwrap_or(587);
            let username = envs
                .iter()
                .find(|(key, _)| key == "SMTP_USERNAME")
                .map(|(_, value)| value.clone());
            let password = envs
                .iter()
                .find(|(key, _)| key == "SMTP_PASSWORD")
                .map(|(_, value)| value.clone());

            debug!("Creating SMTP backend: host={} port={}", host, port);
            if username.is_some() ^ password.is_some() {
                warn!("SMTP credentials misconfigured: only one of SMTP_USERNAME/SMTP_PASSWORD is set");
            }
            Box::new(SmtpBackend::new(host, port, username, password))
        }
        other => {
            if other != "file" {
                warn!(
                    "Unknown SENDMAIL_BACKEND={}; falling back to file backend",
                    other
                );
            }
            info!("Using file backend");
            let path = envs
                .iter()
                .find(|(key, _)| key == "SENDMAIL_FILE_PATH")
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| "/tmp/sendmail_output.txt".to_string());
            debug!("Creating file backend: path={}", path);
            Box::new(FileBackend::new(path))
        }
    }
}
