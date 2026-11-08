#!/usr/bin/env python3
"""Sample a process's resident memory while it runs, and report it.

NFR-02 asks for idle RSS with a 5,000-row transcript: the number must
be flat, not just small, because a transcript that grows its memory
every frame fails the budget no matter where it starts. This script
reads `/proc/<pid>/status` VmRSS on an interval (the same method the
E00 spike used) and prints the series summary: the peak, the first and
last reading, and the growth across the run.

Usage:
    ./crates/pure-desktop/scripts/rss_sampler.py -- ./target/debug/pure-desktop --smoke 10000

The child runs to completion; the sampler samples until it exits, then
prints:

    rss: n=100 over 10.0s  settled=90  median=27.4 MB  peak=27.5 MB  tail drift=+0.2 MB  warmup peak=19.8 MB

`tail drift` is the last settled reading minus the first, which is the
number that says whether the process holds steady once it is warm; a
large positive value is a leak and a near-zero one is flat. The first
second is skipped as warmup (the font stack and the replay load there),
which is why the raw first-to-last growth of a short run reads as a
leak that is not there.
Linux only: the source is /proc.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import time
from pathlib import Path


def vm_rss_mb(pid: int) -> float | None:
    """The process's resident set in MiB, or None once it is gone."""
    try:
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) / 1024.0
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        return None
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--settle",
        type=float,
        default=1.0,
        help="seconds of warmup to skip before the plateau (default: 1.0)",
    )
    parser.add_argument(
        "--interval",
        type=float,
        default=0.1,
        help="seconds between samples (default: 0.1)",
    )
    parser.add_argument(
        "command",
        nargs=argparse.REMAINDER,
        help="the command to run, after --",
    )
    args = parser.parse_args()
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        parser.error("a command is required, after --")

    started = time.monotonic()
    child = subprocess.Popen(command)
    samples: list[tuple[float, float]] = []
    while child.poll() is None:
        value = vm_rss_mb(child.pid)
        if value is not None:
            samples.append((time.monotonic() - started, value))
        time.sleep(args.interval)
    code = child.wait()
    elapsed = time.monotonic() - started

    if not samples:
        print("rss: no samples (the process exited too fast)", file=sys.stderr)
        return code

    # The first samples land while the process is still loading the
    # font stack and the replay, so a plain first-to-last growth
    # measures the warmup and reads as a leak that is not there. The
    # number NFR-02 asks about is the plateau: the median and the peak
    # over the settled tail, and the drift across that tail.
    settled = [(t, mb) for t, mb in samples if t >= args.settle]
    if not settled:
        settled = samples
    values = sorted(mb for _t, mb in settled)
    median = values[len(values) // 2]
    peak = max(values)
    tail_first, tail_last = settled[0][1], settled[-1][1]
    print(
        f"rss: n={len(samples)} over {elapsed:.1f}s  "
        f"settled={len(settled)}  median={median:.1f} MB  peak={peak:.1f} MB  "
        f"tail drift={tail_last - tail_first:+.1f} MB  "
        f"warmup peak={max(mb for _t, mb in samples[: len(samples) // 4]):.1f} MB"
    )
    return code


if __name__ == "__main__":
    raise SystemExit(main())
