// The mock http server does currently not work on WASIX
#![allow(unexpected_cfgs)]
#![cfg(not(target_vendor = "wasmer"))]
use lettre::Address;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tiny_http::{Response, Server, StatusCode};
use wasix_sendmail::backend::EmailBackend;
use wasix_sendmail::backend::api::ApiBackend;

fn email_address(addr: &str) -> Address {
    Address::from_str(addr).expect("valid email address")
}

/// Helper to create a simple mock server that responds with a specific status code and body
fn start_mock_server(status: u16, body: &'static str) -> (String, thread::JoinHandle<()>) {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let addr = server.server_addr().to_string();
    let url = format!("http://{}", addr);

    let handle = thread::spawn(move || {
        if let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(2)) {
            let response = Response::from_string(body).with_status_code(StatusCode(status));
            let _ = request.respond(response);
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(50));

    (url, handle)
}

struct CapturedRequest {
    url: String,
    body: String,
}

fn start_capturing_mock_server(
    status: u16,
    body: &'static str,
) -> (
    String,
    mpsc::Receiver<CapturedRequest>,
    thread::JoinHandle<()>,
) {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let addr = server.server_addr().to_string();
    let url = format!("http://{}", addr);
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        if let Ok(Some(mut request)) = server.recv_timeout(Duration::from_secs(2)) {
            let url = request.url().to_string();
            let mut request_body = String::new();
            let _ = request.as_reader().read_to_string(&mut request_body);
            let _ = tx.send(CapturedRequest {
                url,
                body: request_body,
            });

            let response = Response::from_string(body).with_status_code(StatusCode(status));
            let _ = request.respond(response);
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(50));

    (url, rx, handle)
}

#[test]
fn test_api_backend_successful_send() {
    let (url, handle) = start_mock_server(202, "Message accepted");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token-123".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email =
        "From: sender@example.com\r\nTo: recipient@example.com\r\nSubject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_ok());

    let _ = handle.join();
}

#[test]
fn test_api_backend_multiple_recipients() {
    let (url, handle) = start_mock_server(202, "");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "secret-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to1 = email_address("user1@example.com");
    let to2 = email_address("user2@example.com");
    let to3 = email_address("user3@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to1, &to2, &to3], raw_email);
    assert!(result.is_ok());

    let _ = handle.join();
}

#[test]
fn test_api_backend_empty_url_error() {
    ApiBackend::new(
        "".to_string(),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap_err();
}

#[test]
fn test_api_backend_bad_request_error() {
    let (url, handle) = start_mock_server(400, "Invalid email format");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("400"));
    assert!(err_msg.contains("Invalid email format"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_unauthorized_error() {
    let (url, handle) = start_mock_server(401, "Invalid token");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "invalid-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("401"));
    assert!(err_msg.contains("Invalid token"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_quota_exceeded_error() {
    let (url, handle) = start_mock_server(402, "Monthly quota exceeded");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("402"));
    assert!(err_msg.contains("Monthly quota exceeded"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_forbidden_error() {
    let (url, handle) = start_mock_server(403, "Sender not authorized");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("403"));
    assert!(err_msg.contains("Sender not authorized"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_message_too_large_error() {
    let (url, handle) = start_mock_server(413, "Message exceeds 10MB limit");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    // Create a large email
    let raw_email = format!("Subject: Test\r\n\r\n{}", "X".repeat(11_000_000));

    let result = backend.send(&from, &[&to], &raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("413"));
    assert!(err_msg.contains("Message exceeds 10MB limit"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_server_error() {
    let (url, handle) = start_mock_server(503, "Service temporarily unavailable");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("503"));
    assert!(err_msg.contains("Service temporarily unavailable"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_unexpected_status() {
    let (url, handle) = start_mock_server(418, "I'm a teapot");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("418"));
    assert!(err_msg.contains("I'm a teapot"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_truncates_long_error_messages() {
    // Create an error message longer than 100 characters
    let long_error = "A".repeat(200).leak();
    let (url, handle) = start_mock_server(400, long_error);

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("400"));
    // Error message should be truncated to 100 characters in the response body attachment
    let truncated = "A".repeat(100);
    assert!(err_msg.contains(&truncated));

    let _ = handle.join();
}

#[test]
fn test_api_backend_special_characters_in_email() {
    let (url, handle) = start_mock_server(202, "");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("test+tag@example.com");
    let to = email_address("user+123@example.com");
    let raw_email = "Subject: Test with special chars\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_ok());

    let _ = handle.join();
}

fn assert_query_sender(request_url: &str, expected_sender: &str) {
    let url = url::Url::parse(&format!("http://localhost{request_url}")).unwrap();
    let sender = url
        .query_pairs()
        .find_map(|(name, value)| (name == "sender").then(|| value.into_owned()));

    assert_eq!(sender.as_deref(), Some(expected_sender));
}

fn assert_no_bare_lf(value: &str) {
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            assert!(
                index > 0 && bytes[index - 1] == b'\r',
                "found bare LF at byte {index}: {value:?}"
            );
        }
    }
}

#[test]
fn test_api_backend_uses_default_sender_not_envelope_from() {
    let (url, rx, handle) = start_capturing_mock_server(202, "");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("envelope@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "From: envelope@example.com\r\nSubject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_ok());

    let request = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_query_sender(&request.url, "default@example.com");
    assert!(!request.url.contains("sender=envelope%40example.com"));
    assert!(request.body.contains("From: default@example.com"));
    assert!(!request.body.contains("From: envelope@example.com"));
    assert!(request.body.contains("\r\nSubject: Test\r\n"));
    assert_no_bare_lf(&request.body);

    let _ = handle.join();
}

#[test]
fn test_api_backend_rewrites_existing_from_header() {
    let (url, rx, handle) = start_capturing_mock_server(202, "");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("provisioned@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("wordpress@site.example");
    let to = email_address("recipient@example.com");
    let raw_email =
        "From: wordpress@site.example\nTo: recipient@example.com\nSubject: Test\n\nBody";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_ok());

    let request = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_query_sender(&request.url, "provisioned@example.com");
    assert!(request.body.contains("From: provisioned@example.com"));
    assert!(!request.body.contains("From: wordpress@site.example"));
    assert!(request.body.contains("To: recipient@example.com"));
    assert!(request.body.contains("\r\nSubject: Test\r\n"));
    assert!(request.body.contains("Body"));
    assert_no_bare_lf(&request.body);

    let _ = handle.join();
}

#[test]
fn test_api_backend_adds_missing_from_header() {
    let (url, rx, handle) = start_capturing_mock_server(202, "");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        Address::from_str("provisioned@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("envelope@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\n\nBody";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_ok());

    let request = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_query_sender(&request.url, "provisioned@example.com");
    assert!(
        request
            .body
            .starts_with("From: provisioned@example.com\r\n")
    );
    assert!(request.body.contains("\r\nSubject: Test\r\n"));
    assert!(request.body.contains("Body"));
    assert_no_bare_lf(&request.body);

    let _ = handle.join();
}

#[test]
fn run_sendmail_api_normalizes_lf_only_message_to_crlf() {
    let (url, rx, handle) = start_capturing_mock_server(202, "");

    let args = vec!["sendmail".to_string(), "-t".to_string(), "-i".to_string()];
    let envs = vec![
        ("SENDMAIL_API_URL".to_string(), format!("{url}/send")),
        (
            "SENDMAIL_API_SENDER".to_string(),
            "provisioned@example.com".to_string(),
        ),
        ("SENDMAIL_API_TOKEN".to_string(), "test-token".to_string()),
    ];
    let email = "From: WordPress <wordpress@site.example>\nTo: recipient@example.com\nSubject: Test\nDate: Thu, 26 Feb 2026 10:00:00 +0000\nMIME-Version: 1.0\nContent-Type: text/plain; charset=UTF-8\nContent-Transfer-Encoding: 7bit\n\nEmail Body\n";

    let mut stdin = std::io::Cursor::new(email.as_bytes().to_vec());
    let mut stdout = Vec::<u8>::new();
    let mut stderr = Vec::<u8>::new();

    let rc = wasix_sendmail::run_sendmail(&mut stdin, &mut stdout, &mut stderr, &args, &envs);

    assert_eq!(rc, 0, "stderr: {}", String::from_utf8_lossy(&stderr));

    let request = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_query_sender(&request.url, "provisioned@example.com");
    assert!(request.body.contains("From: provisioned@example.com\r\n"));
    assert!(!request.body.contains("wordpress@site.example"));
    assert!(request.body.contains("\r\nTo: recipient@example.com\r\n"));
    assert!(request.body.contains("\r\nSubject: Test\r\n"));
    assert!(request.body.contains("\r\n\r\nEmail Body\r\n"));
    assert_no_bare_lf(&request.body);

    let _ = handle.join();
}

#[test]
fn test_api_backend_network_timeout() {
    // Test with an unreachable address (should timeout or fail immediately)
    let backend = ApiBackend::new(
        "http://192.0.2.1:9999/send".to_string(), // TEST-NET-1, non-routable
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    // Should be a network/transport error
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("HTTP transport error") || err_msg.contains("transport"));
}

#[test]
fn test_api_backend_invalid_url() {
    ApiBackend::new(
        "not a valid url".to_string(),
        Address::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap_err();
}
