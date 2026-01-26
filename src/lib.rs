use std::{
    io::{Read, Write},
    str::FromStr,
};
pub mod args;
pub mod backend;
pub mod logger;
pub mod parser;

use lettre::Address;
use log::info;
use rootcause::{
    hooks::{
        Hooks,
        builtin_hooks::report_formatter::{DefaultReportFormatter, NodeConfig},
    },
    prelude::*,
};
use uuid::Uuid;

use crate::args::{SendmailArgs, parse_cli_args};

/// Run sendmail and return an error report
pub fn run_sendmail_err(
    stdin: &mut dyn Read,
    _stdout: &mut dyn Write,
    _stderr: &mut dyn Write,
    cli_args: SendmailArgs,
) -> Result<(), Report> {
    logger::init_logger(cli_args.verbosity);

    // Fail early if no recipients specified and not reading from headers
    if !cli_args.read_recipients_from_headers && cli_args.recipients.is_empty() {
        return Err(report!("No recipients specified"));
    }

    let backend = backend::create_from_config(&cli_args.backend_config)?;

    let mut raw_email = String::new();
    stdin.read_to_string(&mut raw_email)?;

    let headers = parser::parse_email_headers(&raw_email);

    // Extract recipients from headers if requested
    let recipients = if cli_args.read_recipients_from_headers {
        info!("Reading recipients from email headers");
        let mut header_recipients = Vec::new();
        for header_name in &["To", "Cc", "Bcc"] {
            for value in parser::header_values(&headers, header_name) {
                let addrs = parser::parse_mailboxes_header(value)?;
                header_recipients.extend(addrs);
            }
        }
        header_recipients
    } else {
        cli_args.recipients.clone()
    };

    // Check again in case the recipients were read from headers
    if recipients.is_empty() {
        return Err(report!("No recipients specified"));
    }

    // Extract From address from headers
    let header_from = parser::header_values(&headers, "From")
        .next()
        .and_then(|value| parser::parse_mailbox_header(value).ok());

    let envelope_from = cli_args
        .from
        .or(header_from)
        .unwrap_or_else(|| Address::from_str("nobody@localhost").unwrap());

    let missing_headers =
        generate_missing_headers(&headers, &envelope_from, cli_args.fullname.as_deref());
    let raw_email = prepend_headers(&raw_email, &missing_headers);

    let recipients_refs: Vec<&Address> = recipients.iter().collect();
    backend.send(&envelope_from, &recipients_refs, &raw_email)?;
    Ok(())
}

pub fn run_sendmail(
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    args: &[String],
    envs: &[(String, String)],
) -> i32 {
    let cli_args = match parse_cli_args(args, envs) {
        Ok(args) => args,
        Err(e) => {
            write!(stderr, "{e}").unwrap();
            return 1;
        }
    };

    // Setup error formatting
    let mut hook = DefaultReportFormatter::ASCII;
    hook.report_header = "";
    hook.report_node_standalone_formatting =
        NodeConfig::new(("", "\n"), ("", "\n"), ("", "\n"), ("", "\n"), "  ");
    let hooks = if cli_args.verbosity == 0 {
        Hooks::new_without_locations()
    } else {
        Hooks::new()
    };
    hooks.report_formatter(hook).replace();

    match run_sendmail_err(stdin, stdout, stderr, cli_args) {
        Ok(()) => 0,
        Err(e) => {
            write!(stderr, "{e}").unwrap();
            1
        }
    }
}

/// Generate missing required headers (From:, Date:, Message-ID:) based on existing headers.
/// Returns a vector of header strings to add.
fn generate_missing_headers(
    headers: &[parser::HeaderField],
    from: &Address,
    fullname: Option<&str>,
) -> Vec<String> {
    let mut headers_to_add = Vec::new();

    if !parser::has_header(headers, "From") {
        let from_header = match fullname {
            Some(name) => {
                let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
                format!("From: \"{escaped}\" <{from}>")
            }
            None => format!("From: {from}"),
        };
        headers_to_add.push(from_header);
    }

    if !parser::has_header(headers, "Date") {
        headers_to_add.push(format!("Date: {}", format_rfc5322_date()));
    }

    if !parser::has_header(headers, "Message-ID") {
        headers_to_add.push(format!("Message-ID: {}", generate_message_id(from)));
    }

    headers_to_add
}

/// Prepend headers to the raw email content.
/// Headers are inserted at the top of the email (before other headers).
fn prepend_headers(raw_email: &str, headers: &[String]) -> String {
    if headers.is_empty() {
        raw_email.to_string()
    } else {
        format!("{}\r\n{}", headers.join("\r\n"), raw_email)
    }
}

