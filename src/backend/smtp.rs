use anyhow::Context;
use lettre::{
    message::{Mailboxes, MessageBuilder},
    transport::smtp::{
        authentication::{Credentials, Mechanism},
        client::{CertificateStore, TlsParameters},
    },
    SmtpTransport, Transport,
};
use log::{debug, info, trace};

use super::{BackendError, EmailBackend};

pub struct SmtpBackend {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
}

impl SmtpBackend {
    pub fn new(
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    ) -> Self {
        Self {
            host,
            port,
            username,
            password,
        }
    }
}

impl EmailBackend for SmtpBackend {
    fn send(
        &self,
        envelope_from: &str,
        envelope_to: &[&str],
        raw_email: &str,
    ) -> Result<(), BackendError> {
        info!(
            "SMTP backend: sending via {}:{} ({} recipient(s))",
            self.host,
            self.port,
            envelope_to.len()
        );
        debug!("SMTP backend: envelope-from={}", envelope_from);
        trace!("SMTP backend: raw_email_bytes={}", raw_email.len());

        if std::env::var("SSL_CERT_DIR").is_err() {
            std::env::set_var("SSL_CERT_DIR", "/openssl/ssl/certs");
            debug!("SMTP backend: set SSL_CERT_DIR=/openssl/ssl/certs");
        }

        if self.host.is_empty() {
            return Err(BackendError::HostNotProvided);
        }
        if envelope_from.is_empty() {
            return Err(BackendError::FromNotProvided);
        }
        if envelope_to.is_empty() {
            debug!("SMTP backend: empty recipient list; nothing to send");
            return Ok(()); // Empty recipient list, nothing to send
        }

        // Validate authentication
        if self.username.is_some() != self.password.is_some() {
            return Err(BackendError::OnlyUsernameOrPasswordProvided);
        }
        if self.username.is_some() {
            debug!("SMTP backend: authentication enabled (Login)");
        } else {
            debug!("SMTP backend: authentication disabled");
        }

        // Parse raw email to extract headers and body
        let (headers, body) = parse_raw_email(raw_email);
        trace!(
            "SMTP backend: parsed headers={} body_bytes={}",
            headers.len(),
            body.len()
        );

        // Build message from raw email
        let mut builder = MessageBuilder::new();

        // Set envelope from
        for addr in envelope_from
            .parse::<Mailboxes>()
            .context("Failed to parse envelope from address")?
        {
            builder = builder.from(addr);
        }

        // Set envelope to recipients
        for to_addr in envelope_to {
            for addr in to_addr
                .parse::<Mailboxes>()
                .context("Failed to parse envelope to address")?
            {
                builder = builder.to(addr);
            }
        }

        // Parse Subject header if present (most common header)
        // Other headers will remain in the body
        let mut subject: Option<&str> = None;
        for header_line in &headers {
            let trimmed = header_line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Extract Subject header value
            if let Some(colon_pos) = trimmed.find(':') {
                let header_name = trimmed[..colon_pos].trim();
                if header_name.eq_ignore_ascii_case("Subject") {
                    let subject_value = trimmed[colon_pos + 1..].trim();
                    builder = builder.subject(subject_value);
                    subject = Some(subject_value);
                    break; // Found subject, no need to continue
                }
            }
        }
        if let Some(subject) = subject {
            debug!("SMTP backend: subject={}", subject);
        } else {
            trace!("SMTP backend: no Subject header found");
        }

        // Set body (which includes any unparsed headers)
        let email = builder.body(body).context("Failed to build message")?;

        // TLS params
        let tls = TlsParameters::builder(self.host.clone())
            .certificate_store(CertificateStore::Default)
            .build_rustls()
            .context("Failed to build certificate store")?;

        // Transport builder
        let mut transport = SmtpTransport::relay(&self.host)
            .context("Invalid host name")?
            .port(self.port)
            .tls(lettre::transport::smtp::client::Tls::Opportunistic(tls));

        // Authentication
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            transport = transport
                .authentication(vec![Mechanism::Login])
                .credentials(Credentials::new(username.clone(), password.clone()));
        }

        // Send
        debug!("SMTP backend: connecting and sending");
        transport
            .build()
            .send(&email)
            .context("Failed to send mail")?;
        info!("SMTP backend: send complete");
        Ok(())
    }
}

/// Parse raw email content into headers and body
fn parse_raw_email(email: &str) -> (Vec<String>, String) {
    let mut headers = Vec::new();
    let mut body_start = 0;
    let lines: Vec<&str> = email.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            // Empty line separates headers from body
            body_start = i + 1;
            break;
        }
        headers.push(line.to_string());
    }

    let body = if body_start < lines.len() {
        lines[body_start..].join("\n")
    } else {
        String::new()
    };

    (headers, body)
}
