#!/usr/bin/env python3
"""
Generate Linux deployment artifacts from the protocol device registry.

Outputs:
- crates/monsgeek-transport/deploy/99-monsgeek.rules
- crates/monsgeek-transport/deploy/monsgeek-hid-usbhid.conf
"""

from __future__ import annotations

import argparse
import glob
import json
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEVICES_GLOB = REPO_ROOT / "crates" / "monsgeek-protocol" / "devices" / "*.json"
RULES_PATH = REPO_ROOT / "crates" / "monsgeek-transport" / "deploy" / "99-monsgeek.rules"
QUIRKS_PATH = (
    REPO_ROOT
    / "crates"
    / "monsgeek-transport"
    / "deploy"
    / "monsgeek-hid-usbhid.conf"
)


def load_pairs() -> list[tuple[int, int]]:
    pairs = set()
    for path in glob.glob(str(DEVICES_GLOB)):
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
        pairs.add((int(data["vid"]), int(data["pid"])))
    return sorted(pairs)


def render_rules(vids: list[int]) -> str:
    lines = [
        "# MonsGeek/Akko keyboard udev rules for Linux",
        "#",
        "# Generated from crates/monsgeek-protocol/devices/*.json",
        "#",
        '# MODE="0666" is fallback for systems without seat ACLs.',
        '# TAG+="uaccess" grants per-user access on desktop Linux.',
        "",
    ]
    for vid in vids:
        lines.append(
            f'SUBSYSTEM=="usb", ATTRS{{idVendor}}=="{vid:04x}", MODE="0666", TAG+="uaccess"'
        )
    lines.append("")
    return "\n".join(lines)


def render_quirks(pairs: list[tuple[int, int]]) -> str:
    entries = [f"0x{vid:04x}:0x{pid:04x}:0x0010" for vid, pid in pairs]
    payload = ",\\\n".join(entries)
    return (
        "# Prevent usbhid from binding to supported MonsGeek/Akko keyboard interfaces.\n"
        "#\n"
        "# Generated from crates/monsgeek-protocol/devices/*.json\n"
        "#\n"
        "options usbhid quirks=\\\n"
        f"{payload}\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="Do not write files; exit non-zero if outputs differ",
    )
    args = parser.parse_args()

    pairs = load_pairs()
    vids = sorted({vid for vid, _ in pairs})

    rules = render_rules(vids)
    quirks = render_quirks(pairs)

    if args.check:
        ok = True
        if RULES_PATH.read_text(encoding="utf-8") != rules:
            ok = False
            print(f"out-of-date: {RULES_PATH}")
        if QUIRKS_PATH.read_text(encoding="utf-8") != quirks:
            ok = False
            print(f"out-of-date: {QUIRKS_PATH}")
        return 0 if ok else 1

    RULES_PATH.write_text(rules, encoding="utf-8")
    QUIRKS_PATH.write_text(quirks, encoding="utf-8")
    print(f"updated {RULES_PATH}")
    print(f"updated {QUIRKS_PATH}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
