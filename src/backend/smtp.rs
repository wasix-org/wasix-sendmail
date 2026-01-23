use anyhow::Context;
use lettre::{
    address::{Address, Envelope},
    transport::smtp::{
        authentication::{Credentials, Mechanism},
        client::{CertificateStore, Tls, TlsParameters},
    },
    SmtpTransport, Transport,
};
use log::{debug, info};

use super::{BackendError, EmailBackend};
use crate::parser::EmailAddress;

pub struct SmtpBackend {
    transport: SmtpTransport,
}

pub enum TlsMode {
    Plain,
    Tls,
    StartTls,
    /// Attempt starttls if available, otherwise use plaintext
    StartTlsIfAvailable,
}

impl SmtpBackend {
    pub fn new(
        host: String,
        port: u16,
        tls_mode: TlsMode,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, BackendError> {
        info!("SMTP relay backend: creating relay via {}:{}", host, port);

        if host.is_empty() {
            return Err(BackendError::HostNotProvided);
        }

        let tls_params = TlsParameters::builder(host.clone())
            .certificate_store(CertificateStore::Default)
            .build_rustls()
            .context("Failed to build certificate store")?;

        let tls = match tls_mode {
            TlsMode::Plain => Tls::None,
            TlsMode::Tls => Tls::Wrapper(tls_params),
            TlsMode::StartTls => Tls::Required(tls_params),
            TlsMode::StartTlsIfAvailable => Tls::Opportunistic(tls_params),
        };

        let mut transport = SmtpTransport::relay(&host)
            .context("Invalid host name")?
            .port(port)
            .tls(tls);

        if username.is_some() || password.is_some() {
            debug!("SMTP relay backend: using authentication");
            let (Some(username), Some(password)) = (username, password) else {
                return Err(BackendError::OnlyUsernameOrPasswordProvided);
            };
            let credentials = Credentials::new(username, password);
            transport = transport
                .authentication(vec![Mechanism::Plain, Mechanism::Login])
                .credentials(credentials);
        } else {
            debug!("SMTP relay backend: not using authentication because no username or password was provided");
        };

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
        let raw_email_bytes = raw_email.as_bytes();

        let lettre_envelope_to = envelope_to
            .iter()
            .map(|envelope_to| {
                Address::new(envelope_to.local_part(), envelope_to.domain()).unwrap()
            })
            .collect::<Vec<_>>();
        let lettre_envelope_from =
            Address::new(envelope_from.local_part(), envelope_from.domain()).unwrap();
        let lettre_envelope = Envelope::new(Some(lettre_envelope_from), lettre_envelope_to)
            .context("Failed to create envelope")?;

        self.transport
            .send_raw(&lettre_envelope, raw_email_bytes)
            .context("Failed to send mail")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smtp_backend_default_sender() {
        let backend = SmtpBackend::new(
            "smtp.example.com".to_string(),
            587,
            TlsMode::StartTlsIfAvailable,
            None,
            None,
        )
        .unwrap();
        let default_sender = backend.default_sender();
        // The default sender should be username@localhost
        assert!(default_sender.as_str().ends_with("@localhost"));
    }
}
