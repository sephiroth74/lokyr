//! A sample to edit: a few functions, comments and strings.

use std::collections::HashMap;

/// Counts the words of `text`.
fn count(text: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts
}

fn main() {
    let text = "the cat and the hat, café and 中文";
    let counts = count(text);
    for (word, n) in &counts {
        println!("{word}: {n}");
    }
}
