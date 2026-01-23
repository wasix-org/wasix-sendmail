// The mock http server does currently not work on WASIX
#![allow(unexpected_cfgs)]
#![cfg(not(target_vendor = "wasmer"))]
use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::Duration;
use tiny_http::{Response, Server, StatusCode};
use wasix_sendmail::backend::EmailBackend;
use wasix_sendmail::backend::api::ApiBackend;
use wasix_sendmail::parser::EmailAddress;

fn email_address(addr: &str) -> EmailAddress {
    EmailAddress::from_str(addr).expect("valid email address")
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

/// Helper that counts requests
fn start_counting_server(
    status: u16,
    body: &'static str,
) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let addr = server.server_addr().to_string();
    let url = format!("http://{}", addr);
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let handle = thread::spawn(move || {
        while let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(500)) {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            let response = Response::from_string(body).with_status_code(StatusCode(status));
            let _ = request.respond(response);
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(50));

    (url, counter, handle)
}

#[test]
fn test_api_backend_successful_send() {
    let (url, handle) = start_mock_server(202, "Message accepted");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(),
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
        EmailAddress::from_str("default@example.com").unwrap(),
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
        EmailAddress::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap_err();
}

#[test]
fn test_api_backend_bad_request_error() {
    let (url, handle) = start_mock_server(400, "Invalid email format");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("400 Bad Request"));
    assert!(err_msg.contains("Invalid email format"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_unauthorized_error() {
    let (url, handle) = start_mock_server(401, "Invalid token");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(),
        "invalid-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("401 Unauthorized"));
    assert!(err_msg.contains("Invalid token"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_quota_exceeded_error() {
    let (url, handle) = start_mock_server(402, "Monthly quota exceeded");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("402 Payment Required"));
    assert!(err_msg.contains("Monthly quota exceeded"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_forbidden_error() {
    let (url, handle) = start_mock_server(403, "Sender not authorized");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("403 Forbidden"));
    assert!(err_msg.contains("Sender not authorized"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_message_too_large_error() {
    let (url, handle) = start_mock_server(413, "Message exceeds 10MB limit");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(),
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
    assert!(err_msg.contains("413 Payload Too Large"));
    assert!(err_msg.contains("Message exceeds 10MB limit"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_server_error() {
    let (url, handle) = start_mock_server(503, "Service temporarily unavailable");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("503 Server Error"));
    assert!(err_msg.contains("Service temporarily unavailable"));

    let _ = handle.join();
}

#[test]
fn test_api_backend_unexpected_status() {
    let (url, handle) = start_mock_server(418, "I'm a teapot");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(),
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
        EmailAddress::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("sender@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("400 Bad Request"));
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
        EmailAddress::from_str("default@example.com").unwrap(),
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

#[test]
fn test_api_backend_uses_envelope_from_not_default_sender() {
    let (url, handle) = start_mock_server(202, "");

    let backend = ApiBackend::new(
        format!("{}/send", url),
        EmailAddress::from_str("default@example.com").unwrap(), // This should NOT be used
        "test-token".to_string(),
    )
    .unwrap();

    let from = email_address("envelope@example.com");
    let to = email_address("recipient@example.com");
    let raw_email = "Subject: Test\r\n\r\nTest body";

    let result = backend.send(&from, &[&to], raw_email);
    assert!(result.is_ok());

    let _ = handle.join();
}

#[test]
fn test_api_backend_network_timeout() {
    // Test with an unreachable address (should timeout or fail immediately)
    let backend = ApiBackend::new(
        "http://192.0.2.1:9999/send".to_string(), // TEST-NET-1, non-routable
        EmailAddress::from_str("default@example.com").unwrap(),
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
        EmailAddress::from_str("default@example.com").unwrap(),
        "test-token".to_string(),
    )
    .unwrap_err();
}
