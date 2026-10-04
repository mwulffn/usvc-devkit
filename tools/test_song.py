"""Tests for song.py."""

import pytest

import song


def test_note_numbers() -> None:
    assert song.note_number("c4") == 60
    assert song.note_number("a4") == 69
    assert song.note_number("f#3") == 54
    assert song.note_number("bb5") == 82


def test_var_len_matches_midi() -> None:
    assert song.var_len(0) == b"\x00"
    assert song.var_len(127) == b"\x7f"
    assert song.var_len(128) == b"\x81\x00"
    assert song.var_len(300) == b"\x82\x2c"


def test_one_shot_song() -> None:
    data = song.encode("tempo 4\npatch 0 8\n0: c4 - .\n")
    assert data == bytes(
        [0, 0xC0, 8]  # patch at time 0
        + [0, 0x90, 60, 0x50]  # note on
        + [8, 0x90, 60, 0]  # note off two steps later
        + [4, 0xFF, 0x2F, 0]  # end after the last step
    )


def test_looping_song_marks_loop_and_keeps_length() -> None:
    data = song.encode("tempo 5\nloop\n0: c4 -\n1: c3 -\n")
    assert data == bytes(
        [0, 0xFF, 0x06, 0x01, ord("S")]
        + [0, 0x90, 60, 0x50]
        + [0, 0x91, 48, 0x50]
        + [10, 0xFF, 0x06, 0x01, ord("E")]
        + [0, 0xFF, 0x2F, 0]
    )


def test_rows_of_a_block_must_match() -> None:
    with pytest.raises(ValueError, match="differ in length"):
        song.encode("0: c4 -\n1: c3\n")


def test_channel_three_is_refused() -> None:
    with pytest.raises(ValueError, match="reserved"):
        song.encode("3: c4\n")
