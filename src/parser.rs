use log::trace;
use rootcause::prelude::*;
use std::str::FromStr;

use lettre::{Address, message::Mailboxes};

/// A parsed email header field with unfolded value
#[derive(Debug, Clone)]
pub struct HeaderField {
    pub name: String,
    pub value: String, // unfolded value
}

/// Parse raw email content into unfolded header fields.
///
/// RFC 5322 specifies that header field bodies can be folded across multiple lines by inserting
/// CRLF followed by whitespace. Unfolding replaces each CRLF + WSP with a single SP.
#[must_use]
pub fn parse_email_headers(email: &str) -> Vec<HeaderField> {
    trace!("Parsing email headers");
    let mut headers: Vec<HeaderField> = Vec::new();
    let mut current: Option<HeaderField> = None;

    for line in email.lines() {
        if line.trim().is_empty() {
            break; // end of header section
        }

        // Continuation (folding) line: append to previous header value.
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(cur) = current.as_mut() {
                // Unfold by replacing the line break + WSP with a single space.
                cur.value.push(' ');
                cur.value.push_str(line.trim());
            }
            continue;
        }

        // New header line: flush previous.
        if let Some(prev) = current.take() {
            headers.push(prev);
        }

        // Parse "Name: value"
        if let Some(colon_pos) = line.find(':') {
            let name = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();
            current = Some(HeaderField { name, value });
        } else {
            // Malformed header line; ignore.
            trace!("Ignoring malformed header line without ':'");
        }
    }

    if let Some(prev) = current.take() {
        headers.push(prev);
    }

    trace!("Parsed {} header field(s)", headers.len());
    headers
}

/// Parse a header value as mailboxes (address list) and extract email addresses.
///
/// This function parses header values like "To", "Cc", "Bcc" that contain mailbox lists.
/// Returns a vector of validated email addresses.
pub fn parse_mailboxes_header(value: &str) -> Result<Vec<Address>, Report> {
    let mailboxes: Mailboxes = value
        .parse()
        .map_err(|e| report!("Invalid email address: {e}").attach(format!("Header: {value}")))?;

    mailboxes
        .iter()
        .map(|mailbox| {
            let addr_str = mailbox.email.to_string();
            Address::from_str(&addr_str).map_err(|e| {
                report!("Invalid email address: {e}")
                    .attach(format!("Address: {addr_str}"))
                    .attach(format!("Header: {value}"))
            })
        })
        .collect()
}

/// Parse a header value as mailboxes and return the first email address.
///
/// This is useful for headers like "From" where we typically want the first address
/// even if multiple are present.
pub fn parse_mailbox_header(value: &str) -> Result<Address, Report> {
    let mut mailboxes = parse_mailboxes_header(value)?;

    let mailboxes_len = mailboxes.len();
    match mailboxes_len {
        0 => Err(report!("Empty From: header")),
        1 => Ok(mailboxes.pop().unwrap()),
        _ => {
            Err(report!("More than one address in the From: header")
                .attach(format!("Header: {value}")))
        }
    }
}

