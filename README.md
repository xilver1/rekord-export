# Rekordbox USB Export

A Rust-based system for analyzing music and generating Pioneer CDJ-compatible USB exports.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Server                             │
│                                                                 │
│  ┌──────────────────┐    ┌─────────────────────────────────┐   │
│  │ rekordbox-server │◄───│ Music Files                     │   │
│  │                  │    │ /mnt/ssd/pre-export/            │   │
│  │ • Audio analysis │    │   ├── Playlist1/                │   │
│  │ • BPM detection  │    │   │   ├── track1.mp3           │   │
│  │ • Waveform gen   │    │   │   └── track2.flac          │   │
│  │ • PDB/ANLZ write │    │   └── Playlist2/                │   │
│  └────────┬─────────┘    │       └── track3.wav           │   │
│           │              └─────────────────────────────────┘   │
│           │                                                     │
│           │ Unix Socket                                         │
│           │ /tmp/rekordbox.sock                                │
│           │                                                     │
│           │              ┌─────────────────────────────────┐   │
│           │              │ Analysis Cache                  │   │
│           └─────────────►│ /mnt/ssd/rekordbox-cache/       │   │
│                          │ (file-based, on SSD)            │   │
│                          └─────────────────────────────────┘   │
│                                                                 │
└───────────────────────────────┬─────────────────────────────────┘
                                │ Wireguard VPN
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│ Android Phone (Termux)                                          │
│                                                                 │
│  ┌─────────────┐                                                │
│  │ rbx CLI     │  $ rbx analyze                                 │
│  │ (~500KB)    │  $ rbx export /mnt/usb                         │
│  └─────────────┘  $ rbx list                                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Hardware Requirements

- **Server**: Dell Wyse 5070 (or similar low-power x86)
  - 8GB RAM minimum
  - External SSD for music and cache (eMMC too small)
  - Running OpenMediaVault or similar

- **Client**: Any device with Termux or Rust support
  - ~500KB binary size for CLI

## Building

### On the NAS (server)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/yourusername/rekordbox-export
cd rekordbox-export

# Build release (optimized for size)
cargo build --release

# Server binary: target/release/rekordbox-server (~5MB)
# CLI binary: target/release/rbx (~500KB)
```

### Cross-compile CLI for Android (aarch64)

```bash
# Add Android target
rustup target add aarch64-linux-android

# Install Android NDK and set up linker (see rustup docs)
# Then build:
cargo build --release --target aarch64-linux-android -p rekordbox-cli
```

## Installation

### Server (systemd service)

```bash
# Copy binary
sudo cp target/release/rekordbox-server /usr/local/bin/

# Create service file
sudo cp rekordbox.service /etc/systemd/system/

# Enable and start
sudo systemctl enable rekordbox
sudo systemctl start rekordbox
```

### CLI (Termux)

```bash
# Copy to Termux
scp target/aarch64-linux-android/release/rbx user@phone:~/bin/

# Or build locally in Termux:
pkg install rust
cargo build --release -p rekordbox-cli
cp target/release/rbx ~/bin/
```

## Usage

### Start the server
```bash
rekordbox-server \
  --music-dir /mnt/ssd/pre-export \
  --cache-dir /mnt/ssd/rekordbox-cache \
  --socket /tmp/rekordbox.sock
```

### CLI commands

```bash
# Check server status
rbx status

# Analyze all tracks
rbx analyze

# List analyzed tracks
rbx list

# Export to USB
rbx export /mnt/usb

# View cache stats
rbx cache

# Clear cache
rbx cache --clear
```

### One-shot export (without server)

```bash
rekordbox-server \
  --music-dir /mnt/ssd/pre-export \
  --cache-dir /mnt/ssd/rekordbox-cache \
  --output-dir /mnt/usb \
  --analyze-only --export
```

## Directory Structure

### Input (pre-export folder)
```
/mnt/ssd/pre-export/
├── House/
│   ├── track1.mp3
│   └── track2.flac
├── Techno/
│   └── track3.wav
└── Warm-up/
    └── track4.aiff
```

Each subfolder becomes a playlist.

### Output (USB export)
```
USB_ROOT/
├── PIONEER/
│   ├── rekordbox/
│   │   └── export.pdb
│   └── USBANLZ/
│       └── P000/
│           ├── 00000001/
│           │   ├── ANLZ0000.DAT
│           │   └── ANLZ0000.EXT
│           └── 00000002/
│               └── ...
└── Contents/
    ├── track1.mp3
    ├── track2.flac
    └── ...
```

## USB Preparation

**CRITICAL**: USB must be FAT32 with MBR partition table!

```bash
# Check current format
lsblk -f /dev/sdX

# Reformat if needed (WARNING: destroys data!)
sudo wipefs -a /dev/sdX
sudo parted /dev/sdX mklabel msdos
sudo parted /dev/sdX mkpart primary fat32 1MiB 100%
sudo mkfs.vfat -F 32 /dev/sdX1
```

GPT partition tables will cause CDJ rejection even with FAT32!

## CDJ Compatibility

| Player | .DAT | .EXT | .2EX | Notes |
|--------|------|------|------|-------|
| CDJ-2000 | ✓ | ✗ | ✗ | Monochrome waveforms only |
| CDJ-2000NXS | ✓ | ✓ | ✗ | Color waveforms |
| CDJ-2000NXS2 | ✓ | ✓ | ✗ | Color waveforms |
| CDJ-3000 | ✓ | ✓ | ✓ | Full color, exFAT support |
| XDJ-XZ | ✓ | ✓ | ✗ | Color waveforms |
| XDJ-RX3 | ✓ | ✓ | ✗ | Color waveforms |

## Memory Optimization

The server is designed for memory-constrained environments:

- Single-threaded async runtime (no thread pool overhead)
- Streaming audio decoding (max ~50MB sample buffer)
- File-based cache (no in-memory analysis storage)
- Chunk-based FFT for waveform generation

Typical memory usage: 200-500MB during analysis, <50MB idle.

## Known Limitations

1. **BPM Detection**: Uses simple autocorrelation (~85% accuracy).
   For DJ-grade accuracy, consider integrating stratum-dsp.

2. **Key Detection**: Not yet implemented.
   Placeholder returns `None` for all tracks.

3. **PDB Format**: Simplified implementation.
   Full rekordcrate-style page management needed for large libraries.

4. **Hot Cues/Memory Points**: Not supported yet.
   Only beat grids and waveforms are exported.

## Development

### Running tests
```bash
cargo test --workspace
```

### Format reference
- [Deep Symmetry Analysis](https://djl-analysis.deepsymmetry.org/djl-analysis/)
- [rekordcrate source](https://github.com/Holzhaus/rekordcrate)
- [REX Go implementation](https://github.com/kimtore/rex)

## License

MIT
