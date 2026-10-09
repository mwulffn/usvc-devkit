# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Michael Wulff Nielsen
"""Tests for gfx.py."""

from pathlib import Path

from PIL import Image

import gfx


def test_colour_byte_matches_kernel_macro() -> None:
    # COLOR_TORGB332(7, 0, 0), (0, 7, 0), (0, 0, 3) from vgaConstants.h.
    assert gfx.colour_byte(255, 0, 0) == 0x07
    assert gfx.colour_byte(0, 255, 0) == 0xD0
    assert gfx.colour_byte(0, 0, 255) == 0x28
    assert gfx.colour_byte(255, 255, 255) == 0xFF
    assert gfx.colour_byte(0, 0, 0) == 0x00


def test_tiles_are_cut_in_reading_order(tmp_path: Path) -> None:
    sheet = Image.new("RGBA", (16, 8), (0, 0, 0, 255))
    sheet.paste((255, 0, 0, 255), (8, 0, 16, 8))
    sheet.save(tmp_path / "t.png")
    (tmp_path / "t.txt").write_text("empty\nred  # second tile\n")
    gfx.convert_tiles(tmp_path / "t.png", tmp_path / "t.txt", tmp_path / "out")
    header = (tmp_path / "out.h").read_text()
    assert "#define NUM_ROM_TILES 2" in header
    assert "#define TILE_RED 1" in header
    source = (tmp_path / "out.c").read_text()
    assert source.count("0x07") == 64


def test_sprite_transparency_and_black(tmp_path: Path) -> None:
    sheet = Image.new("RGBA", (2, 1), (0, 0, 0, 0))
    sheet.putpixel((1, 0), (0, 0, 0, 255))
    sheet.save(tmp_path / "s.png")
    (tmp_path / "s.txt").write_text("dot 0 0 2 1\n")
    gfx.convert_sprites(tmp_path / "s.png", tmp_path / "s.txt", tmp_path / "out")
    source = (tmp_path / "out.c").read_text()
    assert "0x00, 0x01" in source
    assert ".w = 2, .h = 1, .ox = 1, .oy = 0" in source


def test_preview_is_scaled_to_loader_size(tmp_path: Path) -> None:
    Image.new("RGBA", (640, 400), (255, 0, 0, 255)).save(tmp_path / "shot.png")
    gfx.convert_preview(tmp_path / "shot.png", tmp_path / "p.raw")
    assert (tmp_path / "p.raw").read_bytes() == bytes([0x07]) * (96 * 72)


def test_preview_is_stored_as_tiles(tmp_path: Path) -> None:
    # Left half red, right half white: tiles 0-5 of each row of 12 are red.
    image = Image.new("RGBA", (96, 72), (255, 0, 0, 255))
    image.paste((255, 255, 255, 255), (48, 0, 96, 72))
    image.save(tmp_path / "shot.png")
    gfx.convert_preview(tmp_path / "shot.png", tmp_path / "p.raw")
    data = (tmp_path / "p.raw").read_bytes()
    assert data[: 6 * 64] == bytes([0x07]) * (6 * 64)
    assert data[6 * 64 : 12 * 64] == bytes([0xFF]) * (6 * 64)
