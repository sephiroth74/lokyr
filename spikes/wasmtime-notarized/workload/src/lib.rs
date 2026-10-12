//! Plugin-like workloads over terminal lines. The guest and the host compile
//! the same code, so the host can compare a call into the guest with the
//! same work done natively.

/// Words the highlighter marks as keywords.
const KEYWORDS: &[&str] = &[
    "DEBUG", "ERROR", "INFO", "WARN", "else", "false", "fn", "for", "if", "let", "match", "pub",
    "return", "true", "use",
];

/// Span kinds produced by [`highlight`].
pub mod kind {
    pub const IDENT: u8 = 0;
    pub const KEYWORD: u8 = 1;
    pub const NUMBER: u8 = 2;
    pub const STRING: u8 = 3;
    pub const PUNCT: u8 = 4;
}

/// `n` log-like lines of about 80 characters, always the same ones.
pub fn sample_lines(n: usize) -> Vec<String> {
    const PATHS: [&str; 4] = ["items", "users", "orders", "search"];
    const STATUSES: [u16; 4] = [200, 201, 404, 500];
    const LEVELS: [&str; 3] = ["INFO", "WARN", "DEBUG"];
    (0..n)
        .map(|i| {
            format!(
                "{i:05} {} server::handler: request id={} path=\"/api/v1/{}/{}\" took {}ms status={}",
                LEVELS[i % 3],
                (i * 7919) % 100_000,
                PATHS[i % 4],
                (i * 31) % 1000,
                (i * 13) % 250,
                STATUSES[(i / 4) % 4],
            )
        })
        .collect()
}

/// Fuzzy-matches `query` against `line`, like a command palette: the query's
/// characters must appear in order, ignoring ASCII case. Consecutive matches
/// and matches at the start of a word score higher. `None` means no match.
pub fn fuzzy_score(line: &str, query: &str) -> Option<u32> {
    let mut wanted = query.chars().map(|c| c.to_ascii_lowercase()).peekable();
    let mut score = 0;
    let mut previous_matched = false;
    let mut previous = ' ';
    for c in line.chars() {
        let Some(&next) = wanted.peek() else { break };
        if c.to_ascii_lowercase() == next {
            score += 1;
            if previous_matched {
                score += 2;
            }
            if !previous.is_alphanumeric() {
                score += 3;
            }
            wanted.next();
            previous_matched = true;
        } else {
            previous_matched = false;
        }
        previous = c;
    }
    wanted.peek().is_none().then_some(score)
}

/// Splits `line` into highlighted spans `(start, len, kind)`, like a simple
/// syntax highlighter: identifiers, keywords, numbers, strings and
/// punctuation. Whitespace produces no span.
pub fn highlight(line: &str) -> Vec<(u32, u32, u8)> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let b = bytes[i];
        let span_kind = if b.is_ascii_alphabetic() || b == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if KEYWORDS.contains(&&line[start..i]) {
                kind::KEYWORD
            } else {
                kind::IDENT
            }
        } else if b.is_ascii_digit() {
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            kind::NUMBER
        } else if b == b'"' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            kind::STRING
        } else if b.is_ascii_whitespace() {
            i += 1;
            continue;
        } else {
            i += 1;
            kind::PUNCT
        };
        spans.push((start as u32, (i - start) as u32, span_kind));
    }
    spans
}
