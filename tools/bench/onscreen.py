#!/usr/bin/env python3
"""Times a file's output through the terminal that this runs in, on screen:
`cat` writes the file, then a query of the cursor's position, whose answer
the terminal gives once it has parsed everything before it, so that the time
ends as the terminal has taken the whole output, not as the PTY took the
last bytes. The same script in every terminal measures them alike.

    python3 tools/bench/onscreen.py <file> <result.json> [<runs>]

Appends one JSON line a run to the result: the file, its bytes, and the
seconds and MiB/s from `cat`'s start to the answer.
"""
import json
import os
import subprocess
import sys
import termios
import time
import tty


def answer(fd):
    """Asks for the cursor's position and reads the answer, `ESC [ r ; c R`."""
    os.write(fd, b"\x1b[6n")
    reply = b""
    while not reply.endswith(b"R"):
        reply += os.read(fd, 1)
    return reply


def main():
    path, result = sys.argv[1], sys.argv[2]
    runs = int(sys.argv[3]) if len(sys.argv) > 3 else 1
    size = os.path.getsize(path)
    fd = os.open("/dev/tty", os.O_RDWR)
    saved = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        for _ in range(runs):
            start = time.perf_counter()
            subprocess.run(["/bin/cat", path], stdout=fd, check=True)
            answer(fd)
            seconds = time.perf_counter() - start
            with open(result, "a") as out:
                out.write(json.dumps({
                    "file": os.path.basename(path),
                    "bytes": size,
                    "seconds": round(seconds, 4),
                    "mib_s": round(size / seconds / 1048576, 1),
                }) + "\n")
            # The screen, cleared between runs.
            os.write(fd, b"\x1b[H\x1b[2J\x1b[3J")
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, saved)
        os.close(fd)


if __name__ == "__main__":
    main()
