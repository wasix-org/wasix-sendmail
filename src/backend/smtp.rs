use lettre::{
    SmtpTransport, Transport,
    address::Envelope,
    transport::smtp::{
        authentication::{Credentials, Mechanism},
        client::{CertificateStore, Tls, TlsParameters},
    },
};
use log::{debug, info};
use rootcause::prelude::*;

use super::EmailBackend;

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
    ) -> Result<Self, Report> {
        info!("SMTP relay backend: creating relay via {}:{}", host, port);

        if host.is_empty() {
            return Err(report!("Host not provided"));
        }

        let tls_params = TlsParameters::builder(host.clone())
            .certificate_store(CertificateStore::Default)
            .build_rustls()
            .map_err(|e| {
                report!("Failed to build certificate store")
                    .attach(format!("Host: {}", host))
                    .attach(format!("Error: {}", e))
            })?;

        let tls = match tls_mode {
            TlsMode::Plain => Tls::None,
            TlsMode::Tls => Tls::Wrapper(tls_params),
            TlsMode::StartTls => Tls::Required(tls_params),
            TlsMode::StartTlsIfAvailable => Tls::Opportunistic(tls_params),
        };

        let mut transport = SmtpTransport::relay(&host)
            .map_err(|e| {
                report!("Invalid host name")
                    .attach(format!("Host: {}", host))
                    .attach(format!("Error: {}", e))
            })?
            .port(port)
            .tls(tls);

        if username.is_some() || password.is_some() {
            debug!("SMTP relay backend: using authentication");
            let (Some(username), Some(password)) = (username, password) else {
                return Err(report!("Username and password must be provided together"));
            };
            let credentials = Credentials::new(username, password);
            transport = transport
                .authentication(vec![Mechanism::Plain, Mechanism::Login])
                .credentials(credentials);
        } else {
            debug!(
                "SMTP relay backend: not using authentication because no username or password was provided"
            );
        };

        let transport = transport.build();

        Ok(Self { transport })
    }
}

impl EmailBackend for SmtpBackend {
    fn send(
        &self,
        envelope_from: &lettre::Address,
        envelope_to: &[&lettre::Address],
        raw_email: &str,
    ) -> Result<(), Report> {
        let raw_email_bytes = raw_email.as_bytes();

        let lettre_envelope_to = envelope_to.iter().map(|e| (*e).clone()).collect::<Vec<_>>();
        let lettre_envelope_from = envelope_from.clone();
        let lettre_envelope = Envelope::new(Some(lettre_envelope_from), lettre_envelope_to)
            .map_err(|e| {
                report!("Failed to create envelope")
                    .attach(format!("From: {}", envelope_from))
                    .attach(format!("To: {:?}", envelope_to))
                    .attach(format!("Error: {}", e))
            })?;

        self.transport
            .send_raw(&lettre_envelope, raw_email_bytes)
            .map_err(|e| report!("Failed to send mail").attach(format!("Error: {}", e)))?;
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
        assert_eq!(default_sender.domain(), "localhost");
    }
}
