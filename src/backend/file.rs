use std::{io::Write, path::PathBuf};

use super::EmailBackend;
use crate::parser::EmailAddress;
use rootcause::prelude::*;

pub struct FileBackend {
    path: PathBuf,
}

impl FileBackend {
    pub fn new(path: PathBuf) -> Result<Self, Report> {
        let path = PathBuf::from(".").join(path);
        let parent_dir = path
            .parent()
            .ok_or_else(|| {
                report!("Failed to get parent directory of the output file")
                    .attach(format!("Path: {}", path.display()))
            })?
            .canonicalize()
            .map_err(|e| {
                report!("Parent directory of the output file does not exist")
                    .attach(format!("Path: {}", path.display()))
                    .attach(format!("Error: {}", e))
            })?;
        let basename = path.file_name().ok_or_else(|| {
            report!("Failed to get basename of the output file")
                .attach(format!("Path: {}", path.display()))
        })?;
        let absolute_path = parent_dir.join(basename);

        Ok(Self {
            path: absolute_path,
        })
    }
}

impl EmailBackend for FileBackend {
    fn send(
        &self,
        envelope_from: &EmailAddress,
        envelope_to: &[&EmailAddress],
        raw_email: &str,
    ) -> Result<(), Report> {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)
            .map_err(|e| {
                report!("Failed to open file for writing")
                    .attach(format!("Path: {}", self.path.display()))
                    .attach(format!("Error: {}", e))
            })?;

        writeln!(file, "Envelope-From: {}", envelope_from.as_str())?;
        let recipients_str = envelope_to
            .iter()
            .map(|e| e.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(file, "Envelope-To: {}", recipients_str)?;
        writeln!(file, "---")?;
        writeln!(file, "{}", raw_email)?;
        writeln!(file, "---")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::str::FromStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_temp_file() -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "test_sendmail_{}_{}.txt",
            std::process::id(),
            timestamp
        ))
    }

    #[test]
    fn test_file_backend_single_recipient() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email =
            "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\nTest body";

        let from = EmailAddress::from_str("sender@example.com").unwrap();
        let to = EmailAddress::from_str("recipient@example.com").unwrap();
        assert!(backend.send(&from, &[&to], raw_email).is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Envelope-From: sender@example.com"));
        assert!(content.contains("Envelope-To: recipient@example.com"));
        assert!(content.contains("From: sender@example.com"));
        assert!(content.contains("Test body"));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_multiple_recipients() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email = "From: sender@example.com\nSubject: Test\n\nTest body";

        let from = EmailAddress::from_str("sender@example.com").unwrap();
        let to1 = EmailAddress::from_str("recipient1@example.com").unwrap();
        let to2 = EmailAddress::from_str("recipient2@example.com").unwrap();
        let to3 = EmailAddress::from_str("recipient3@example.com").unwrap();
        assert!(backend.send(&from, &[&to1, &to2, &to3], raw_email).is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Envelope-From: sender@example.com"));
        assert!(content.contains(
            "Envelope-To: recipient1@example.com, recipient2@example.com, recipient3@example.com"
        ));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_empty_recipients() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email = "From: sender@example.com\nSubject: Test\n\nTest body";

        let from = EmailAddress::from_str("sender@example.com").unwrap();
        assert!(backend.send(&from, &[], raw_email).is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Envelope-From: sender@example.com"));
        assert!(content.contains("Envelope-To: "));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_appends_to_file() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email1 = "From: sender1@example.com\nSubject: First\n\nFirst email";
        let raw_email2 = "From: sender2@example.com\nSubject: Second\n\nSecond email";

        let from1 = EmailAddress::from_str("sender1@example.com").unwrap();
        let from2 = EmailAddress::from_str("sender2@example.com").unwrap();
        let to = EmailAddress::from_str("recipient@example.com").unwrap();

        assert!(backend.send(&from1, &[&to], raw_email1).is_ok());
        assert!(backend.send(&from2, &[&to], raw_email2).is_ok());

        let content = fs::read_to_string(&temp_file).expect("File should exist after sending");
        // Should contain both emails
        assert!(
            content.contains("Envelope-From: sender1@example.com"),
            "Should contain first sender"
        );
        assert!(
            content.contains("Envelope-From: sender2@example.com"),
            "Should contain second sender"
        );
        assert!(
            content.contains("First email"),
            "Should contain first email body"
        );
        assert!(
            content.contains("Second email"),
            "Should contain second email body"
        );
        // Should have separator lines (two separators per email)
        let separator_count = content.matches("---").count();
        assert_eq!(
            separator_count, 4,
            "Should have 4 separator lines (2 per email)"
        );

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_file_format() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email =
            "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\nTest body";

        let from = EmailAddress::from_str("sender@example.com").unwrap();
        let to = EmailAddress::from_str("recipient@example.com").unwrap();
        assert!(backend.send(&from, &[&to], raw_email).is_ok());

        let content = fs::read_to_string(&temp_file).expect("File should exist after sending");
        let lines: Vec<&str> = content.lines().collect();

        // Check format: Envelope-From, Envelope-To, separator, email content, separator
        assert!(lines.len() >= 4, "File should have at least 4 lines");
        assert!(
            lines[0].starts_with("Envelope-From:"),
            "First line should be Envelope-From"
        );
        assert!(
            lines[1].starts_with("Envelope-To:"),
            "Second line should be Envelope-To"
        );
        assert_eq!(lines[2], "---", "Third line should be separator");
        assert!(
            lines[3].contains("From: sender@example.com"),
            "Fourth line should contain email header"
        );
        assert!(content.contains("---"), "File should end with separator");

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_empty_email_body() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email = "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\n";

        let from = EmailAddress::from_str("sender@example.com").unwrap();
        let to = EmailAddress::from_str("recipient@example.com").unwrap();
        assert!(backend.send(&from, &[&to], raw_email).is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Envelope-From: sender@example.com"));
        assert!(content.contains("Subject: Test"));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_special_characters() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email = "From: sender+test@example.com\nTo: recipient@example.com\nSubject: Test with special chars: !@#$%\n\nBody with special chars: àáâãäå";

        let from = EmailAddress::from_str("sender+test@example.com").unwrap();
        let to = EmailAddress::from_str("recipient@example.com").unwrap();
        assert!(backend.send(&from, &[&to], raw_email).is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Envelope-From: sender+test@example.com"));
        assert!(content.contains("àáâãäå"));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_multiline_email() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let raw_email = "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\nLine 1\nLine 2\nLine 3";

        let from = EmailAddress::from_str("sender@example.com").unwrap();
        let to = EmailAddress::from_str("recipient@example.com").unwrap();
        assert!(backend.send(&from, &[&to], raw_email).is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Line 1"));
        assert!(content.contains("Line 2"));
        assert!(content.contains("Line 3"));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_default_sender() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.clone()).unwrap();
        let default_sender = backend.default_sender();
        // The default sender should be username@localhost
        assert!(default_sender.as_str().ends_with("@localhost"));

        let _ = fs::remove_file(&temp_file);
    }
}
