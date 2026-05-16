#!/usr/bin/env python3
"""
Parse Maya benchmark output and compare it against Linux baselines.

Usage:
  python3 scripts/benchmark-compare.py < serial.log
  python3 scripts/benchmark-compare.py serial.log
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

LINUX_BASELINES = {
    "IPC round-trip": 800,
    "Cap create+validate+revoke": None,
    "IO mediation decision": None,
    "AI scheduler score": 2000,
    "PMM alloc+free": 200,
}

MAYA_TARGETS = {
    "IPC round-trip": 500,
    "Cap create+validate+revoke": 100,
    "IO mediation decision": 2000,
    "AI scheduler score": 5000,
    "PMM alloc+free": 300,
}

PATTERN = re.compile(r"^(.*?):\s+(\d+)\s+cycles\s*$")


def read_input() -> str:
    if len(sys.argv) > 1:
        return Path(sys.argv[1]).read_text()
    return sys.stdin.read()


def parse_cycles(text: str) -> dict[str, int]:
    results: dict[str, int] = {}
    for line in text.splitlines():
        match = PATTERN.match(line.strip())
        if match:
            results[match.group(1)] = int(match.group(2))
    return results


def format_baseline(value: int | None) -> str:
    return "N/A" if value is None else str(value)


def compare_line(name: str, maya_cycles: int) -> str:
    linux = LINUX_BASELINES.get(name)
    target = MAYA_TARGETS.get(name)

    if linux is None:
        linux_note = "no Linux equivalent"
    elif maya_cycles < linux:
        linux_note = "Maya beats Linux"
    elif maya_cycles > linux:
        linux_note = "Maya overhead vs Linux"
    else:
        linux_note = "matches Linux"

    if target is None:
        target_note = "no target"
    elif maya_cycles <= target:
        target_note = "meets Maya target"
    else:
        target_note = "above Maya target"

    return (
        f"{name:30} maya={maya_cycles:>6}  "
        f"linux={format_baseline(linux):>6}  "
        f"target={format_baseline(target):>6}  "
        f"{linux_note}; {target_note}"
    )


def main() -> int:
    text = read_input()
    results = parse_cycles(text)

    if not results:
        print("No benchmark cycle counts found in input.", file=sys.stderr)
        return 1

    print("Maya vs Linux benchmark comparison")
    print("=" * 72)
    for name in [
        "IPC round-trip",
        "Cap create+validate+revoke",
        "IO mediation decision",
        "AI scheduler score",
        "PMM alloc+free",
    ]:
        if name in results:
            print(compare_line(name, results[name]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
