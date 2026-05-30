# Disc: the bin/cue CD image

Status: verified against the real image (a 282 MB mixed-mode CD).

The game shipped on a CD, not as loose files. The image is a single `.bin`
(raw 2352-byte sectors) plus a `.cue` sheet. `prototype-disc` reads both the
game files (ISO9660 data track) and the soundtrack (CD-DA tracks) straight off
it, so nothing copyrighted is bundled in the repo.

## Layout

Mixed-mode CD, one data track followed by seven audio tracks:

- `TRACK 01 MODE1/2352` — the ISO9660 filesystem.
- `TRACK 02..08 AUDIO` — the red-book soundtrack.

The cue names the bin file in uppercase (`PROTOTYPE.BIN`) while the file on disk
is lowercase; the reader resolves it case-insensitively.

## Sectors

Every sector is **2352 bytes**.

- **MODE1/2352**: 12-byte sync + 4-byte header + **2048** user bytes + EDC/ECC.
  The user payload of logical block `lba` is at file byte `lba * 2352 + 16`.
- **AUDIO**: the whole 2352-byte sector is sample data — 588 frames of 16-bit
  stereo little-endian PCM at 44100 Hz. No header to skip.

## ISO9660

Level 1: uppercase 8.3 names with a `;1` version suffix, volume `PROTOTYPE`.
Read the Primary Volume Descriptor at logical sector 16 (`CD001` at offset 1;
root directory record at offset 156). Directory records:

| offset | field |
|-------:|-------|
| 0  | record length (0 ⇒ pad to next sector) |
| 2  | extent LBA (4-byte LE, then BE copy) |
| 10 | data length (4-byte LE, then BE copy) |
| 25 | file flags (bit 1 = directory) |
| 32 | file-identifier length |
| 33 | file identifier |

The root holds 75 files plus one `FLI/` subdirectory (12 files, one level deep)
for 87 files total. The root directory record's own identifier is a single
`0x00` byte; inside a directory, `.` and `..` are single `0x00` / `0x01`
identifier bytes, all skipped.

## CD-DA tracks

In-file LBA from the cue is frame-accurate with no lead-in adjustment:
`lba = (m*60 + s)*75 + f`. Verified `INDEX 01` starts:

| track | INDEX 01 LBA |
|------:|-------------:|
| 2 | 17709 |
| 3 | 33796 |
| 4 | 49164 |
| 5 | 66466 |
| 6 | 84189 |
| 7 | 101019 |
| 8 | 106804 |

Disc end (last sector + 1): **120104**. Tracks 3-8 carry an `INDEX 00` pregap
before `INDEX 01`. The reader treats a track as `[INDEX 01, next track's first
index)`, so the ~2 s pregaps are dropped from playback; the last track runs to
the disc end.
