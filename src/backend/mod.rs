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

/// Helper to lookup an environment variable by key
fn get_env(envs: &[(String, String)], key: &str) -> Option<String> {
    envs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

/// Create a backend instance based on environment variables.
///
/// Reads `SENDMAIL_BACKEND` to determine the backend type:
/// - `"smtp"` - Creates an SMTP backend (requires `SMTP_HOST`, `SMTP_PORT`, optionally `SMTP_USERNAME`/`SMTP_PASSWORD`)
/// - `"file"` or any other value - Creates a file backend (uses `SENDMAIL_FILE_PATH` or defaults to `/tmp/sendmail_output.txt`)
pub fn create_from_env(envs: &[(String, String)]) -> Box<dyn EmailBackend> {
    let backend_env = get_env(envs, "SENDMAIL_BACKEND");
    let backend_type = backend_env.as_deref().unwrap_or("file");

    debug!(
        "Selecting backend via env SENDMAIL_BACKEND={}",
        backend_type
    );

    match backend_type {
        "smtp" => {
            info!("Using SMTP backend");
            let host = get_env(envs, "SMTP_HOST").unwrap_or_else(|| "localhost".to_string());
            let port = get_env(envs, "SMTP_PORT")
                .and_then(|v| v.parse().ok())
                .unwrap_or(587);
            let username = get_env(envs, "SMTP_USERNAME");
            let password = get_env(envs, "SMTP_PASSWORD");

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
            let path = get_env(envs, "SENDMAIL_FILE_PATH")
                .unwrap_or_else(|| "/tmp/sendmail_output.txt".to_string());
            debug!("Creating file backend: path={}", path);
            Box::new(FileBackend::new(path))
        }
    }
}
