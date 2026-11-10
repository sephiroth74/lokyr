//! Sleip's requests of a registry: over HTTPS, of rustls of ring and the
//! system's trust store, through the platform's verifier, with no list of
//! roots of its own; of a folder of `file://`, a mirror's; and of
//! `http://` of the machine itself alone, as the tests serve a registry.
//! A file that the registry holds as Sleip holds it already, of its ETag,
//! comes as not modified. The files' integrity is their signatures', which
//! Sleip checks whatever carries them. An address is of no user; a request
//! of `https://` follows no redirect but to `https://`, and one of the
//! machine's own `http://` none; a file of `file://` is a plain file, read
//! within its bound; a request's deadline grows with the bytes that it
//! may bring.

use std::io::Read as _;
use std::time::Duration;

use lokyr_pkg_model::address::Address;

/// The most bytes of a file of the registry's index.
pub const MOST_BYTES: u64 = 16 * 1024 * 1024;

/// The longest that a request of no body takes, its answer's bytes among
/// it.
const DEADLINE: Duration = Duration::from_secs(30);

/// The slowest rate of an answer's bytes that a request's deadline makes
/// room for, beside [`DEADLINE`]: 256 KiB a second.
const SLOWEST: u64 = 256 * 1024;

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
    /// Of `https://`, its redirects of `https://` alone.
    secure: ureq::Agent,
    /// Of the machine's own `http://`, of no redirect.
    local: ureq::Agent,
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
        let agent = |secure: bool| -> ureq::Agent {
            let config = ureq::Agent::config_builder()
                .tls_config(tls.clone())
                .http_status_as_error(false)
                .https_only(secure)
                .max_redirects(if secure { 10 } else { 0 })
                .timeout_global(Some(DEADLINE))
                .user_agent(format!("sleip/{}", env!("CARGO_PKG_VERSION")))
                .build();
            config.into()
        };
        Self {
            secure: agent(true),
            local: agent(false),
        }
    }

    /// The file at `url`, of `https://`, `file://`, or `http://` of the
    /// machine itself; as not modified of `etag`, its ETag as Sleip last
    /// took it, if the registry holds it so; of [`MOST_BYTES`] at most.
    pub fn get(&self, url: &str, etag: Option<&str>) -> Result<Fetched, String> {
        self.get_within(url, etag, MOST_BYTES)
    }

    /// The file at `url`, as [`Client::get`] fetches it, of `most` bytes at
    /// most, as an archive's bound.
    pub fn get_within(&self, url: &str, etag: Option<&str>, most: u64) -> Result<Fetched, String> {
        let address = Address::parse(url)
            .filter(Address::is_registry)
            .ok_or_else(|| {
                format!("{url} is no registry's address: `https://`, or `file://` of a folder, of no user")
            })?;
        let agent = match address.scheme {
            "file" => return plain_file(address.path, most),
            "https" => &self.secure,
            _ => &self.local,
        };
        let mut request = agent
            .get(url)
            .config()
            .timeout_global(Some(DEADLINE + Duration::from_secs(most / SLOWEST)))
            .build();
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
                    .limit(most)
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

/// The file at `path`, a plain file of `most` bytes at most, read within
/// them: no folder, device nor pipe, which a read would never end of.
fn plain_file(path: &str, most: u64) -> Result<Fetched, String> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Fetched::Missing),
        Err(error) => return Err(format!("{path} cannot be read: {error}")),
    };
    if !metadata.is_file() {
        return Err(format!("{path} is no plain file"));
    }
    if metadata.len() > most {
        return Err(format!("{path} is more than {most} bytes"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(most + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("{path} cannot be read: {error}"))?;
    if bytes.len() as u64 > most {
        return Err(format!("{path} is more than {most} bytes"));
    }
    Ok(Fetched::Body { bytes, etag: None })
}

/// Whether `url` is of `http://` of the machine itself, as a registry of
/// the tests, of no user.
pub fn is_loopback(url: &str) -> bool {
    Address::parse(url).is_some_and(|address| address.is_loopback())
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
        assert!(!is_loopback("http://localhost:1@evil.example/"));
        let client = Client::new();
        assert!(client.get("http://example.com/keys.json", None).is_err());
        assert!(client.get("ftp://example.com/keys.json", None).is_err());
        let user = client
            .get("http://localhost:1@evil.example/keys.json", None)
            .unwrap_err();
        assert!(user.contains("of no user"), "{user}");
    }

    #[test]
    fn a_file_of_file_is_a_plain_file_read_within_its_bound() {
        let folder = std::env::temp_dir().join(format!("sleip-client-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("keys.json");
        std::fs::write(&file, "{}").unwrap();
        let client = Client::new();
        let url = |path: &std::path::Path| format!("file://{}", path.display());
        assert_eq!(
            client.get(&url(&file), None).unwrap(),
            Fetched::Body {
                bytes: b"{}".to_vec(),
                etag: None
            }
        );
        assert_eq!(
            client.get(&url(&folder.join("none")), None).unwrap(),
            Fetched::Missing
        );
        assert!(
            client
                .get_within(&url(&file), None, 1)
                .unwrap_err()
                .contains("more than 1 bytes")
        );
        // A folder, a device of no end, and a link to it.
        assert!(
            client
                .get(&url(&folder), None)
                .unwrap_err()
                .contains("no plain file")
        );
        assert!(
            client
                .get("file:///dev/zero", None)
                .unwrap_err()
                .contains("no plain file")
        );
        std::os::unix::fs::symlink("/dev/zero", folder.join("zero")).unwrap();
        assert!(
            client
                .get(&url(&folder.join("zero")), None)
                .unwrap_err()
                .contains("no plain file")
        );
        // A pipe, which a read would wait on.
        let pipe = folder.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap();
        assert!(made.success());
        assert!(
            client
                .get(&url(&pipe), None)
                .unwrap_err()
                .contains("no plain file")
        );
        // Of a host, no mirror's.
        assert!(client.get("file://host/srv/keys.json", None).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
