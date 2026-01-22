use anyhow::Context;
use std::io::Write;

use super::{BackendError, EmailBackend};
use log::{debug, info, trace};

pub struct FileBackend {
    path: String,
}

impl FileBackend {
    pub fn new(path: String) -> Self {
        Self { path }
    }
}

impl EmailBackend for FileBackend {
    fn send(
        &self,
        envelope_from: &str,
        envelope_to: &[&str],
        raw_email: &str,
    ) -> Result<(), BackendError> {
        info!("File backend: writing message to {} ({} recipient(s))", self.path, envelope_to.len());
        debug!("File backend: envelope-from={}", envelope_from);
        trace!("File backend: raw_email_bytes={}", raw_email.len());

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .context("Failed to open file for writing")?;

        writeln!(file, "Envelope-From: {}\nEnvelope-To: {}\n---\n{}\n---", 
                 envelope_from, envelope_to.join(", "), raw_email)?;

        debug!("File backend: write complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email =
            "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\nTest body";

        assert!(backend
            .send("sender@example.com", &["recipient@example.com"], raw_email)
            .is_ok());

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
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email = "From: sender@example.com\nSubject: Test\n\nTest body";

        assert!(backend
            .send(
                "sender@example.com",
                &[
                    "recipient1@example.com",
                    "recipient2@example.com",
                    "recipient3@example.com"
                ],
                raw_email
            )
            .is_ok());

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
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email = "From: sender@example.com\nSubject: Test\n\nTest body";

        assert!(backend.send("sender@example.com", &[], raw_email).is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Envelope-From: sender@example.com"));
        assert!(content.contains("Envelope-To: "));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_appends_to_file() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email1 = "From: sender1@example.com\nSubject: First\n\nFirst email";
        let raw_email2 = "From: sender2@example.com\nSubject: Second\n\nSecond email";

        assert!(backend
            .send(
                "sender1@example.com",
                &["recipient@example.com"],
                raw_email1
            )
            .is_ok());
        assert!(backend
            .send(
                "sender2@example.com",
                &["recipient@example.com"],
                raw_email2
            )
            .is_ok());

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
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email =
            "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\nTest body";

        assert!(backend
            .send("sender@example.com", &["recipient@example.com"], raw_email)
            .is_ok());

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
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email = "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\n";

        assert!(backend
            .send("sender@example.com", &["recipient@example.com"], raw_email)
            .is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Envelope-From: sender@example.com"));
        assert!(content.contains("Subject: Test"));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_special_characters() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email = "From: sender+test@example.com\nTo: recipient@example.com\nSubject: Test with special chars: !@#$%\n\nBody with special chars: àáâãäå";

        assert!(backend
            .send(
                "sender+test@example.com",
                &["recipient@example.com"],
                raw_email
            )
            .is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Envelope-From: sender+test@example.com"));
        assert!(content.contains("àáâãäå"));

        let _ = fs::remove_file(&temp_file);
    }

    #[test]
    fn test_file_backend_multiline_email() {
        let temp_file = create_temp_file();
        let backend = FileBackend::new(temp_file.to_string_lossy().to_string());
        let raw_email = "From: sender@example.com\nTo: recipient@example.com\nSubject: Test\n\nLine 1\nLine 2\nLine 3";

        assert!(backend
            .send("sender@example.com", &["recipient@example.com"], raw_email)
            .is_ok());

        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("Line 1"));
        assert!(content.contains("Line 2"));
        assert!(content.contains("Line 3"));

        let _ = fs::remove_file(&temp_file);
    }
}