/// Format current date/time in RFC 5322 format using lettre's Date API.
fn format_rfc5322_date() -> String {
    use lettre::message::{Mailbox, MessageBuilder};
    let dummy: Mailbox = "nobody@localhost".parse().unwrap();
    let message = MessageBuilder::new()
        .from(dummy.clone())
        .to(dummy)
        .date_now()
        .body(String::new())
        .unwrap();
    String::from_utf8_lossy(&message.formatted())
        .lines()
        .find_map(|line| line.strip_prefix("Date: "))
        .map(|s| s.trim().to_string())
        .expect("Date header not found in formatted message")
}

/// Generate a unique Message-ID header value using UUID format: <UUID@domain>
fn generate_message_id(from: &Address) -> String {
    let uuid = Uuid::new_v4();
    let domain = from.domain();
    format!("<{uuid}@{domain}>")
}

#[cfg(test)]
mod tests {
    use lettre::Address;

    use super::{generate_missing_headers, prepend_headers};
    use crate::backend::{EmailBackend, FileBackend};
    use crate::parser::parse_email_headers;
    use std::str::FromStr;

    #[test]
    fn test_file_backend() {
        let temp_file = std::env::temp_dir().join("test_email.txt");
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email =
            "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\nTest body";
        let from = Address::from_str("sender@example.com").unwrap();
        let to = Address::from_str("recipient@example.com").unwrap();
        assert!(backend.send(&from, &[&to], raw_email).is_ok());
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_add_missing_headers_all_missing() {
        let raw_email = "Subject: Test\n\nBody content";
        let headers = parse_email_headers(raw_email);
        let from = Address::from_str("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, None);
        let result = prepend_headers(raw_email, &missing);

        assert!(result.contains("From: sender@example.com"));
        assert!(result.contains("Date:"));
        assert!(result.contains("Message-ID:"));
        assert!(result.contains("Subject: Test"));
        assert!(result.contains("Body content"));
    }

    #[test]
    fn test_add_missing_headers_from_exists() {
        let raw_email = "From: existing@example.com\nSubject: Test\n\nBody";
        let headers = parse_email_headers(raw_email);
        let from = Address::from_str("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, None);
        let result: String = prepend_headers(raw_email, &missing);

        // Should not add From header since it exists
        assert!(!result.contains("From: sender@example.com"));
        assert!(result.contains("From: existing@example.com"));
        assert!(result.contains("Date:"));
        assert!(result.contains("Message-ID:"));
    }

    #[test]
    fn test_add_missing_headers_date_exists() {
        let raw_email = "Date: Mon, 1 Jan 2024 12:00:00 +0000\nSubject: Test\n\nBody";
        let headers = parse_email_headers(raw_email);
        let from = Address::from_str("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, None);
        let result = prepend_headers(raw_email, &missing);

        assert!(result.contains("From: sender@example.com"));
        // Should not add another Date header
        let date_count = result.matches("Date:").count();
        assert_eq!(date_count, 1);
        assert!(result.contains("Message-ID:"));
    }

    #[test]
    fn test_add_missing_headers_message_id_exists() {
        let raw_email = "Message-ID: <test@example.com>\nSubject: Test\n\nBody";
        let headers = parse_email_headers(raw_email);
        let from = Address::from_str("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, None);
        let result = prepend_headers(raw_email, &missing);

        assert!(result.contains("From: sender@example.com"));
        assert!(result.contains("Date:"));
        // Should not add another Message-ID header
        let msgid_count = result.matches("Message-ID:").count();
        assert_eq!(msgid_count, 1);
    }

    #[test]
    fn test_add_missing_headers_no_empty_line() {
        let raw_email = "Subject: Test\nBody content";
        let headers = parse_email_headers(raw_email);
        let from = Address::from_str("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, None);
        let result = prepend_headers(raw_email, &missing);

        assert!(result.contains("From: sender@example.com"));
        assert!(result.contains("Date:"));
        assert!(result.contains("Message-ID:"));
    }

    #[test]
    fn test_add_missing_headers_with_fullname() {
        let raw_email = "Subject: Test\n\nBody content";
        let headers = parse_email_headers(raw_email);
        let from = Address::from_str("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, Some("John Doe"));
        let result = prepend_headers(raw_email, &missing);

        assert!(result.contains("From: \"John Doe\" <sender@example.com>"));
        assert!(result.contains("Date:"));
        assert!(result.contains("Message-ID:"));
    }

    #[test]
    fn test_add_missing_headers_with_fullname_escapes_quotes() {
        let raw_email = "Subject: Test\n\nBody content";
        let headers = parse_email_headers(raw_email);
        let from = Address::from_str("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, Some("John \"Johnny\" Doe"));
        let result = prepend_headers(raw_email, &missing);

        assert!(result.contains("From: \"John \\\"Johnny\\\" Doe\" <sender@example.com>"));
    }
}
