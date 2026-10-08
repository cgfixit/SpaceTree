#!/usr/bin/env python3
"""Compare two SpaceTree CLI scans on the same temporary fixture."""

import json
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time


def scan(binary, root):
    started = time.perf_counter_ns()
    result = subprocess.run([str(binary), "--scan", str(root)], capture_output=True, text=True)
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
    if result.returncode:
        raise SystemExit(f"{binary}: scan failed: {result.stderr.strip()}")
    lines = result.stdout.splitlines()
    if len(lines) < 3 or not lines[1].startswith("root_size_bytes="):
        raise SystemExit(f"{binary}: unexpected scan report")
    return elapsed_ms, {
        "root_size_bytes": int(lines[1].split("=", 1)[1]),
        "report_rows": len(lines) - 2,
    }


def main():
    if len(sys.argv) != 3:
        raise SystemExit(f"usage: {sys.argv[0]} BASELINE_BINARY CANDIDATE_BINARY")
    binaries = [Path(arg).resolve(strict=True) for arg in sys.argv[1:]]
    samples = [[], []]
    reference = None
    with tempfile.TemporaryDirectory(prefix="spacetree-bench-") as temp:
        root = Path(temp)
        payload = b"x" * 4096
        for directory in range(30):
            folder = root / f"dir-{directory:02}"
            folder.mkdir()
            for file in range(200):
                (folder / f"file-{file:03}.bin").write_bytes(payload)
        for cycle in range(8):  # First run of each binary warms the cache.
            for index in ((0, 1) if cycle % 2 == 0 else (1, 0)):
                elapsed_ms, report = scan(binaries[index], root)
                if reference is None:
                    reference = report
                elif report != reference:
                    raise SystemExit(f"scan totals differ: {binaries[index]}: {report}")
                if cycle:
                    samples[index].append(round(elapsed_ms, 3))
    print(json.dumps({
        "files": 6000,
        "report": reference,
        "binaries": [
            {"path": str(binary), "median_ms": round(statistics.median(times), 3),
             "warm_cache_ms": times}
            for binary, times in zip(binaries, samples)
        ],
    }, indent=2))


if __name__ == "__main__":
    main()
