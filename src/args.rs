use crate::parser::EmailAddress;
use clap::Parser;
use std::str::FromStr;

/// Parse an email address from a string for clap
fn parse_email(s: &str) -> Result<EmailAddress, String> {
    EmailAddress::from_str(s).map_err(|_| format!("Invalid email address: {}", s))
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
    pub from: Option<EmailAddress>,

    /// Set the full name (display name) for the From header
    #[arg(short = 'F', long = "fullname", value_name = "NAME")]
    pub fullname: Option<String>,

    /// Increase verbosity (can be used multiple times: -v, -vv, -vvv)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbosity: u8,

    /// Recipient email addresses (ignored when reading recipients from headers)
    #[arg(value_name = "RECIPIENT", value_parser = parse_email)]
    pub recipients: Vec<EmailAddress>,
}
