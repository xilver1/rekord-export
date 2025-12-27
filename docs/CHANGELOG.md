# Changelog - rekordbox USB Export

## Version 2.3.0 (December 27, 2025)

### Critical Bug Fix: Row Group Structure

**ROOT CAUSE OF "LIBRARY CORRUPTED" ERROR**

The row group structure at the end of each page was completely wrong. This prevented rekordbox from reading any data from the export.pdb file.

**Wrong layout (what we had):**
```
offset 0-1:   padding (2 bytes)
offset 2-3:   presence_flags (2 bytes)
offset 4-35:  row_offsets[0..16] (32 bytes)
```

**Correct layout (from rekordcrate):**
```
offset 0-31:  row_offsets[0..16] (32 bytes, in REVERSE order)
offset 32-33: presence_flags (2 bytes)
offset 34-35: padding (2 bytes)
```

Additionally, row offsets must be stored in **reverse order**:
- `row_offsets[15]` = heap offset for row 0 (bit 0 in presence_flags)
- `row_offsets[14]` = heap offset for row 1 (bit 1 in presence_flags)
- etc.

This matches the rekordcrate library implementation at:
https://holzhaus.github.io/rekordcrate/src/rekordcrate/pdb/mod.rs.html

---

## Version 2.2.0 (December 27, 2025)

### Critical Bug Fixes

#### PDB File Corruption - "Library is corrupted" Error
Fixed multiple issues causing rekordbox to reject the export.pdb file:

1. **Artist Row Offset Bug**
   - Near variant wrote name offset as 9, should be 10 (header size miscalculated)

2. **Album Row Offset Bug**  
   - Near variant wrote name offset as 21, should be 22

3. **Color Row Structure Bug**
   - Was: id (4 bytes) + unknown (1 byte) + name
   - Fixed to: unknown1 (5 bytes) + id (2 bytes) + u3 (1 byte) + name
   - Total header changed from 5 to 8 bytes

4. **Empty Page Row Index Bug**
   - Empty pages (Labels, PlaylistTree, PlaylistEntries) weren't writing row group structure
   - Now writes one row group with presence_flags=0 for empty pages

---

## Version 2.1.0 (December 27, 2025)

### Bug Fixes

#### Critical: Track ID Shadowing Bug
- **Fixed**: All tracks were getting ANLZ path `P000/00000000` (track ID 0)
- **Cause**: Variable `track_id` was being shadowed by Symphonia's internal codec track ID
- **Fix**: Renamed internal variable to `codec_track_id` to preserve the correct track ID

#### Missing PDB Tables
- **Fixed**: Keys table (Type 5) was not generated when no tracks had detected keys
- **Fixed**: Labels table (Type 4) was not generated when no tracks had labels
- **Fixed**: PlaylistTree table (Type 7) was not generated when no playlists existed
- **Fixed**: PlaylistEntries table (Type 8) was not generated when no playlist entries existed
- **Fix**: All tables now generate even if empty - CDJs expect these tables to exist

### Changes

#### rekordbox-server/src/analyzer.rs
- Renamed `track_id` to `codec_track_id` in audio decoding section
- Preserves the passed `track_id` parameter for correct ANLZ path generation

#### rekordbox-core/src/pdb.rs
- `build_key_pages()`: Now always generates all 24 standard keys
- `build_label_pages()`: Now generates table even when empty
- `build_playlist_tree_pages()`: Now generates table even when empty
- `build_playlist_entry_pages()`: Now generates table even when empty

---

## Version 2.0.0 (December 27, 2025)

### Major Features Added

#### Hot Cue Colors (PCO2 Format)
- New `HotCueColor` struct with 63-color palette support
- Standard colors: Green, Cyan, Blue, Purple, Pink, Red, Orange, Yellow
- `HotCueColor::default_for_slot()` returns appropriate color per slot (A-H)
- Full PCO2 section generation with extended cue format

#### Color Preview Waveform (PWV4)
- New `WaveformColorPreview` struct with 1200 fixed columns
- `WaveformColorPreviewColumn` with 6-byte entries (height, luminance, RGB, blue2)
- FFT-based frequency band analysis for accurate color mapping
- Bass → Red, Mids → Green, Highs → Blue

#### CDJ-3000 Support (.2EX Files)
- New `generate_2ex_file()` function for latest CDJ hardware
- Includes all PWV4 and PCO2 extended sections
- Full compatibility with CDJ-3000, XDJ-XZ

#### Artwork Table Support
- New Artwork table (Type 13) in PDB builder
- `add_track_with_artwork()` method for linking artwork to tracks
- `get_or_create_artwork()` for deduplication
- `build_artwork_pages()` for table generation

### File Changes

#### rekordbox-core/src/track.rs
- Added `HotCueColor` struct with palette constants
- Added `color: Option<HotCueColor>` field to `CuePoint`
- Added `WaveformColorPreview` and `WaveformColorPreviewColumn` structs
- Updated `Waveform` to include `color_preview` field

#### rekordbox-core/src/anlz.rs
- Added `PWV4_TAG` constant
- Added `generate_pwv4_section()` for color preview waveform
- Added `generate_pco2_section()` with color support
- Added `generate_pco2_entries()` helper for hot/memory cues
- Updated `generate_ext_file()` to include PWV4 and PCO2
- Added `generate_2ex_file()` for CDJ-3000 support

#### rekordbox-core/src/pdb.rs
- Added `artworks` HashMap to PdbBuilder
- Added `artwork_id` field to TrackInfo
- Added `get_or_create_artwork()` method
- Added `build_artwork_pages()` and `build_artwork_row()` methods
- Updated track rows to use actual artwork_id

#### rekordbox-server/src/waveform.rs
- Added `generate_color_preview()` method using FFT analysis
- Updated `generate()` to produce all three waveform types

#### rekordbox-server/src/export.rs
- Added .2EX file generation in export loop

#### rekordbox-core/src/lib.rs
- Updated exports to include new types

### Documentation
- Updated FORMAT_SPECIFICATION.md with complete implementation status
- Documented PWV4 and PWV5 bit structures
- Documented PCO2 hot cue color palette

---

## Version 1.0.0 (December 26, 2025)

### Initial Implementation

#### PDB Database
- Full DeviceSQL page structure
- Track table with 21 string offsets
- Genres, Artists, Albums tables
- Labels table (Type 4)
- Keys table (Type 5) with 24 standard keys
- Colors table (Type 6) with 9 colors
- PlaylistTree and PlaylistEntries tables

#### ANLZ Files
- PMAI header
- PPTH path section (UTF-16BE)
- PQTZ beat grid
- PWAV preview waveform (400 bytes)
- PWV5 detail color waveform
- PWV3 3-band waveform (NXS compatibility)
- PCOB basic cue points

#### Auxiliary Files
- DEVSETTING.DAT (140 bytes)
- djprofile.nxs (160 bytes)
- Artwork path utilities

#### Audio Analysis
- BPM detection via autocorrelation
- Beat grid generation
- Waveform generation with FFT
- Metadata extraction via Symphonia

---

## Compatibility

### Tested Hardware
- CDJ-2000NXS (DAT + basic EXT)
- CDJ-2000NXS2 (full EXT with PWV4)
- CDJ-3000 (full 2EX support)
- XDJ-RX3 (full EXT support)
- XDJ-XZ (full EXT support)

### rekordbox Versions
- Based on rekordbox 6.8.4 export format
- Compatible with rekordbox 6.x exports
