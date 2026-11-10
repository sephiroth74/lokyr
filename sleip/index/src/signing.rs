//! The registry's keys and the signatures of its index, of ed25519: a
//! root key, offline, the maintainer's, whose public part Sleip holds,
//! signs `keys.json`, the list of the index keys; an index key, which the
//! registry's CI holds, signs each index file whole on its last line, of
//! the plugin's name and of the time of its generation. Sleip refuses a
//! file of a key that the list lacks, and one older than it holds.
//!
//! A key and a signature are written `ed25519:` and the base64 of their
//! bytes; a secret key, in a file of its own, `ed25519-secret:` and the
//! base64 of its seed.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use serde::{Deserialize, Serialize};

/// The public parts of the root keys, which `keys.json`'s signature is
/// checked against: a new root comes with a new Sleip.
pub const ROOT_KEYS: &[&str] = &[];

/// The prefix of a public key and of a signature.
const PUBLIC: &str = "ed25519:";

/// The prefix of a secret key.
const SECRET: &str = "ed25519-secret:";

/// The version of `keys.json` and of an index's lines.
pub const VERSION: u32 = 1;

/// A public key, of a root or of the index.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PublicKey(VerifyingKey);

impl PublicKey {
    /// The key of `text`, `ed25519:` and the base64 of its 32 bytes.
    pub fn parse(text: &str) -> Result<Self, String> {
        let bytes: [u8; 32] = decoded(text, PUBLIC)?
            .try_into()
            .map_err(|_| format!("`{text}` is not a key of 32 bytes"))?;
        VerifyingKey::from_bytes(&bytes)
            .map(Self)
            .map_err(|_| format!("`{text}` is not a key of ed25519"))
    }

    /// Whether `signature`, as `ed25519:` and the base64 of its 64 bytes,
    /// is this key's of `message`.
    pub fn verifies(&self, message: &[u8], signature: &str) -> bool {
        let Ok(bytes) = decoded(signature, PUBLIC) else {
            return false;
        };
        let Ok(bytes): Result<[u8; 64], _> = bytes.try_into() else {
            return false;
        };
        self.0
            .verify(message, &Signature::from_bytes(&bytes))
            .is_ok()
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PUBLIC}{}", STANDARD.encode(self.0.as_bytes()))
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

/// A secret key, which signs: never written but to its own file, nor
/// told.
pub struct SecretKey(SigningKey);

impl SecretKey {
    /// A new key, of the system's randomness.
    pub fn generate() -> Result<Self, String> {
        let mut seed = [0; 32];
        getrandom::fill(&mut seed)
            .map_err(|error| format!("the system gives no randomness: {error}"))?;
        Ok(Self(SigningKey::from_bytes(&seed)))
    }

    /// The key of `text`, `ed25519-secret:` and the base64 of its seed,
    /// as its file holds it, of the spaces and the line's end around it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let seed: [u8; 32] = decoded(text.trim(), SECRET)
            .map_err(|_| "it is not `ed25519-secret:` and the base64 of 32 bytes".to_owned())?
            .try_into()
            .map_err(|_| "it is not `ed25519-secret:` and the base64 of 32 bytes".to_owned())?;
        Ok(Self(SigningKey::from_bytes(&seed)))
    }

    /// The text of its file, of a line.
    pub fn file_text(&self) -> String {
        format!("{SECRET}{}\n", STANDARD.encode(self.0.to_bytes()))
    }

    /// Its public part.
    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.verifying_key())
    }

    /// Its signature of `message`, `ed25519:` and the base64 of its 64
    /// bytes.
    pub fn sign(&self, message: &[u8]) -> String {
        format!(
            "{PUBLIC}{}",
            STANDARD.encode(self.0.sign(message).to_bytes())
        )
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretKey({})", self.public())
    }
}

/// The bytes of `text`, of its prefix `prefix` and its base64.
fn decoded(text: &str, prefix: &str) -> Result<Vec<u8>, String> {
    let encoded = text
        .strip_prefix(prefix)
        .ok_or_else(|| format!("`{text}` is not of `{prefix}`"))?;
    STANDARD
        .decode(encoded)
        .map_err(|_| format!("`{text}` is not of base64"))
}

/// The root keys that Sleip holds.
pub fn roots() -> Vec<PublicKey> {
    ROOT_KEYS
        .iter()
        .filter_map(|text| PublicKey::parse(text).ok())
        .collect()
}

