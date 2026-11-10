//! Sleip's requests of a registry: over HTTPS, of rustls of ring and the
//! system's trust store, through the platform's verifier, with no list of
//! roots of its own; of a folder of `file://`, a mirror's; and of
//! `http://` of the machine itself alone, as the tests serve a registry.
//! A file that the registry holds as Sleip holds it already, of its ETag,
//! comes as not modified. The files' integrity is their signatures', which
//! Sleip checks whatever carries them.

use std::time::Duration;

/// The most bytes of a file of the registry's index.
pub const MOST_BYTES: u64 = 16 * 1024 * 1024;

/// The longest that a request takes, its answer's bytes among it.
const DEADLINE: Duration = Duration::from_secs(30);

/// What a request of a file brought.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// The file, and its ETag, if the registry tells one.
    Body {
        /// Its bytes.
        bytes: Vec<u8>,
        /// Its ETag, to ask of it again.
        etag: Option<String>,
    },
    /// The file, as Sleip holds it already, of the ETag asked of.
    NotModified,
    /// No such file.
    Missing,
}

/// The client of a registry's requests.
pub struct Client {
    agent: ureq::Agent,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// A client of rustls of ring, of the system's trust store.
    pub fn new() -> Self {
        // The process's provider of rustls, ring, once; a second time
        // finds it there.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .timeout_global(Some(DEADLINE))
            .user_agent(format!("sleip/{}", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: config.into(),
        }
    }

    /// The file at `url`, of `https://`, `file://`, or `http://` of the
    /// machine itself; as not modified of `etag`, its ETag as Sleip last
    /// took it, if the registry holds it so.
    pub fn get(&self, url: &str, etag: Option<&str>) -> Result<Fetched, String> {
        if let Some(path) = url.strip_prefix("file://") {
            return match std::fs::read(path) {
                Ok(bytes) => Ok(Fetched::Body { bytes, etag: None }),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Fetched::Missing),
                Err(error) => Err(format!("{path} cannot be read: {error}")),
            };
        }
        if !url.starts_with("https://") && !is_loopback(url) {
            return Err(format!(
                "{url} is no registry's address: `https://`, or `file://` of a folder"
            ));
        }
        let mut request = self.agent.get(url);
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let response = request
            .call()
            .map_err(|error| format!("{url} cannot be fetched: {error}"))?;
        match response.status().as_u16() {
            200 => {
                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned);
                let bytes = response
                    .into_body()
                    .with_config()
                    .limit(MOST_BYTES)
                    .read_to_vec()
                    .map_err(|error| format!("{url} cannot be read: {error}"))?;
                Ok(Fetched::Body { bytes, etag })
            }
            304 => Ok(Fetched::NotModified),
            404 => Ok(Fetched::Missing),
            status => Err(format!("{url} answers {status}")),
        }
    }
}

/// Whether `url` is of `http://` of the machine itself, as a registry of
/// the tests.
pub fn is_loopback(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    ["127.0.0.1", "localhost", "[::1]"].iter().any(|loopback| {
        host == *loopback
            || host
                .strip_prefix(loopback)
                .is_some_and(|port| port.starts_with(':'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_is_taken_of_the_machine_itself_alone() {
        assert!(is_loopback("http://127.0.0.1:8123/keys.json"));
        assert!(is_loopback("http://localhost/x"));
        assert!(is_loopback("http://[::1]:9/x"));
        assert!(!is_loopback("http://127.0.0.1.evil.example/x"));
        assert!(!is_loopback("http://localhost.evil.example/x"));
        assert!(!is_loopback("https://127.0.0.1/x"));
        let client = Client::new();
        assert!(client.get("http://example.com/keys.json", None).is_err());
        assert!(client.get("ftp://example.com/keys.json", None).is_err());
    }
}
