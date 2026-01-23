use clap::{Args, Parser};
use lettre::Address;
use std::str::FromStr;

/// Parse an email address from a string for clap
fn parse_email(s: &str) -> Result<Address, String> {
    Address::from_str(s).map_err(|_| format!("Invalid email address: {}", s))
}

#[derive(Parser, Debug)]
#[command(name = "sendmail")]
#[command(about = "Sendmail-compatible mail sending utility")]
#[command(
    long_about = "A POSIX-compliant sendmail implementation that supports multiple backends for sending email."
)]
pub struct SendmailArgs {
    /// Read recipients from message headers (To, Cc, Bcc)
    #[arg(short = 't', long = "read-recipients")]
    pub read_recipients_from_headers: bool,

    /// Ignore dots in message body
    #[arg(short = 'i', long = "ignore-dot")]
    pub ignore_dot: bool,

    /// Set the envelope sender address
    #[arg(short = 'f', long = "from", value_name = "ADDRESS", value_parser = parse_email)]
    pub from: Option<Address>,

    /// Set the full name (display name) for the From header
    #[arg(short = 'F', long = "fullname", value_name = "NAME")]
    pub fullname: Option<String>,

    /// Increase verbosity (can be used multiple times: -v, -vv, -vvv)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbosity: u8,

    /// Recipient email addresses (ignored when reading recipients from headers)
    #[arg(value_name = "RECIPIENT", value_parser = parse_email)]
    pub recipients: Vec<Address>,

    #[command(flatten)]
    pub backend_config: BackendConfig,
}

#[derive(Args, Debug)]
pub struct BackendConfig {
    #[command(flatten)]
    pub file: FileBackendConfig,

    #[command(flatten)]
    pub smtp_relay: SmtpRelayConfig,

    #[command(flatten)]
    pub api: ApiBackendConfig,
}

/// File backend configuration (for debugging)
#[derive(Args, Debug)]
pub struct FileBackendConfig {
    /// Path to the output file for file backend
    #[arg(long, env = "SENDMAIL_FILE_PATH")]
    pub file_path: Option<String>,
}

/// SMTP relay backend configuration
#[derive(Args, Debug)]
pub struct SmtpRelayConfig {
    /// SMTP relay host
    #[arg(long, env = "SENDMAIL_RELAY_HOST")]
    pub relay_host: Option<String>,

    /// SMTP relay port
    #[arg(long, env = "SENDMAIL_RELAY_PORT")]
    pub relay_port: Option<u16>,

    /// SMTP relay protocol (e.g., tls, starttls, plain)
    #[arg(long, env = "SENDMAIL_RELAY_PROTO")]
    pub relay_proto: Option<String>,

    /// SMTP relay username
    #[arg(long, env = "SENDMAIL_RELAY_USER")]
    pub relay_user: Option<String>,

    /// SMTP relay password
    #[arg(long, env = "SENDMAIL_RELAY_PASS")]
    pub relay_pass: Option<String>,
}

/// Backend REST API configuration
#[derive(Args, Debug)]
pub struct ApiBackendConfig {
    /// URL of the mail endpoint
    #[arg(long, env = "SENDMAIL_API_URL")]
    pub api_url: Option<String>,

    /// Default sender of the mail
    #[arg(long, env = "SENDMAIL_API_SENDER")]
    pub api_sender: Option<String>,

    /// Token which can be used to identify with the backend server
    #[arg(long, env = "SENDMAIL_API_TOKEN")]
    pub api_token: Option<String>,
}
