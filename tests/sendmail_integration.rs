use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_file(name: &str) -> std::path::PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "wasix_sendmail_integration_{}_{}_{}.txt",
        name,
        std::process::id(),
        ts
    ))
}

fn run_with_file_backend(
    args: Vec<String>,
    envs: Vec<(String, String)>,
    email: &str,
) -> (i32, std::path::PathBuf) {
    let temp_file = envs
        .iter()
        .find(|(k, _)| k == "SENDMAIL_FILE_PATH")
        .map(|(_, v)| std::path::PathBuf::from(v))
        .expect("SENDMAIL_FILE_PATH must be set");

    let mut stdin = Cursor::new(email.as_bytes().to_vec());
    let mut stdout = Vec::<u8>::new();
    let mut stderr = Vec::<u8>::new();

    let rc = wasix_sendmail::run_sendmail(&mut stdin, &mut stdout, &mut stderr, &args, &envs);
    (rc, temp_file)
}

fn envs_for_file_backend(path: &std::path::Path) -> Vec<(String, String)> {
    vec![
        ("SENDMAIL_BACKEND".to_string(), "file".to_string()),
        (
            "SENDMAIL_FILE_PATH".to_string(),
            path.to_string_lossy().to_string(),
        ),
    ]
}

#[test]
fn common_cli_recipient_defaults_from() {
    let out = unique_temp_file("common_cli_recipient_defaults_from");
    let envs = envs_for_file_backend(&out);

    let args = vec!["sendmail".to_string(), "recipient@example.com".to_string()];
    let email = "Subject: Test\n\nTest body";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    assert!(content.contains("Envelope-From: nobody@localhost"));
    assert!(content.contains("Envelope-To: recipient@example.com"));
    // Email body should contain the original content (headers may be added)
    assert!(content.contains("Subject: Test"));
    assert!(content.contains("Test body"));
    // Added headers should be present
    assert!(content.contains("From: nobody@localhost"));
    assert!(content.contains("Date:"));
    assert!(content.contains("Message-ID:"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn common_t_reads_to_cc_bcc_and_from_header() {
    let out = unique_temp_file("common_t_reads_to_cc_bcc_and_from_header");
    let envs = envs_for_file_backend(&out);

    let args = vec!["sendmail".to_string(), "-t".to_string()];
    let email = "From: sender@example.com\nTo: a@example.com\nCc: c@example.com\nBcc: b@example.com\nSubject: Hi\n\nBody";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    assert!(content.contains("Envelope-From: sender@example.com"));
    assert!(content.contains("Envelope-To: a@example.com, c@example.com, b@example.com"));
    assert!(content.contains("Subject: Hi"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn common_t_no_recipients_is_error() {
    let out = unique_temp_file("common_t_no_recipients_is_error");
    let envs = envs_for_file_backend(&out);

    let args = vec!["sendmail".to_string(), "-t".to_string()];
    let email = "From: sender@example.com\nSubject: No recipients\n\nBody";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 1);
    assert!(
        !path.exists(),
        "backend should not have been invoked without recipients"
    );
}

#[test]
fn uncommon_cli_from_overrides_header_from() {
    let out = unique_temp_file("uncommon_cli_from_overrides_header_from");
    let envs = envs_for_file_backend(&out);

    let args = vec![
        "sendmail".to_string(),
        "-f".to_string(),
        "override@example.com".to_string(),
        "recipient@example.com".to_string(),
    ];
    let email = "From: header@example.com\nSubject: From override\n\nBody";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    assert!(content.contains("Envelope-From: override@example.com"));
    assert!(!content.contains("Envelope-From: header@example.com"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn malicious_unknown_backend_falls_back_to_file() {
    let out = unique_temp_file("malicious_unknown_backend_falls_back_to_file");
    let mut envs = envs_for_file_backend(&out);
    // Override to an unknown backend; implementation should fall back to file.
    envs.iter_mut()
        .find(|(k, _)| k == "SENDMAIL_BACKEND")
        .unwrap()
        .1 = "evil".to_string();

    let args = vec!["sendmail".to_string(), "recipient@example.com".to_string()];
    let email = "Subject: Unknown backend\n\nBody";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    assert!(content.contains("Envelope-To: recipient@example.com"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn malicious_header_folding_like_line_does_not_create_extra_recipient() {
    let out =
        unique_temp_file("malicious_header_folding_like_line_does_not_create_extra_recipient");
    let envs = envs_for_file_backend(&out);

    let args = vec!["sendmail".to_string(), "-t".to_string()];
    // This is a folding-like line (starts with a space). We must NOT treat it as a new header
    // (otherwise it becomes a header-injection vector).
    let email = "From: sender@example.com\nTo: victim@example.com\n Bcc: attacker@example.com\nSubject: Fold\n\nBody";

    let (rc, path) = run_with_file_backend(args, envs, email);
    // With RFC 5322 unfolding enabled and strict address-list parsing, this malformed message
    // makes the To field invalid (the folded line becomes part of the To field body).
    // Correct behavior is to *fail*, rather than smuggling a new Bcc recipient.
    assert_eq!(rc, 1);
    assert!(
        !path.exists(),
        "backend should not have been invoked on parse failure"
    );
}

#[test]
fn rfc5322_folded_to_header_is_unfolded() {
    let out = unique_temp_file("rfc5322_folded_to_header_is_unfolded");
    let envs = envs_for_file_backend(&out);

    let args = vec!["sendmail".to_string(), "-t".to_string()];
    let email =
        "From: sender@example.com\nTo: a@example.com,\n\tb@example.com\nSubject: Folded\n\nBody";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    let envelope_to_line = content
        .lines()
        .find(|l| l.starts_with("Envelope-To:"))
        .expect("output should contain Envelope-To line");
    assert_eq!(
        envelope_to_line,
        "Envelope-To: a@example.com, b@example.com"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn common_f_flag_sets_fullname_in_from_header() {
    let out = unique_temp_file("common_f_flag_sets_fullname_in_from_header");
    let envs = envs_for_file_backend(&out);

    let args = vec![
        "sendmail".to_string(),
        "-f".to_string(),
        "sender@example.com".to_string(),
        "-F".to_string(),
        "John Doe".to_string(),
        "recipient@example.com".to_string(),
    ];
    let email = "Subject: Test\n\nTest body";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    assert!(content.contains("Envelope-From: sender@example.com"));
    assert!(content.contains("Envelope-To: recipient@example.com"));
    // The From header should include the fullname
    assert!(content.contains("From: \"John Doe\" <sender@example.com>"));
    assert!(content.contains("Subject: Test"));
    assert!(content.contains("Test body"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn common_f_flag_without_f_flag_uses_default_from() {
    let out = unique_temp_file("common_f_flag_without_f_flag_uses_default_from");
    let envs = envs_for_file_backend(&out);

    let args = vec![
        "sendmail".to_string(),
        "-F".to_string(),
        "Jane Smith".to_string(),
        "recipient@example.com".to_string(),
    ];
    let email = "Subject: Test\n\nTest body";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    // Should use default from address
    assert!(content.contains("Envelope-From: nobody@localhost"));
    // The From header should include the fullname with default address
    assert!(content.contains("From: \"Jane Smith\" <nobody@localhost>"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn common_f_flag_escapes_quotes_in_fullname() {
    let out = unique_temp_file("common_f_flag_escapes_quotes_in_fullname");
    let envs = envs_for_file_backend(&out);

    let args = vec![
        "sendmail".to_string(),
        "-f".to_string(),
        "sender@example.com".to_string(),
        "-F".to_string(),
        "John \"Johnny\" Doe".to_string(),
        "recipient@example.com".to_string(),
    ];
    let email = "Subject: Test\n\nTest body";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    // The From header should properly escape quotes
    assert!(content.contains("From: \"John \\\"Johnny\\\" Doe\" <sender@example.com>"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn uncommon_f_flag_does_not_override_existing_from_header() {
    let out = unique_temp_file("uncommon_f_flag_does_not_override_existing_from_header");
    let envs = envs_for_file_backend(&out);

    let args = vec![
        "sendmail".to_string(),
        "-f".to_string(),
        "envelope@example.com".to_string(),
        "-F".to_string(),
        "CLI Fullname".to_string(),
        "recipient@example.com".to_string(),
    ];
    // Email already has a From header, so -F should not add a new one
    let email = "From: \"Header Fullname\" <header@example.com>\nSubject: Test\n\nTest body";

    let (rc, path) = run_with_file_backend(args, envs, email);
    assert_eq!(rc, 0);

    let content = std::fs::read_to_string(&path).expect("output file should exist");
    // Envelope should use CLI -f flag
    assert!(content.contains("Envelope-From: envelope@example.com"));
    // From header should remain as in the original email (not replaced)
    assert!(content.contains("From: \"Header Fullname\" <header@example.com>"));
    // Should not contain the CLI fullname
    assert!(!content.contains("CLI Fullname"));

    let _ = std::fs::remove_file(&path);
}
