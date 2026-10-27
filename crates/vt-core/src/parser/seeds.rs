//! Seeds for the parser's fuzz target, taken from the inputs of the tests:
//! the corpus and the prefixes of the reference tests, and the lines of
//! Markus Kuhn's UTF-8 stress test that are not plain ASCII.

use std::path::PathBuf;
use std::{env, fs};

use super::reference::{CORPUS, PREFIXES, PROBE};

const KUHN: &[u8] = include_bytes!("../../testdata/UTF-8-test.txt");

/// Every seed.
fn seeds() -> Vec<Vec<u8>> {
    let corpus = CORPUS.iter().map(|input| input.to_vec());
    let prefixes = PREFIXES.iter().map(|&(prefix, _)| [prefix, PROBE].concat());
    let kuhn = KUHN
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_ascii())
        .map(<[u8]>::to_vec);
    corpus.chain(prefixes).chain(kuhn).collect()
}

#[test]
fn there_is_a_seed_for_every_input() {
    let seeds = seeds();
    assert!(seeds.len() > CORPUS.len() + PREFIXES.len());
    assert!(seeds.iter().all(|seed| !seed.is_empty()));
}

/// Writes the seeds, one per file, to the directory that the
/// `LOKYR_FUZZ_SEEDS` environment variable names.
#[test]
#[ignore = "it writes files, for `just fuzz`"]
fn export_fuzz_seeds() {
    let dir = PathBuf::from(
        env::var_os("LOKYR_FUZZ_SEEDS").expect("LOKYR_FUZZ_SEEDS names the seed directory"),
    );
    fs::create_dir_all(&dir).expect("the seed directory can be created");
    for (index, seed) in seeds().iter().enumerate() {
        fs::write(dir.join(format!("seed-{index:03}")), seed).expect("a seed can be written");
    }
}
