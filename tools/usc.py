# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Michael Wulff Nielsen
"""Inspect, unpack and build uSVC game packages (.usc).

A package is a 512-byte header, a 96x72 preview image padded to whole
512-byte sectors, and the raw game binary that the loader writes to flash
at 0x6000.

Usage:
    uv run usc.py info GAME.usc
    uv run usc.py unpack GAME.usc OUT_DIR
    uv run usc.py pack BINARY.bin OUT.usc --title "My game" [--preview FILE]
"""

import argparse
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path

MAGIC = b"USVC"
HEADER_SIZE = 512
SECTOR = 512
PREVIEW_WIDTH = 96
PREVIEW_HEIGHT = 72
PREVIEW_BYTES = PREVIEW_WIDTH * PREVIEW_HEIGHT
PREVIEW_SIZE = -(-PREVIEW_BYTES // SECTOR) * SECTOR
FIELD = 32

SHORT_TITLE_OFFSET = 32
TITLE_OFFSET = 64
DESCRIPTION_OFFSET = 192
AUTHORS_OFFSET = 320
DATE_OFFSET = 384
VERSION_OFFSET = 416


@dataclass
class Package:
    """Contents of a .usc file."""

    binary: bytes
    short_title: str = ""
    title: list[str] = field(default_factory=list)
    description: list[str] = field(default_factory=list)
    authors: list[str] = field(default_factory=list)
    date: str = ""
    version: str = ""
    preview: bytes = bytes(PREVIEW_BYTES)


def checksum(binary: bytes) -> int:
    """Sum of the little-endian words of the binary, padded to 4 bytes."""
    padded = binary + bytes(-len(binary) % 4)
    words = struct.unpack(f"<{len(padded) // 4}I", padded)
    return sum(words) & 0xFFFFFFFF


def _get_text(data: bytes, offset: int) -> str:
    raw = data[offset : offset + FIELD]
    return raw.split(b"\0", 1)[0].decode("latin-1")


def _put_text(header: bytearray, offset: int, text: str) -> None:
    raw = text.encode("latin-1")[:FIELD]
    header[offset : offset + len(raw)] = raw


def parse(data: bytes) -> Package:
    """Parse a .usc file, checking signature, length and checksum."""
    if data[:4] != MAGIC:
        raise ValueError("missing USVC signature")
    stored_sum, length = struct.unpack_from("<II", data, 4)
    start = HEADER_SIZE + PREVIEW_SIZE
    binary = data[start : start + length]
    if len(binary) != length:
        raise ValueError("binary length in header exceeds the file")
    if checksum(binary) != stored_sum:
        raise ValueError("checksum mismatch")

    def lines(offset: int, count: int) -> list[str]:
        return [_get_text(data, offset + i * FIELD) for i in range(count)]

    return Package(
        binary=binary,
        short_title=_get_text(data, SHORT_TITLE_OFFSET),
        title=lines(TITLE_OFFSET, 4),
        description=lines(DESCRIPTION_OFFSET, 4),
        authors=lines(AUTHORS_OFFSET, 2),
        date=_get_text(data, DATE_OFFSET),
        version=_get_text(data, VERSION_OFFSET),
        preview=data[HEADER_SIZE : HEADER_SIZE + PREVIEW_BYTES],
    )


def build(pkg: Package) -> bytes:
    """Serialise a package to .usc bytes."""
    if len(pkg.preview) != PREVIEW_BYTES:
        raise ValueError(f"preview must be {PREVIEW_BYTES} bytes")
    binary = pkg.binary + bytes(-len(pkg.binary) % 4)
    header = bytearray(HEADER_SIZE)
    header[:4] = MAGIC
    struct.pack_into("<II", header, 4, checksum(binary), len(binary))
    _put_text(header, SHORT_TITLE_OFFSET, pkg.short_title)
    for base, texts, count in (
        (TITLE_OFFSET, pkg.title, 4),
        (DESCRIPTION_OFFSET, pkg.description, 4),
        (AUTHORS_OFFSET, pkg.authors, 2),
    ):
        for i, text in enumerate(texts[:count]):
            _put_text(header, base + i * FIELD, text)
    _put_text(header, DATE_OFFSET, pkg.date)
    _put_text(header, VERSION_OFFSET, pkg.version)
    preview = pkg.preview + bytes(PREVIEW_SIZE - PREVIEW_BYTES)
    return bytes(header) + preview + binary


def cmd_info(args: argparse.Namespace) -> None:
    pkg = parse(args.file.read_bytes())
    print(f"short title: {pkg.short_title}")
    for name, texts in (
        ("title", pkg.title),
        ("description", pkg.description),
        ("authors", pkg.authors),
    ):
        print(f"{name}: {' / '.join(t for t in texts if t)}")
    print(f"date: {pkg.date}")
    print(f"version: {pkg.version}")
    print(f"binary: {len(pkg.binary)} bytes, checksum {checksum(pkg.binary):#010x}")


def cmd_unpack(args: argparse.Namespace) -> None:
    pkg = parse(args.file.read_bytes())
    args.out_dir.mkdir(parents=True, exist_ok=True)
    (args.out_dir / "binary.bin").write_bytes(pkg.binary)
    (args.out_dir / "preview.raw").write_bytes(pkg.preview)
    print(f"wrote binary.bin and preview.raw to {args.out_dir}")


# The game loader prints these fields in a column 15 characters wide.
LOADER_LINE_WIDTH = 15


def warn_long_lines(pkg: Package) -> None:
    lines = [*pkg.title, *pkg.description, *pkg.authors]
    for text in lines:
        if len(text) > LOADER_LINE_WIDTH:
            print(
                f'warning: "{text}" is longer than {LOADER_LINE_WIDTH} characters '
                "and will be cut off in the game loader",
                file=sys.stderr,
            )


def cmd_pack(args: argparse.Namespace) -> None:
    preview = args.preview.read_bytes() if args.preview else bytes(PREVIEW_BYTES)
    pkg = Package(
        binary=args.binary.read_bytes(),
        short_title=args.short_title or args.title[0],
        title=args.title,
        description=args.description,
        authors=args.author,
        date=args.date,
        version=args.version,
        preview=preview,
    )
    warn_long_lines(pkg)
    args.out.write_bytes(build(pkg))
    print(f"wrote {args.out} ({len(pkg.binary)} byte binary)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    sub = parser.add_subparsers(dest="command", required=True)

    info = sub.add_parser("info", help="print the header of a package")
    info.add_argument("file", type=Path)
    info.set_defaults(func=cmd_info)

    unpack = sub.add_parser("unpack", help="extract the binary and preview")
    unpack.add_argument("file", type=Path)
    unpack.add_argument("out_dir", type=Path)
    unpack.set_defaults(func=cmd_unpack)

    pack = sub.add_parser("pack", help="build a package from a binary")
    pack.add_argument("binary", type=Path)
    pack.add_argument("out", type=Path)
    pack.add_argument("--title", action="append", required=True, help="up to 4")
    pack.add_argument("--short-title", help="defaults to the first title line")
    pack.add_argument("--description", action="append", default=[], help="up to 4")
    pack.add_argument("--author", action="append", default=[], help="up to 2")
    pack.add_argument("--date", default="")
    pack.add_argument("--version", default="")
    pack.add_argument("--preview", type=Path, help="96x72 raw 8-bit image")
    pack.set_defaults(func=cmd_pack)

    args = parser.parse_args()
    try:
        args.func(args)
    except (OSError, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
