#!/usr/bin/env python3
"""Writes the on-screen workloads into a directory, the same bytes each time:

    python3 tools/bench/workloads.py <dir>

- `ascii.txt`, 1 GiB of lines of ASCII text, as `cat` of a log shows them;
- `seq.txt`, `seq 1 10000000`'s output;
- `sgr.txt`, 128 MiB of lines whose every character changes its
  foreground color, of the 256's or a 24-bit one, and every third its
  background too and its weight to bold, the others' back to normal;
- `cjk.txt`, 128 MiB of lines of CJK, two columns each, and emoji, of
  their variation selectors, joiners and skin tones.
"""
import os
import sys

MIB = 1 << 20


def fill(path, line, size):
    """Writes `line` over and over into `path`, to `size` bytes."""
    data = line.encode()
    block = data * max(1, (4 * MIB) // len(data))
    with open(path, "wb") as out:
        written = 0
        while written < size:
            part = block[: size - written]
            out.write(part)
            written += len(part)


def sgr_line(n):
    out = []
    for column in range(80):
        k = (n * 7 + column) % 256
        if column % 3 == 0:
            out.append(f"\x1b[1;38;5;{k};48;5;{255 - k}m")
        else:
            out.append(f"\x1b[22;38;2;{k};{(k * 3) % 256};{(k * 7) % 256}m")
        out.append(chr(0x21 + (n + column) % 94))
    return "".join(out) + "\x1b[0m\n"


def main():
    directory = sys.argv[1]
    os.makedirs(directory, exist_ok=True)
    fill(
        os.path.join(directory, "ascii.txt"),
        "The quick brown fox jumps over the lazy dog, 0123456789, and then some more text.\n",
        1 << 30,
    )
    with open(os.path.join(directory, "seq.txt"), "w") as out:
        for n in range(1, 10_000_001):
            out.write(f"{n}\n")
    sgr = "".join(sgr_line(n) for n in range(64))
    fill(os.path.join(directory, "sgr.txt"), sgr, 128 * MIB)
    cjk = (
        "漢字かなカナ한글中文日本語 🦀🌍✨ 👩🏽‍💻👨‍👩‍👧 ❤️✔️ 🇮🇹🇯🇵 汉语拼音 東京大阪 서울부산 😀😃😄\n"
    )
    fill(os.path.join(directory, "cjk.txt"), cjk, 128 * MIB)


if __name__ == "__main__":
    main()
