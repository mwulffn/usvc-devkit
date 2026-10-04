# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Michael Wulff Nielsen
"""Round-trip tests for usc.py against the shipped packages."""

from pathlib import Path

import pytest

import usc

PACKAGES = Path(__file__).parent.parent / "reference" / "uSVC" / "usc packages"


@pytest.mark.parametrize("path", sorted(PACKAGES.glob("*.usc")), ids=lambda p: p.stem)
def test_rebuilds_shipped_package_byte_for_byte(path: Path) -> None:
    data = path.read_bytes()
    assert usc.build(usc.parse(data)) == data


def test_checksum_pads_to_word() -> None:
    assert usc.checksum(b"\x01\x00\x00\x00\x02") == 3


def test_parse_rejects_bad_signature() -> None:
    with pytest.raises(ValueError, match="signature"):
        usc.parse(b"NOPE" + bytes(8000))