/// A time of a file's generation, in seconds since 1970 in UTC, written as
/// RFC 3339 writes it, `2026-11-10T09:00:00Z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Now.
    pub fn now() -> Self {
        Self(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
        )
    }

    /// The time of `text`, as [`Timestamp`]'s text writes it, and no other
    /// form.
    pub fn parse(text: &str) -> Result<Self, String> {
        let wrong = || format!("`{text}` is not a time of `YYYY-MM-DDTHH:MM:SSZ`");
        let bytes = text.as_bytes();
        if bytes.len() != 20
            || [4, 7].iter().any(|&at| bytes[at] != b'-')
            || bytes[10] != b'T'
            || [13, 16].iter().any(|&at| bytes[at] != b':')
            || bytes[19] != b'Z'
        {
            return Err(wrong());
        }
        let number = |from: usize, to: usize| -> Result<i64, String> {
            let part = &text[from..to];
            if part.bytes().all(|byte| byte.is_ascii_digit()) {
                part.parse().map_err(|_| wrong())
            } else {
                Err(wrong())
            }
        };
        let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
        let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return Err(wrong());
        }
        let days = days_from_civil(year, month, day);
        let seconds = days * 86_400 + hour * 3600 + minute * 60 + second;
        let seconds = u64::try_from(seconds).map_err(|_| wrong())?;
        let back = Self(seconds);
        // A day past its month's end, as `02-30`, writes back otherwise.
        if back.to_string() == text {
            Ok(back)
        } else {
            Err(wrong())
        }
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (days, rest) = (self.0 / 86_400, self.0 % 86_400);
        // The civil date of the days since 1970-01-01, after Howard
        // Hinnant's `civil_from_days`
        // (https://howardhinnant.github.io/date_algorithms.html).
        let days = i64::try_from(days).unwrap_or(0) + 719_468;
        let era = days.div_euclid(146_097);
        let of_era = days.rem_euclid(146_097);
        let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
        let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * of_year + 2) / 153;
        let day = of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
            rest / 3600,
            rest % 3600 / 60,
            rest % 60
        )
    }
}

/// The days since 1970-01-01 of the civil date `year`, `month` and `day`,
/// after Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year.rem_euclid(400);
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

/// `keys.json`, as it is written.
#[derive(Deserialize, Serialize)]
struct KeysFile {
    v: u32,
    keys: Vec<String>,
    generated: String,
    sig: String,
}

/// The index keys of a `keys.json` checked, and its time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys {
    /// The index keys.
    pub keys: Vec<PublicKey>,
    /// When it was made.
    pub generated: Timestamp,
}

/// What a root key signs of `keys.json`: its time, then each of its keys,
/// a line each.
fn keys_message(generated: &str, keys: &[String]) -> Vec<u8> {
    let mut message = format!("sleip keys.json v{VERSION}\n{generated}\n");
    for key in keys {
        message.push_str(key);
        message.push('\n');
    }
    message.into_bytes()
}

/// The text of `keys.json` of the index keys `keys`, made at `generated`,
/// signed by the root key `root`.
pub fn sign_keys(root: &SecretKey, keys: &[PublicKey], generated: Timestamp) -> String {
    let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
    let generated = generated.to_string();
    let sig = root.sign(&keys_message(&generated, &keys));
    let file = KeysFile {
        v: VERSION,
        keys,
        generated,
        sig,
    };
    // A struct of texts serializes.
    serde_json::to_string(&file).unwrap_or_default() + "\n"
}

/// The index keys of `text`, a `keys.json`, checked: signed by one of
/// `roots`, and not older than `held`, the time of the one that Sleip
/// holds, if it holds one.
pub fn verify_keys(
    text: &str,
    roots: &[PublicKey],
    held: Option<Timestamp>,
) -> Result<Keys, String> {
    let file: KeysFile =
        serde_json::from_str(text).map_err(|error| format!("keys.json does not read: {error}"))?;
    if file.v != VERSION {
        return Err(format!(
            "keys.json is of version {}, which this Sleip does not read: a newer Sleip does",
            file.v
        ));
    }
    let message = keys_message(&file.generated, &file.keys);
    if !roots.iter().any(|root| root.verifies(&message, &file.sig)) {
        return Err("keys.json is signed by no root key of this Sleip's".to_owned());
    }
    let generated = Timestamp::parse(&file.generated)?;
    if let Some(held) = held
        && generated < held
    {
        return Err(format!(
            "keys.json was made at {generated}, before the one that this Sleip holds, of {held}"
        ));
    }
    let keys = file
        .keys
        .iter()
        .map(|key| PublicKey::parse(key))
        .collect::<Result<_, _>>()?;
    Ok(Keys { keys, generated })
}

