//! What a replay feeds to the terminal: a raw byte stream, or the output of
//! an asciicast recording (<https://docs.asciinema.org/manual/asciicast/v2/>
//! and v3).

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// The bytes to feed, in the chunks they came in.
#[derive(Debug, PartialEq, Eq)]
pub struct Recording {
    /// The size of the terminal that the recording was made in, if it says.
    pub size: Option<(u16, u16)>,
    /// The output, chunk by chunk.
    pub chunks: Vec<Vec<u8>>,
    /// How many resizes the recording has, which the replay leaves out.
    pub resizes: usize,
}

/// The size of the chunks of a raw stream, as a terminal session reads them.
const RAW_CHUNK: usize = 64 * 1024;

impl Recording {
    /// The recording in `bytes`: an asciicast if its first line is the
    /// header of one, a raw byte stream otherwise.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let first = bytes
            .split(|&byte| byte == b'\n')
            .next()
            .unwrap_or_default();
        match serde_json::from_slice::<Value>(first) {
            Ok(header @ Value::Object(_)) if header.get("version").is_some() => {
                Self::asciicast(&header, bytes)
            }
            _ => Ok(Self {
                size: None,
                chunks: bytes.chunks(RAW_CHUNK).map(<[u8]>::to_vec).collect(),
                resizes: 0,
            }),
        }
    }

    /// Every byte of the output, in one piece.
    #[cfg(test)]
    pub fn output(&self) -> Vec<u8> {
        self.chunks.concat()
    }

    fn asciicast(header: &Value, bytes: &[u8]) -> Result<Self> {
        let size = match header["version"].as_u64() {
            Some(2) => (&header["width"], &header["height"]),
            Some(3) => (&header["term"]["cols"], &header["term"]["rows"]),
            other => bail!("unsupported asciicast version {other:?}"),
        };
        let size = match size {
            (Value::Number(cols), Value::Number(rows)) => {
                Some((dimension(cols)?, dimension(rows)?))
            }
            _ => None,
        };
        let mut recording = Self {
            size,
            chunks: Vec::new(),
            resizes: 0,
        };
        let text = std::str::from_utf8(bytes).context("an asciicast is UTF-8")?;
        for (number, line) in (1..).zip(text.lines()).skip(1) {
            let line = line.trim();
            // Version 3 allows comments.
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let event: Value =
                serde_json::from_str(line).with_context(|| format!("line {number} is not JSON"))?;
            match (
                event.get(1).and_then(Value::as_str),
                event.get(2).and_then(Value::as_str),
            ) {
                (Some("o"), Some(data)) => recording.chunks.push(data.as_bytes().to_vec()),
                (Some("r"), Some(_)) => recording.resizes += 1,
                (Some(_), Some(_)) => {}
                _ => bail!("line {number} is not an event"),
            }
        }
        Ok(recording)
    }
}

fn dimension(number: &serde_json::Number) -> Result<u16> {
    number
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .context("a terminal size in cells")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_raw_stream_comes_in_chunks() {
        let bytes = vec![b'x'; RAW_CHUNK + 10];
        let recording = Recording::parse(&bytes).unwrap();
        assert_eq!(recording.size, None);
        assert_eq!(recording.chunks.len(), 2);
        assert_eq!(recording.output(), bytes);
    }

    #[test]
    fn a_raw_stream_can_start_like_json() {
        let recording = Recording::parse(b"{\"not\": \"a header\"}\nhello").unwrap();
        assert_eq!(recording.size, None);
        assert_eq!(recording.output(), b"{\"not\": \"a header\"}\nhello");
    }

    #[test]
    fn an_asciicast_2_gives_its_size_and_its_output() {
        let cast = concat!(
            "{\"version\": 2, \"width\": 40, \"height\": 10, \"timestamp\": 1}\n",
            "[0.1, \"o\", \"hello\\r\\n\"]\n",
            "[0.2, \"i\", \"typed\"]\n",
            "[0.3, \"r\", \"80x24\"]\n",
            "[0.4, \"o\", \"\\u001b[1mworld\"]\n",
        );
        let recording = Recording::parse(cast.as_bytes()).unwrap();
        assert_eq!(recording.size, Some((40, 10)));
        assert_eq!(recording.output(), b"hello\r\n\x1b[1mworld");
        assert_eq!(recording.resizes, 1);
    }

    #[test]
    fn an_asciicast_3_too() {
        let cast = concat!(
            "{\"version\": 3, \"term\": {\"cols\": 100, \"rows\": 30}}\n",
            "# a comment\n",
            "[0.5, \"o\", \"hi\"]\n",
            "[0.1, \"m\", \"\"]\n",
        );
        let recording = Recording::parse(cast.as_bytes()).unwrap();
        assert_eq!(recording.size, Some((100, 30)));
        assert_eq!(recording.output(), b"hi");
    }

    #[test]
    fn a_broken_asciicast_is_an_error() {
        let cast = "{\"version\": 2, \"width\": 10, \"height\": 5}\nnot json\n";
        assert!(Recording::parse(cast.as_bytes()).is_err());
        assert!(Recording::parse(b"{\"version\": 9}\n").is_err());
    }
}
