//! Seeds for the parser's fuzz target, taken from the inputs of the tests:
//! the corpus and the prefixes of the reference tests, and the lines of
//! Markus Kuhn's UTF-8 stress test that are not plain ASCII.

use std::path::PathBuf;
use std::{env, fs};

use super::reference::{CORPUS, PREFIXES, PROBE};

const KUHN: &[u8] = include_bytes!("../../testdata/UTF-8-test.txt");

/// Inline images' sequences, iTerm2's of headers of PNG, GIF and JPEG, and
/// kitty's of chunks, compression, placements, a query and deletions,
/// which arbitrary bytes would hardly make.
const IMAGES: [&[u8]; 6] = [
    b"\x1b]1337;File=inline=1;width=3;height=2:iVBORw0KGgoAAAANSUhEUgAAAAoAAAAKCAYAAAA=\x07",
    b"\x1b]1337;File=inline=1;doNotMoveCursor=1;preserveAspectRatio=0;width=50%;height=40px:R0lGODlhCgAKAA==\x1b\\",
    b"\x1b]1337;File=name=eA==;size=20;inline=1:/9j/4AAEAAD/wAARCAAKAAw=\x07",
    b"\x1b_Ga=T,f=100,i=1,m=1;iVBORw0KGgoAAAAN\x1b\\\x1b_Gm=0;SUhEUgAAAAoAAAAKCAYAAAA=\x1b\\",
    b"\x1b_Ga=t,f=24,s=4,v=4,o=z,I=7,q=1;eJzj4iINAAAuIAHh\x1b\\\x1b_Ga=p,I=7,c=3,r=2,C=1,z=-1,x=1,y=1,w=2,h=2\x1b\\",
    b"\x1b_Ga=q,f=32,s=1,v=1,i=31;AAAAAA==\x1b\\\x1b_Ga=d,d=A\x1b\\\x1b_Ga=t,t=f,i=2;L3RtcC94\x1b\\",
];

/// Every seed.
fn seeds() -> Vec<Vec<u8>> {
    let corpus = CORPUS.iter().map(|input| input.to_vec());
    let prefixes = PREFIXES.iter().map(|&(prefix, _)| [prefix, PROBE].concat());
    let kuhn = KUHN
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_ascii())
        .map(<[u8]>::to_vec);
    let images = IMAGES.iter().map(|input| input.to_vec());
    corpus.chain(prefixes).chain(kuhn).chain(images).collect()
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
