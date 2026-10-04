"""Convert songs written as text into the uSVC kernel's music format.

Usage:
    uv run song.py OUT SONG.song [SONG.song ...]

Writes OUT.c and OUT.h with one byte array per song, named after the file
(`title.song` becomes `titleSong`). Play one with the kernel's `startSong`.

A song file is a step sequencer in text form:

    tempo 6          # video frames per step (57 frames per second)
    patch 0 8        # channel 0 plays patches[8]
    volume 2 60      # channel volume, 0-127
    loop             # everything below repeats forever (optional)

    0: a4 -  c5 -  e5 .  .  .
    1: a2 -  -  -  a2 -  -  -

Rows that follow each other without a blank line play together and must have
the same number of steps. A step is a note (`c4` is middle C, `f#3`, `bb5`),
`-` to let the previous note ring on, or `.` to stop it. Channels are 0-2;
the kernel keeps channel 3 for sound effects. `#` at the start of a line or
after a space starts a comment.
"""

import argparse
import re
import sys
from pathlib import Path

NOTE_VELOCITY = 0x50
CONTROLLER_VOLUME = 7
MIDDLE_C = 60
SEMITONES = {"c": 0, "d": 2, "e": 4, "f": 5, "g": 7, "a": 9, "b": 11}
NOTE_RE = re.compile(r"^([a-g])([#b]?)(-?\d)$")
MAX_CHANNEL = 2
BYTES_PER_LINE = 16

LOOP_START = bytes([0xFF, 0x06, 0x01, ord("S")])
LOOP_END = bytes([0xFF, 0x06, 0x01, ord("E")])
END_OF_SONG = bytes([0xFF, 0x2F])


def note_number(token: str) -> int:
    """MIDI note number of a name such as c4, f#3 or bb5."""
    match = NOTE_RE.match(token.lower())
    if not match:
        raise ValueError(f'bad note "{token}"')
    letter, accidental, octave = match.groups()
    shift = {"#": 1, "b": -1, "": 0}[accidental]
    return MIDDLE_C + SEMITONES[letter] + shift + 12 * (int(octave) - 4)


def var_len(value: int) -> bytes:
    """A delay in the MIDI variable-length encoding the kernel reads."""
    out = [value & 0x7F]
    value >>= 7
    while value:
        out.insert(0, 0x80 | (value & 0x7F))
        value >>= 7
    return bytes(out)


def strip_comment(line: str) -> str:
    return re.split(r"(^|\s)#(\s|$)", line, maxsplit=1)[0].strip()


def parse(text: str) -> tuple[list[tuple[int, bytes]], int]:
    """Return (events as (frame, bytes), total length in frames)."""
    events: list[tuple[int, bytes]] = []
    tempo = 6
    frame = 0
    block: dict[int, list[str]] = {}

    def flush_block() -> None:
        nonlocal frame
        if not block:
            return
        lengths = {len(steps) for steps in block.values()}
        if len(lengths) != 1:
            raise ValueError(f"rows of one block differ in length: {sorted(lengths)}")
        for step in range(lengths.pop()):
            for channel in sorted(block):
                token = block[channel][step]
                if token == "-":
                    continue
                if token == ".":
                    # A note-on with volume 0 stops the note; the pitch is unused.
                    events.append((frame, bytes([0x90 | channel, MIDDLE_C, 0])))
                else:
                    note = note_number(token)
                    events.append((frame, bytes([0x90 | channel, note, NOTE_VELOCITY])))
            frame += tempo
        block.clear()

    for number, raw in enumerate(text.splitlines(), 1):
        line = strip_comment(raw)
        try:
            row = re.match(r"^(\d):\s*(.*)$", line)
            if row:
                channel = int(row.group(1))
                if channel > MAX_CHANNEL:
                    raise ValueError(f"channel {channel} is reserved for effects")
                block.setdefault(channel, []).extend(row.group(2).split())
                continue
            flush_block()
            words = line.split()
            if not words:
                continue
            if words[0] == "tempo":
                tempo = int(words[1])
            elif words[0] == "patch":
                events.append((frame, bytes([0xC0 | int(words[1]), int(words[2])])))
            elif words[0] == "volume":
                channel, volume = int(words[1]), int(words[2])
                events.append(
                    (frame, bytes([0xB0 | channel, CONTROLLER_VOLUME, volume]))
                )
            elif words[0] == "loop":
                events.append((frame, LOOP_START))
            else:
                raise ValueError(f'unknown directive "{words[0]}"')
        except (ValueError, IndexError) as err:
            raise ValueError(f"line {number}: {err}") from err
    flush_block()
    return events, frame


def encode(text: str) -> bytes:
    """The byte stream for a song: (delay, event) pairs and an end marker."""
    events, length = parse(text)
    loops = any(data == LOOP_START for _, data in events)
    events.append((length, LOOP_END if loops else END_OF_SONG))
    out = bytearray()
    now = 0
    for frame, data in events:
        out += var_len(frame - now) + data
        now = frame
    if loops:
        out += var_len(0) + END_OF_SONG
    return bytes(out) + b"\x00"


def format_bytes(data: bytes) -> str:
    rows = []
    for i in range(0, len(data), BYTES_PER_LINE):
        rows.append(
            "\t" + ", ".join(f"0x{b:02X}" for b in data[i : i + BYTES_PER_LINE])
        )
    return ",\n".join(rows)


def convert(out: Path, songs: list[Path]) -> None:
    header = out.with_suffix(".h")
    guard = "SONG_" + "".join(c if c.isalnum() else "_" for c in header.name).upper()
    declarations = []
    definitions = []
    for path in songs:
        try:
            data = encode(path.read_text())
        except ValueError as err:
            raise ValueError(f"{path.name}: {err}") from err
        name = f"{path.stem}Song"
        declarations.append(f"extern const uint8_t {name}[{len(data)}];")
        definitions.append(
            f"/* {path.name} */\nconst uint8_t {name}[{len(data)}] =\n{{\n"
            f"{format_bytes(data)}\n}};\n"
        )
        print(f"{path.name}: {len(data)} bytes")
    note = "/* Generated by tools/song.py. Do not edit. */\n"
    header.write_text(
        f"{note}#ifndef {guard}\n#define {guard}\n#include <stdint.h>\n\n"
        + "\n".join(declarations)
        + "\n\n#endif\n"
    )
    out.with_suffix(".c").write_text(
        f'{note}#include "{header.name}"\n\n' + "\n".join(definitions)
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("out", type=Path, help="output path without extension")
    parser.add_argument("songs", type=Path, nargs="+", help=".song files")
    args = parser.parse_args()
    try:
        convert(args.out, args.songs)
    except (OSError, ValueError) as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
