//! A registry's address, as Sleip and the list of plugins read it: its
//! scheme, its host and port, and its path; none of a user (`user@host`),
//! whose host would be the one after the `@`, nor of a space, a control
//! character or a backslash, which hosts read apart.

/// An address, of its parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address<'a> {
    /// Its scheme, as `https`, of lowercase letters.
    pub scheme: &'a str,
    /// Its host, as `pkg.lokyr.dev` or `[::1]`; empty of `file://`.
    pub host: &'a str,
    /// Its port, if it names one.
    pub port: Option<u16>,
    /// What follows its host: its path, its query and its fragment.
    pub path: &'a str,
}

impl<'a> Address<'a> {
    /// The address of `url`, of its parts, or none of an address that is
    /// none, or of a user.
    pub fn parse(url: &'a str) -> Option<Self> {
        if url.chars().any(|character| {
            character.is_control() || character.is_whitespace() || character == '\\'
        }) {
            return None;
        }
        let (scheme, after) = url.split_once("://")?;
        if scheme.is_empty() || !scheme.bytes().all(|byte| byte.is_ascii_lowercase()) {
            return None;
        }
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        let (authority, path) = after.split_at(end);
        if authority.contains('@') {
            return None;
        }
        let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
            let close = inner.find(']')?;
            let host = &authority[..close + 2];
            (host, &inner[close + 1..])
        } else {
            match authority.find(':') {
                Some(at) => (&authority[..at], &authority[at..]),
                None => (authority, ""),
            }
        };
        let port = match port.strip_prefix(':') {
            Some(digits) => Some(digits.parse().ok()?),
            None if port.is_empty() => None,
            None => return None,
        };
        Some(Self {
            scheme,
            host,
            port,
            path,
        })
    }

    /// Whether it is of `http://` of the machine itself, as the tests serve
    /// a registry.
    pub fn is_loopback(&self) -> bool {
        self.scheme == "http" && ["127.0.0.1", "localhost", "[::1]"].contains(&self.host)
    }

    /// Whether it is a registry's: of `https://` of a host, of `file://` of
    /// a folder of the machine's, as a mirror, or of `http://` of the
    /// machine itself.
    pub fn is_registry(&self) -> bool {
        match self.scheme {
            "https" => !self.host.is_empty(),
            "file" => self.host.is_empty() && self.port.is_none() && self.path.starts_with('/'),
            _ => self.is_loopback(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_tells_its_parts_and_none_of_a_user() {
        let address = Address::parse("https://pkg.lokyr.dev/index/acme/x?a#b").unwrap();
        assert_eq!(
            (address.scheme, address.host, address.port, address.path),
            ("https", "pkg.lokyr.dev", None, "/index/acme/x?a#b")
        );
        let local = Address::parse("http://127.0.0.1:8123/keys.json").unwrap();
        assert!(local.is_loopback() && local.is_registry());
        assert_eq!(local.port, Some(8123));
        assert!(Address::parse("http://[::1]:9/x").unwrap().is_loopback());
        assert!(Address::parse("http://localhost").unwrap().is_loopback());
        // A user's address, its host the one after its `@`.
        for url in [
            "http://localhost:1@evil.example/",
            "https://user@pkg.lokyr.dev/",
            "http://127.0.0.1@evil.example",
        ] {
            assert_eq!(Address::parse(url), None, "{url}");
        }
        for url in [
            "http://127.0.0.1.evil.example/x",
            "http://localhost.evil.example/x",
            "https://127.0.0.1/x",
            "http://example.com/x",
        ] {
            assert!(
                !Address::parse(url).is_some_and(|address| address.is_loopback()),
                "{url}"
            );
        }
        for url in [
            "https://a b/",
            "https://a\\b/",
            "https://x\u{1b}/",
            "HTTPS://x/",
            "https://x:port/",
            "https://[::1/",
            "pkg.lokyr.dev",
        ] {
            assert_eq!(Address::parse(url), None, "{url}");
        }
        assert!(Address::parse("file:///srv/mirror").unwrap().is_registry());
        assert!(!Address::parse("file://host/srv").unwrap().is_registry());
        assert!(!Address::parse("https:///x").unwrap().is_registry());
        assert!(!Address::parse("ftp://x/").unwrap().is_registry());
    }
}
