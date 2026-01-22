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
        let mut builder = envelope_from
            .parse::<Mailboxes>()
            .context("Failed to parse envelope from address")?
            .into_iter()
            .fold(MessageBuilder::new(), |b, addr| b.from(addr));

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
        let subject = headers.iter()
            .find_map(|line| {
                let trimmed = line.trim();
                trimmed.find(':').and_then(|colon_pos| {
                    let header_name = trimmed[..colon_pos].trim();
                    if header_name.eq_ignore_ascii_case("Subject") {
                        Some(trimmed[colon_pos + 1..].trim())
                    } else {
                        None
                    }
                })
            });
        
        if let Some(subject_value) = subject {
            builder = builder.subject(subject_value);
            debug!("SMTP backend: subject={}", subject_value);
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
    let lines: Vec<&str> = email.lines().collect();
    let body_start = lines
        .iter()
        .position(|line| line.trim().is_empty())
        .map_or(lines.len(), |pos| pos + 1);

    let headers = lines[..body_start.saturating_sub(1)]
        .iter()
        .map(|&s| s.to_string())
        .collect();
    let body = lines.get(body_start..).map_or(String::new(), |b| b.join("\n"));

    (headers, body)
}
