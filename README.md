# rekord-export

Rust-based Pioneer CDJ USB export generator. Creates USB drives compatible with CDJ-2000 and newer players without requiring Rekordbox software.

## Architecture

```
┌─────────────────┐     Unix Socket      ┌─────────────────┐
│  rekordbox-cli  │ ◄──────────────────► │ rekordbox-server│
│   (Termux)      │                      │   (NAS/x86)     │
│   ~400KB        │                      │                 │
└─────────────────┘                      └────────┬────────┘
                                                  │
                                         ┌────────▼────────┐
                                         │  rekordbox-core │
                                         │ (PDB/ANLZ gen)  │
                                         └─────────────────┘
```

- **rekordbox-core**: Binary format library for PDB (DeviceSQL) and ANLZ files
- **rekordbox-server**: Audio analysis + export generation daemon for NAS
- **rekordbox-cli**: Lightweight client for Termux on Android

## USB Structure Generated

```
USB_ROOT/
├── PIONEER/
│   ├── rekordbox/
│   │   └── export.pdb          # Track database (DeviceSQL format)
│   └── USBANLZ/
│       └── P000/
│           └── 00000001/
│               ├── ANLZ0000.DAT  # Beat grid, waveforms
│               └── ANLZ0000.EXT  # Extended analysis
└── Contents/
    └── *.mp3, *.flac, etc.     # Audio files
```

## Building

```bash
# Build all crates
cargo build --release

# Build only the CLI (for Termux)
cargo build --release -p rekordbox-cli

# Cross-compile CLI for Android/aarch64
cargo build --release -p rekordbox-cli --target aarch64-linux-android
```

## Usage

### Direct Export (no server)

```bash
# Export music folder directly to USB
rekordbox-server --music-dir /path/to/music --export /media/usb
```

### Server Mode

On the NAS:
```bash
# Start server
rekordbox-server --music-dir /mnt/ssd/pre-export --socket /tmp/rekordbox.sock
```

From Termux (or any client):
```bash
# Check server status
rekordbox status

# Analyze tracks
rekordbox analyze

# Export to USB
rekordbox export /storage/usb

# List analyzed tracks
rekordbox list

# Cache management
rekordbox cache-stats
rekordbox cache-clear
```

## PDB Format Implementation

The export.pdb file uses Pioneer's DeviceSQL format:
- 4096-byte pages
- Little-endian byte order
- Tables: tracks, artists, albums, genres, keys, playlists
- Row index grows backward from page end
- Heap grows forward from offset 0x28
- DeviceSQL strings: short ASCII (flag|1), long ASCII (0x40), UTF-16LE (0x90)

### Track Row Structure (94+ bytes)
- Subtype, sample_rate, file_size, artwork_id, key_id
- artist_id, album_id, genre_id, tempo (BPM × 100)
- 21 string offsets pointing to: title, artist, file_path, analyze_path, etc.

## ANLZ Format Implementation

Analysis files (.DAT, .EXT) are **big-endian** and contain tagged sections:
- **PPTH**: File path (UTF-16BE encoded)
- **PQTZ**: Beat grid (beat number, tempo×100, time_ms)
- **PWAV**: Preview waveform (400 bytes, 5-bit height + 3-bit whiteness)
- **PWV5**: Detail waveform (150 entries/sec, RGB + height)

## Testing Without CDJ Hardware

1. **Mixxx DJ** (v2.3+): Import USB as Rekordbox library
2. **rekordcrate CLI**: `rekordcrate dump-pdb export.pdb`
3. **Kaitai Web IDE**: Visual binary inspection at ide.kaitai.io

## Key Differences From Original Broken Implementation

| Issue | Before | After |
|-------|--------|-------|
| edition | "2024" (invalid) | "2021" |
| Track pages | `vec![0u8; PAGE_SIZE]` (empty) | Actual row data with strings |
| Row index | Missing | Backward-growing from page end |
| String encoding | None | DeviceSQL format (short/long/UTF-16) |
| Page headers | Incomplete | Full header with row counts, free_size |
| ANLZ byte order | Little-endian | Big-endian (correct) |

## References

- [Deep Symmetry Analysis](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html)
- [rekordcrate](https://github.com/Holzhaus/rekordcrate) - Rust PDB/ANLZ library
- [REX](https://github.com/kimtore/rex) - Go implementation
- [crate-digger](https://github.com/Deep-Symmetry/crate-digger) - Java + documentation

## License

MIT
