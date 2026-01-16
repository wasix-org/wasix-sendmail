use log::{debug, trace};
use std::str::FromStr;
use thiserror::Error;

pub mod email_parser;

pub use email_address::EmailAddress;
use lettre::message::Mailboxes;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Invalid email address: {0}")]
    InvalidEmail(String),
}

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

/// Parse and validate an email address
pub fn parse_email_address(email: &str) -> Result<EmailAddress, ParseError> {
    EmailAddress::from_str(email).map_err(|_| ParseError::InvalidEmail(email.to_string()))
}

/// Parse a header value as mailboxes (address list) and extract email addresses.
/// 
/// This function parses header values like "To", "Cc", "Bcc" that contain mailbox lists.
/// Returns a vector of validated email addresses.
pub fn parse_mailboxes_header(value: &str) -> Result<Vec<EmailAddress>, ParseError> {
    let mut addresses = Vec::new();
    
    // Parse address list using lettre's Mailboxes parser
    let mailboxes: Mailboxes = value
        .parse()
        .map_err(|_| ParseError::InvalidEmail(value.to_string()))?;

    // Extract email addresses from mailboxes
    for mailbox in mailboxes.iter() {
        let addr_str = mailbox.email.to_string();
        addresses.push(
            EmailAddress::from_str(&addr_str)
                .map_err(|_| ParseError::InvalidEmail(addr_str.clone()))?,
        );
    }
    
    Ok(addresses)
}

/// Parse a header value as mailboxes and return the first email address.
/// 
/// This is useful for headers like "From" where we typically want the first address
/// even if multiple are present.
pub fn parse_mailbox_header(value: &str) -> Result<Option<EmailAddress>, ParseError> {
    let mailboxes: Mailboxes = value
        .parse()
        .map_err(|_| ParseError::InvalidEmail(value.to_string()))?;

    // Collect mailboxes into a vector to check length
    let mailbox_vec: Vec<_> = mailboxes.iter().collect();
    if mailbox_vec.is_empty() {
        return Ok(None);
    }
    
    if mailbox_vec.len() > 1 {
        debug!("Multiple addresses found in mailbox header; using the first");
    }

    // Extract email address from first mailbox
    let addr_str = mailbox_vec[0].email.to_string();
    Ok(Some(
        EmailAddress::from_str(&addr_str)
            .map_err(|_| ParseError::InvalidEmail(addr_str.clone()))?,
    ))
}

/// Return all header values for a header name (case-insensitive).
pub fn header_values<'a>(headers: &'a [HeaderField], name: &'a str) -> impl Iterator<Item = &'a str> {
    headers
        .iter()
        .filter(move |h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

/// Check if a header exists (case-insensitive).
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
        assert_eq!(addresses[0].as_str(), "recipient1@example.com");
        assert_eq!(addresses[1].as_str(), "recipient2@example.com");
    }

    #[test]
    fn test_parse_mailbox_header() {
        let value = "sender@example.com";
        let address = parse_mailbox_header(value).unwrap();
        assert_eq!(address.as_ref().map(|e| e.as_str()), Some("sender@example.com"));
    }

    #[test]
    fn test_parse_mailbox_header_with_display_name() {
        let value = "\"Sender Name\" <sender@example.com>";
        let address = parse_mailbox_header(value).unwrap();
        assert_eq!(address.as_ref().map(|e| e.as_str()), Some("sender@example.com"));
    }

    #[test]
    fn test_parse_mailbox_header_multiple() {
        let value = "first@example.com, second@example.com";
        let address = parse_mailbox_header(value).unwrap();
        // Should return first address
        assert_eq!(address.as_ref().map(|e| e.as_str()), Some("first@example.com"));
    }

    #[test]
    fn test_parse_mailbox_header_empty() {
        let value = "";
        let address = parse_mailbox_header(value).unwrap();
        assert_eq!(address, None);
    }

    #[test]
    fn test_parse_mailboxes_header_invalid() {
        let value = "invalid-email";
        let result = parse_mailboxes_header(value);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::InvalidEmail(_)));
    }

    #[test]
    fn test_parse_mailbox_header_invalid() {
        let value = "invalid-email";
        let result = parse_mailbox_header(value);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::InvalidEmail(_)));
    }

    #[test]
    fn rfc5322_unfolding_allows_folded_to_header() {
        // Folded header continuation (WSP line) is valid per RFC 5322.
        let email = "From: sender@example.com\nTo: a@example.com,\n\tb@example.com\nSubject: Folded\n\nBody";
        let headers = parse_email_headers(email);
        let to_value = header_values(&headers, "To").next().unwrap();
        let addresses = parse_mailboxes_header(to_value).unwrap();
        let recipient_strs: Vec<&str> = addresses.iter().map(|e| e.as_str()).collect();
        assert_eq!(recipient_strs, vec!["a@example.com", "b@example.com"]);
    }

    #[test]
    fn rfc5322_mailbox_parsing_allows_display_name() {
        let email =
            "From: \"Sender Name\" <sender@example.com>\nTo: Recipient <to@example.com>\nSubject: Names\n\nBody";
        let headers = parse_email_headers(email);
        let from_value = header_values(&headers, "From").next().unwrap();
        let from = parse_mailbox_header(from_value).unwrap();
        assert_eq!(from.as_ref().map(|e| e.as_str()), Some("sender@example.com"));
        
        let to_value = header_values(&headers, "To").next().unwrap();
        let to_addresses = parse_mailboxes_header(to_value).unwrap();
        assert_eq!(to_addresses.len(), 1);
        assert_eq!(to_addresses[0].as_str(), "to@example.com");
    }

    #[test]
    #[ignore = "Comments are not supported for now. If we want them we need to switch from lettre to a custom parser."]
    fn rfc5322_comments_are_ignored() {
        let email = "From: sender@example.com (comment)\nTo: a@example.com (x), b@example.com\nSubject: C\n\nBody";
        let headers = parse_email_headers(email);
        let from_value = header_values(&headers, "From").next().unwrap();
        let from = parse_mailbox_header(from_value).unwrap();
        assert_eq!(from.as_ref().map(|e| e.as_str()), Some("sender@example.com"));
        
        let to_value = header_values(&headers, "To").next().unwrap();
        let addresses = parse_mailboxes_header(to_value).unwrap();
        let recipient_strs: Vec<&str> = addresses.iter().map(|e| e.as_str()).collect();
        assert_eq!(recipient_strs, vec!["a@example.com", "b@example.com"]);
    }

    // Tests for the new chumsky-based parser are in email_parser.rs
}
