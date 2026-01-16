use std::io::{Read, Write};

pub mod args;
pub mod backend;
pub mod logger;
pub mod parser;

use clap::Parser;
use log::{error, info};
use parser::EmailAddress;
use uuid::Uuid;

pub fn run_sendmail(
    stdin: &mut dyn Read,
    _stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    args: &[String],
    envs: &[(String, String)],
) -> i32 {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let cli_args = match args::SendmailArgs::try_parse_from(args_str) {
        Ok(args) => args,
        Err(e) => {
            let _ = e.print();
            return 1;
        }
    };

    logger::init_logger(cli_args.verbosity);

    let backend = backend::create_from_env(envs);

    let mut raw_email = String::new();
    if let Err(e) = stdin.read_to_string(&mut raw_email) {
        error!("Failed to read email from stdin: {}", e);
        let _ = writeln!(stderr, "sendmail: Failed to read email: {}", e);
        return 1;
    }

    let headers = parser::parse_email_headers(&raw_email);

    // Extract recipients from headers if requested
    let recipients = if cli_args.read_recipients_from_headers {
        info!("Reading recipients from email headers");
        let mut header_recipients = Vec::new();
        for header_name in &["To", "Cc", "Bcc"] {
            for value in parser::header_values(&headers, header_name) {
                match parser::parse_mailboxes_header(value) {
                    Ok(addrs) => header_recipients.extend(addrs),
                    Err(e) => {
                        error!("Failed to parse {} header: {}", header_name, e);
                        let _ = writeln!(stderr, "sendmail: {}", e);
                        return 1;
                    }
                }
            }
        }
        header_recipients
    } else {
        cli_args.recipients.clone()
    };

    if recipients.is_empty() && !cli_args.read_recipients_from_headers {
        let _ = writeln!(stderr, "sendmail: No recipients specified");
        return 1;
    }

    // Extract From address from headers
    let header_from = parser::header_values(&headers, "From")
        .next()
        .and_then(|value| {
            parser::parse_mailbox_header(value)
                .map_err(|e| {
                    error!("Failed to parse From header: {}", e);
                    e
                })
                .ok()
                .flatten()
        });

    let envelope_from = cli_args.from.or(header_from).unwrap_or_else(|| {
        parser::parse_email_address("nobody@localhost")
            .expect("Failed to parse default from address")
    });

    let missing_headers =
        generate_missing_headers(&headers, &envelope_from, cli_args.fullname.as_deref());
    let raw_email = prepend_headers(&raw_email, &missing_headers);

    let recipients_refs: Vec<&str> = recipients.iter().map(|e| e.as_str()).collect();
    match backend.send(envelope_from.as_str(), &recipients_refs, &raw_email) {
        Ok(()) => 0,
        Err(e) => {
            error!("Failed to send email: {}", e);
            let _ = writeln!(stderr, "sendmail: {}", e);
            1
        }
    }
}

/// Generate missing required headers (From:, Date:, Message-ID:) based on existing headers.
/// Returns a vector of header strings to add.
fn generate_missing_headers(
    headers: &[parser::HeaderField],
    from: &EmailAddress,
    fullname: Option<&str>,
) -> Vec<String> {
    let mut headers_to_add = Vec::new();

    if !parser::has_header(headers, "From") {
        let from_header = if let Some(name) = fullname {
            // Format as "Full Name" <email@example.com>
            // Escape quotes in the name if present
            let escaped_name = name.replace('\\', "\\\\").replace('"', "\\\"");
            format!("From: \"{}\" <{}>", escaped_name, from.as_str())
        } else {
            format!("From: {}", from.as_str())
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
        return raw_email.to_string();
    }

    // Add headers at the top, then the rest of the email
    headers.join("\r\n") + "\r\n" + raw_email
}

/// Format current date/time in RFC 5322 format using lettre's Date API.
fn format_rfc5322_date() -> String {
    // Date implements HeaderValue which can be formatted
    // We can use a MessageBuilder to format the date header and extract it
    use lettre::message::{Mailbox, MessageBuilder};
    // Build a minimal message with the date to get the formatted string
    // MessageBuilder requires From and To addresses
    let dummy_from: Mailbox = "nobody@localhost".parse().unwrap();
    let dummy_to: Mailbox = "nobody@localhost".parse().unwrap();
    let message = MessageBuilder::new()
        .from(dummy_from)
        .to(dummy_to)
        .date_now()
        .body(String::new())
        .unwrap();
    // Format the message and extract the Date header value
    // formatted() returns Vec<u8>, convert to string
    let formatted_bytes = message.formatted();
    let formatted_str = String::from_utf8_lossy(&formatted_bytes);
    // Parse the Date header from the formatted message
    for line in formatted_str.lines() {
        if let Some(content) = line.strip_prefix("Date: ") {
            return content.trim().to_string();
        }
    }
    // Fail if Date header is not found - this should never happen
    panic!("Failed to extract Date header from lettre formatted message");
}

/// Generate a unique Message-ID header value using UUID format: <UUID@domain>
fn generate_message_id(from: &EmailAddress) -> String {
    let uuid = Uuid::new_v4();
    let domain = from.domain();
    format!("<{}@{}>", uuid, domain)
}

#[cfg(test)]
mod tests {
    use super::{generate_missing_headers, prepend_headers};
    use crate::backend::{EmailBackend, FileBackend};
    use crate::parser::{parse_email_address, parse_email_headers};

    #[test]
    fn test_file_backend() {
        let temp_file = std::env::temp_dir().join("test_email.txt");
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email =
            "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\nTest body";
        assert!(backend
            .send("sender@example.com", &["recipient@example.com"], raw_email)
            .is_ok());
        let _ = std::fs::remove_file(&temp_file);
    }

    #[test]
    fn test_add_missing_headers_all_missing() {
        let raw_email = "Subject: Test\n\nBody content";
        let headers = parse_email_headers(raw_email);
        let from = parse_email_address("sender@example.com").unwrap();
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
        let from = parse_email_address("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, None);
        let result = prepend_headers(raw_email, &missing);

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
        let from = parse_email_address("sender@example.com").unwrap();
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
        let from = parse_email_address("sender@example.com").unwrap();
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
        let from = parse_email_address("sender@example.com").unwrap();
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
        let from = parse_email_address("sender@example.com").unwrap();
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
        let from = parse_email_address("sender@example.com").unwrap();
        let missing = generate_missing_headers(&headers, &from, Some("John \"Johnny\" Doe"));
        let result = prepend_headers(raw_email, &missing);

        assert!(result.contains("From: \"John \\\"Johnny\\\" Doe\" <sender@example.com>"));
    }
}
