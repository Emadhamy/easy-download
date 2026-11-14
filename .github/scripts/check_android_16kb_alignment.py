#!/usr/bin/env python3
"""Audit APK native-library ZIP and ELF alignment for Android 16 KB pages."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path


LOAD_LINE = re.compile(r"^\s*LOAD\s+.*?\s+(0x[0-9a-fA-F]+|2\*\*\d+)\s*$")


def run(command: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=False, text=True, capture_output=True)


def alignment_value(value: str) -> int:
    if value.startswith("2**"):
        return 1 << int(value[3:])
    return int(value, 16)


def audit_elf(path: Path, readelf: str, archive_path: str) -> tuple[list[str], bool]:
    # youtubedl-android intentionally packages Python ZIP data with a .so suffix.
    # It is not a loadable ELF and must not be passed to llvm-readelf.
    with path.open("rb") as source:
        if source.read(4) != b"\\x7fELF":
            return [], False
    result = run([readelf, "-lW", str(path)])
    if result.returncode:
        return [f"{archive_path}: llvm-readelf failed: {result.stderr.strip()}"], True
    loads = []
    for line in result.stdout.splitlines():
        match = LOAD_LINE.match(line)
        if match:
            loads.append(alignment_value(match.group(1)))
    if not loads:
        return [f"{archive_path}: no ELF LOAD segments found"], True
    failures = [
        f"{archive_path}: LOAD segment alignment 0x{value:x} is below 0x4000"
        for value in loads
        if value < 0x4000
    ]
    return failures, True


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", required=True, type=Path)
    parser.add_argument("--readelf", required=True)
    parser.add_argument("--zipalign", required=True)
    args = parser.parse_args()

    if not args.apk.is_file():
        print(f"APK not found: {args.apk}", file=sys.stderr)
        return 2

    zip_result = run([args.zipalign, "-c", "-P", "16", "-v", "4", str(args.apk)])
    if zip_result.returncode:
        print("APK ZIP alignment check failed:")
        print(zip_result.stdout)
        print(zip_result.stderr, file=sys.stderr)
        return 1

    failures: list[str] = []
    checked = 0
    skipped_non_elf = 0
    with tempfile.TemporaryDirectory(prefix="easy-download-16kb-") as temp:
        root = Path(temp)
        with zipfile.ZipFile(args.apk) as archive:
            native_entries = [
                entry for entry in archive.infolist()
                if not entry.is_dir() and entry.filename.startswith("lib/")
                and entry.filename.endswith(".so")
            ]
            if not native_entries:
                print("No native shared libraries were found in the APK.", file=sys.stderr)
                return 1
            for entry in native_entries:
                destination = root / entry.filename
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(archive.read(entry))
                elf_failures, is_elf = audit_elf(destination, args.readelf, entry.filename)
                if is_elf:
                    checked += 1
                    failures.extend(elf_failures)
                else:
                    skipped_non_elf += 1

    print(f"Checked {checked} ELF shared libraries in {args.apk}; skipped {skipped_non_elf} non-ELF .so payload(s).")
    if failures:
        print("16 KB ELF alignment audit failed:")
        for failure in failures:
            print(f"- {failure}")
        return 1

    print("APK ZIP alignment and all native ELF LOAD segments passed the 16 KB alignment audit.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