/// A version of a plugin, a line of its index file.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Entry {
    /// The line's version.
    pub v: u32,
    /// The plugin, `owner/name`.
    pub name: String,
    /// Its version.
    pub vers: String,
    /// Its kind, as its manifest writes it.
    pub kind: String,
    /// The plugin API that it takes, if it names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// The plugins that it depends on, each of its constraint.
    #[serde(default)]
    pub deps: Vec<Dependency>,
    /// The capabilities that it declares.
    #[serde(default)]
    pub caps: Vec<String>,
    /// The commands of others that it runs.
    #[serde(default)]
    pub uses: Vec<String>,
    /// The SHA-256 of its archive, `sha256:` and its hexadecimal.
    pub cksum: String,
    /// Whether it is yanked.
    pub yanked: bool,
}

/// A plugin that a version depends on, of its constraint.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Dependency {
    /// The plugin, `owner/name`.
    pub name: String,
    /// Its constraint, as Cargo writes one.
    pub req: String,
}

/// An index file's last line, its signature.
#[derive(Deserialize, Serialize)]
struct Signed {
    generated: String,
    key: String,
    sig: String,
}

/// The versions of an index file checked, and its time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Index {
    /// Its versions, in their order.
    pub entries: Vec<Entry>,
    /// When it was made.
    pub generated: Timestamp,
}

/// What an index key signs of the index file of the plugin `name`: the
/// plugin, its time, then its lines as they are, each of its line's end.
fn index_message(name: &str, generated: &str, body: &str) -> Vec<u8> {
    format!("sleip index v{VERSION}\n{name}\n{generated}\n{body}").into_bytes()
}

/// The text of the index file of the plugin `name`, of `entries`, made at
/// `generated`, signed by the index key `key`.
pub fn sign_index(name: &str, entries: &[Entry], key: &SecretKey, generated: Timestamp) -> String {
    let mut body = String::new();
    for entry in entries {
        // A struct of texts, numbers and lists serializes.
        body.push_str(&serde_json::to_string(entry).unwrap_or_default());
        body.push('\n');
    }
    let generated = generated.to_string();
    let signed = Signed {
        sig: key.sign(&index_message(name, &generated, &body)),
        key: key.public().to_string(),
        generated,
    };
    body + &serde_json::to_string(&signed).unwrap_or_default() + "\n"
}

/// The versions of `text`, the index file of the plugin `name`, checked:
/// signed whole by one of the index keys of `keys`, each line the
/// plugin's, and not older than `held`, the time of the one that Sleip
/// holds, if it holds one.
pub fn verify_index(
    text: &str,
    name: &str,
    keys: &Keys,
    held: Option<Timestamp>,
) -> Result<Index, String> {
    let file = format!("the index of `{name}`");
    let trimmed = text
        .strip_suffix('\n')
        .ok_or_else(|| format!("{file} does not end with a line's end"))?;
    let (body, last) = match trimmed.rfind('\n') {
        Some(at) => (&text[..=at], &trimmed[at + 1..]),
        None => ("", trimmed),
    };
    let signed: Signed =
        serde_json::from_str(last).map_err(|_| format!("{file} ends with no signature"))?;
    let key = PublicKey::parse(&signed.key)?;
    if !keys.keys.contains(&key) {
        return Err(format!(
            "{file} is signed by {key}, which keys.json does not list"
        ));
    }
    if !key.verifies(&index_message(name, &signed.generated, body), &signed.sig) {
        return Err(format!("{file} is not as its signature says"));
    }
    let generated = Timestamp::parse(&signed.generated)?;
    if let Some(held) = held
        && generated < held
    {
        return Err(format!(
            "{file} was made at {generated}, before the one that this Sleip holds, of {held}"
        ));
    }
    let mut entries = Vec::new();
    for line in body.lines() {
        let entry: Entry = serde_json::from_str(line)
            .map_err(|error| format!("{file} has a line that does not read: {error}"))?;
        if entry.v != VERSION {
            return Err(format!(
                "{file} is of version {}, which this Sleip does not read: a newer Sleip does",
                entry.v
            ));
        }
        if entry.name != name {
            return Err(format!("{file} has a version of `{}`", entry.name));
        }
        entries.push(entry);
    }
    Ok(Index { entries, generated })
}

#[cfg(test)]
mod tests;
