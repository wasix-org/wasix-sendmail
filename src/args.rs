use clap::{Args, Parser, ValueEnum};
use lettre::Address;
use std::{str::FromStr, sync::Mutex};

/// Parse an email address from a string for clap
fn parse_email(s: &str) -> Result<Address, String> {
    Address::from_str(s).map_err(|_| format!("Invalid email address: {s}"))
}

fn parse_port(s: &str) -> Result<u16, String> {
    s.parse::<i64>()
        .map_err(|_| format!("Invalid port: {s}"))
        .and_then(|port| {
            if !(1..=65535).contains(&port) {
                Err(format!("Port must be between 1 and 65535: {port}"))
            } else {
                Ok(port as u16)
            }
        })
}

#[derive(Parser, Debug)]
#[command(name = "sendmail")]
#[command(about = "Sendmail-compatible mail sending utility")]
#[command(
    long_about = "A sendmail-compatible mail sending utility that supports multiple backends."
)]
#[command(after_help = "For more information, see https://github.com/wasix-org/wasix-sendmail")]
#[command(group(
    clap::ArgGroup::new("api_backend")
        .required(false)
        .multiple(true)
        .requires_all(["api_url", "api_sender", "api_token"])
))]
#[command(group(
    clap::ArgGroup::new("relay_backend")
        .required(false)
        .multiple(true)
        .requires_all(["relay_host"])
))]
#[command(group(
    clap::ArgGroup::new("file_backend")
        .required(false)
        .multiple(true)
        .requires_all(["file_path"])
))]
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
    #[arg(
        long,
        env = "SENDMAIL_FILE_PATH",
        group = "file_backend",
        help_heading = "File backend"
    )]
    pub file_path: Option<String>,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum SmtpRelayProtocol {
    /// Use TLS encryption
    Tls,
    /// Use STARTTLS encryption
    #[clap(name = "starttls")]
    StartTls,
    /// Use plain text
    Plain,
    /// Attempt STARTTLS if available, otherwise use plain text
    Opportunistic,
}

/// SMTP relay backend configuration
#[derive(Args, Debug)]
pub struct SmtpRelayConfig {
    /// SMTP relay host
    #[arg(
        long,
        env = "SENDMAIL_RELAY_HOST",
        group = "relay_backend",
        help_heading = "SMTP relay backend"
    )]
    pub relay_host: Option<String>,

    /// SMTP relay port
    #[arg(
        long,
        env = "SENDMAIL_RELAY_PORT",
        group = "relay_backend",
        help_heading = "SMTP relay backend",
        default_value = "587",
        value_parser = parse_port,
    )]
    pub relay_port: u16,

    /// SMTP relay protocol (e.g., tls, starttls, plain)
    #[arg(
        long,
        env = "SENDMAIL_RELAY_PROTO",
        group = "relay_backend",
        help_heading = "SMTP relay backend",
        default_value = "opportunistic"
    )]
    pub relay_proto: SmtpRelayProtocol,

    /// SMTP relay username
    #[arg(
        long,
        env = "SENDMAIL_RELAY_USER",
        group = "relay_backend",
        help_heading = "SMTP relay backend",
        requires_all = ["relay_pass"]
    )]
    pub relay_user: Option<String>,

    /// SMTP relay password
    #[arg(
        long,
        env = "SENDMAIL_RELAY_PASS",
        group = "relay_backend",
        help_heading = "SMTP relay backend",
        requires_all = ["relay_user"]

    )]
    pub relay_pass: Option<String>,
}

/// Backend REST API configuration
#[derive(Args, Debug)]
pub struct ApiBackendConfig {
    /// URL of the mail endpoint
    #[arg(
        long,
        env = "SENDMAIL_API_URL",
        group = "api_backend",
        help_heading = "API backend"
    )]
    pub api_url: Option<String>,

    /// Default sender of the mail
    #[arg(
        long,
        env = "SENDMAIL_API_SENDER",
        group = "api_backend",
        help_heading = "API backend"
    )]
    pub api_sender: Option<String>,

    /// Token which can be used to identify with the backend server
    #[arg(
        long,
        env = "SENDMAIL_API_TOKEN",
        group = "api_backend",
        help_heading = "API backend"
    )]
    pub api_token: Option<String>,
}

/// During parsing, we modify the environment variables and restore them after parsing.
///
/// The mutex is used to allow running tests in parallel with different environment variables.
static PARSER_MUTEX: Mutex<()> = Mutex::new(());

/// Parse CLI arguments from environment variables and command line arguments
pub fn parse_cli_args(
    args: &[String],
    envs: &[(String, String)],
) -> Result<SendmailArgs, clap::Error> {
    let args_str: Vec<&str> = args.iter().map(std::string::String::as_str).collect();

    let _guard = PARSER_MUTEX.lock().unwrap();
    let mut restored_envs = Vec::new();
    for (key, value) in envs {
        let previous_value = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        restored_envs.push((key.clone(), previous_value));
    }
    let parsed_args = SendmailArgs::try_parse_from(args_str);
    for (key, value) in restored_envs {
        match value {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
    parsed_args
}
