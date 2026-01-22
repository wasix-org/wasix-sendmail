use anyhow::Context;
use lettre::{
    address::{Address, Envelope},
    transport::smtp::{
        authentication::{Credentials, Mechanism},
        client::{CertificateStore, TlsParameters},
    },
    SmtpTransport, Transport,
};
use log::{debug, info};

use super::{BackendError, EmailBackend};
use crate::parser::EmailAddress;

pub struct SmtpBackend {
    transport: SmtpTransport,
}

impl SmtpBackend {
    pub fn new(
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, BackendError> {
        info!("SMTP relay backend: creating relay via {}:{}", host, port);

        if host.is_empty() {
            return Err(BackendError::HostNotProvided);
        }

        // Validate authentication
        let credentials = if username.is_some() || password.is_some() {
            let (Some(username), Some(password)) = (username, password) else {
                return Err(BackendError::OnlyUsernameOrPasswordProvided);
            };
            Some(Credentials::new(username, password))
        } else {
            None
        };

        // TLS params
        let tls: TlsParameters = TlsParameters::builder(host.clone())
            .certificate_store(CertificateStore::Default)
            .build_rustls()
            .context("Failed to build certificate store")?;

        // Transport builder
        let mut transport = SmtpTransport::relay(&host)
            .context("Invalid host name")?
            .port(port)
            .tls(lettre::transport::smtp::client::Tls::Opportunistic(tls));

        // Authentication
        if let Some(credentials) = credentials {
            transport = transport
                .authentication(vec![Mechanism::Login])
                .credentials(credentials);
        }

        let transport = transport.build();

        Ok(Self { transport })
    }
}

impl EmailBackend for SmtpBackend {
    fn send(
        &self,
        envelope_from: &EmailAddress,
        envelope_to: &[&EmailAddress],
        raw_email: &str,
    ) -> Result<(), BackendError> {
        info!(
            "SMTP relay backend: sending from {} to {} recipient(s)",
            envelope_from,
            envelope_to.len()
        );

        if envelope_to.is_empty() {
            debug!("SMTP relay backend: empty recipient list; nothing to send");
            return Ok(());
        }

        let raw_email_bytes = raw_email.as_bytes();

        let lettre_envelope_to = envelope_to
            .iter()
            .map(|envelope_to| {
                Address::new(envelope_to.local_part(), envelope_to.domain()).unwrap()
            })
            .collect::<Vec<_>>();
        let lettre_envelope_from =
            Address::new(envelope_from.local_part(), envelope_from.domain()).unwrap();
        let envelope = Envelope::new(Some(lettre_envelope_from), lettre_envelope_to)
            .context("Failed to create envelope")?;

        self.transport
            .send_raw(&envelope, raw_email_bytes)
            .context("Failed to send mail")?;
        info!("SMTP relay backend: send complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smtp_backend_default_sender() {
        let backend = SmtpBackend::new("smtp.example.com".to_string(), 587, None, None).unwrap();
        let default_sender = backend.default_sender();
        // The default sender should be username@localhost
        assert!(default_sender.as_str().ends_with("@localhost"));
    }
}