/// Return all header values for a header name (case-insensitive).
pub fn header_values<'a>(
    headers: &'a [HeaderField],
    name: &'a str,
) -> impl Iterator<Item = &'a str> {
    headers
        .iter()
        .filter(move |h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

/// Check if a header exists (case-insensitive).
#[must_use]
pub fn has_header(headers: &[HeaderField], name: &str) -> bool {
    headers.iter().any(|h| h.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_email_headers() {
        let email = "From: sender@example.com\nTo: recipient1@example.com, recipient2@example.com\nCc: cc@example.com\nSubject: Test\n\nBody content";
        let headers = parse_email_headers(email);

        assert_eq!(headers.len(), 4);
        assert!(has_header(&headers, "From"));
        assert!(has_header(&headers, "To"));
        assert!(has_header(&headers, "Cc"));
        assert!(has_header(&headers, "Subject"));
    }

    #[test]
    fn test_parse_mailboxes_header() {
        let value = "recipient1@example.com, recipient2@example.com";
        let addresses = parse_mailboxes_header(value).unwrap();
        assert_eq!(addresses.len(), 2);
        assert_eq!(addresses[0].to_string(), "recipient1@example.com");
        assert_eq!(addresses[1].to_string(), "recipient2@example.com");
    }

    #[test]
    fn test_parse_mailbox_header() {
        let value = "sender@example.com";
        let address = parse_mailbox_header(value).unwrap();
        assert_eq!(address.to_string(), "sender@example.com");
    }

    #[test]
    fn test_parse_mailbox_header_with_display_name() {
        let value = "\"Sender Name\" <sender@example.com>";
        let address = parse_mailbox_header(value).unwrap();
        assert_eq!(address.to_string(), "sender@example.com");
    }

    #[test]
    fn test_parse_mailbox_header_multiple() {
        let value = "first@example.com, second@example.com";
        // Should fail
        parse_mailbox_header(value).unwrap_err();
        // Should succeed
        parse_mailboxes_header(value).unwrap();
    }

    #[test]
    fn test_parse_mailbox_header_empty() {
        let value = "";
        parse_mailbox_header(value).unwrap_err();
    }

    #[test]
    fn test_parse_mailboxes_header_invalid() {
        let value = "invalid-email";
        let result = parse_mailboxes_header(value);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Invalid email address"));
    }

    #[test]
    fn test_parse_mailbox_header_invalid() {
        let value = "invalid-email";
        let result = parse_mailbox_header(value);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Invalid email address"));
    }

    #[test]
    fn rfc5322_unfolding_allows_folded_to_header() {
        // Folded header continuation (WSP line) is valid per RFC 5322.
        let email = "From: sender@example.com\nTo: a@example.com,\n\tb@example.com\nSubject: Folded\n\nBody";
        let headers = parse_email_headers(email);
        let to_value = header_values(&headers, "To").next().unwrap();
        let addresses = parse_mailboxes_header(to_value).unwrap();
        let recipient_strs: Vec<String> = addresses.iter().map(|e| e.to_string()).collect();
        assert_eq!(recipient_strs, vec!["a@example.com", "b@example.com"]);
    }

    #[test]
    fn rfc5322_mailbox_parsing_allows_display_name() {
        let email = "From: \"Sender Name\" <sender@example.com>\nTo: Recipient <to@example.com>\nSubject: Names\n\nBody";
        let headers = parse_email_headers(email);
        let from_value = header_values(&headers, "From").next().unwrap();
        let from = parse_mailbox_header(from_value).unwrap();
        assert_eq!(from.to_string(), "sender@example.com");

        let to_value = header_values(&headers, "To").next().unwrap();
        let to_addresses = parse_mailboxes_header(to_value).unwrap();
        assert_eq!(to_addresses.len(), 1);
        assert_eq!(to_addresses[0].to_string(), "to@example.com");
    }

    #[test]
    #[ignore = "Comments are not supported for now. If we want them we need to switch from lettre to a custom parser."]
    fn rfc5322_comments_are_ignored() {
        let email = "From: sender@example.com (comment)\nTo: a@example.com (x), b@example.com\nSubject: C\n\nBody";
        let headers = parse_email_headers(email);
        let from_value = header_values(&headers, "From").next().unwrap();
        let from = parse_mailbox_header(from_value).unwrap();
        assert_eq!(from.to_string(), "sender@example.com");

        let to_value = header_values(&headers, "To").next().unwrap();
        let addresses = parse_mailboxes_header(to_value).unwrap();
        let recipient_strs: Vec<String> = addresses.iter().map(|e| e.to_string()).collect();
        assert_eq!(recipient_strs, vec!["a@example.com", "b@example.com"]);
    }

    // Tests for the new chumsky-based parser are in email_parser.rs
}
