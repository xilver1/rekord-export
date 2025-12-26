//! Track analysis data structures
//!
//! These are the high-level representations that get serialized to Pioneer formats.

use serde::{Deserialize, Serialize};

/// Complete analysis results for a single track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAnalysis {
    /// Unique track ID (generated, starts at 1)
    pub id: u32,
    /// Original file path relative to USB root
    pub file_path: String,
    /// Track title from metadata
    pub title: String,
    /// Artist name
    pub artist: String,
    /// Album name
    pub album: Option<String>,
    /// Genre
    pub genre: Option<String>,
    /// Track duration in seconds
    pub duration_secs: f64,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Bit depth
    pub bit_depth: u16,
    /// Bitrate in kbps
    pub bitrate: u32,
    /// BPM (beats per minute)
    pub bpm: f64,
    /// Musical key
    pub key: Option<Key>,
    /// Beat grid data
    pub beat_grid: BeatGrid,
    /// Waveform data (preview + detail)
    pub waveform: Waveform,
    /// Cue points (hot cues, memory cues, loops)
    pub cue_points: Vec<CuePoint>,
    /// File size in bytes
    pub file_size: u64,
    /// XXH3 hash of file for cache invalidation
    pub file_hash: u64,
    /// Year of release
    pub year: Option<u16>,
    /// Track comment
    pub comment: Option<String>,
    /// Track number in album
    pub track_number: Option<u32>,
    /// File type (MP3, FLAC, etc.)
    pub file_type: FileType,
}

/// Audio file type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u16)]
pub enum FileType {
    #[default]
    Unknown = 0x00,
    Mp3 = 0x01,
    M4a = 0x04,
    Flac = 0x05,
    Wav = 0x0B,
    Aiff = 0x0C,
}

impl FileType {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "mp3" => FileType::Mp3,
            "m4a" | "aac" => FileType::M4a,
            "flac" => FileType::Flac,
            "wav" => FileType::Wav,
            "aiff" | "aif" => FileType::Aiff,
            _ => FileType::Unknown,
        }
    }
}

/// Musical key in Open Key / Camelot notation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Key {
    /// Pitch class (0-11, where 0=C, 1=C#, etc.)
    pub pitch_class: u8,
    /// True for major, false for minor
    pub is_major: bool,
}

impl Key {
    /// Create a new key
    pub fn new(pitch_class: u8, is_major: bool) -> Self {
        Self {
            pitch_class: pitch_class % 12,
            is_major,
        }
    }
    
    /// Convert to Camelot wheel notation (1A-12B)
    pub fn to_camelot(&self) -> String {
        // Camelot mapping: minor keys are 'A', major keys are 'B'
        let camelot_map_minor = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10]; // Am=5A, etc.
        let camelot_map_major = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1]; // C=8B, etc.
        
        let pos = if self.is_major {
            camelot_map_major[self.pitch_class as usize]
        } else {
            camelot_map_minor[self.pitch_class as usize]
        };
        
        let suffix = if self.is_major { "B" } else { "A" };
        format!("{}{}", pos, suffix)
    }
    
    /// Convert to Open Key notation (1m-12d)
    pub fn to_open_key(&self) -> String {
        // Open Key maps differently
        let open_key_map = [1, 8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6];
        let pos = open_key_map[self.pitch_class as usize];
        let suffix = if self.is_major { "d" } else { "m" };
        format!("{}{}", pos, suffix)
    }
    
    /// Convert to Rekordbox's internal key ID (1-24)
    /// Based on observed export.pdb values
    pub fn to_rekordbox_id(&self) -> u8 {
        // Rekordbox key IDs follow the circle of fifths
        // Minor: 1=Cm, 2=Gm, 3=Dm, 4=Am, 5=Em, 6=Bm, 7=F#m, 8=C#m, 9=G#m, 10=D#m, 11=A#m, 12=Fm
        // Major: 13=C, 14=G, 15=D, 16=A, 17=E, 18=B, 19=F#, 20=C#, 21=G#, 22=D#, 23=A#, 24=F
        let minor_map = [1, 8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6]; // C=1, C#=8, D=3, etc.
        let id = minor_map[self.pitch_class as usize];
        if self.is_major {
            id + 12
        } else {
            id
        }
    }
    
    /// Get the key name (e.g., "Am", "C")
    pub fn name(&self) -> String {
        let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        let note = note_names[self.pitch_class as usize];
        if self.is_major {
            note.to_string()
        } else {
            format!("{}m", note)
        }
    }
}

/// Beat grid containing all beat positions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BeatGrid {
    /// Tempo in BPM
    pub bpm: f64,
    /// First beat position in milliseconds from track start
    pub first_beat_ms: f64,
    /// Beat positions
    pub beats: Vec<Beat>,
}

/// Single beat in the grid
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Beat {
    /// Position within bar (1-4 for 4/4 time)
    pub beat_number: u8,
    /// Time from track start in milliseconds
    pub time_ms: f64,
    /// Tempo at this beat (BPM × 100 for PQTZ format)
    pub tempo_100: u16,
}

impl BeatGrid {
    /// Generate a constant-tempo beat grid
    pub fn constant_tempo(bpm: f64, first_beat_ms: f64, duration_ms: f64) -> Self {
        let beat_duration_ms = 60_000.0 / bpm;
        let tempo_100 = (bpm * 100.0).round() as u16;

        let mut beats = Vec::new();
        let mut time = first_beat_ms;
        let mut beat_in_bar = 1u8;

        while time < duration_ms {
            beats.push(Beat {
                beat_number: beat_in_bar,
                time_ms: time,
                tempo_100,
            });

            time += beat_duration_ms;
            beat_in_bar = if beat_in_bar == 4 { 1 } else { beat_in_bar + 1 };
        }

        Self {
            bpm,
            first_beat_ms,
            beats,
        }
    }

    /// Number of beats
    pub fn len(&self) -> usize {
        self.beats.len()
    }

    pub fn is_empty(&self) -> bool {
        self.beats.is_empty()
    }
}

/// Cue point type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum CueType {
    /// Regular cue point
    Cue = 1,
    /// Fade-in point
    FadeIn = 2,
    /// Fade-out point
    FadeOut = 3,
    /// Load point (where track starts playing)
    Load = 4,
    /// Loop point
    Loop = 5,
}

impl Default for CueType {
    fn default() -> Self {
        CueType::Cue
    }
}

/// Cue point for PCOB section
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CuePoint {
    /// Hot cue number (0-7 for hot cues, 0 for memory cues)
    pub hot_cue: u8,
    /// Cue type
    pub cue_type: CueType,
    /// Time in milliseconds from track start
    pub time_ms: f64,
    /// Loop length in milliseconds (0 if not a loop)
    pub loop_ms: f64,
    /// Optional comment/label
    pub comment: Option<String>,
}

/// Waveform data for both preview and detail displays
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Waveform {
    /// Preview waveform (400 entries, monochrome)
    pub preview: WaveformPreview,
    /// Detail color waveform (150 entries/second)
    pub detail: WaveformDetail,
}

/// Preview waveform (PWAV format - 400 bytes total)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WaveformPreview {
    /// 400 columns, each with height (0-31) and whiteness (0-7)
    pub columns: Vec<WaveformColumn>,
}

/// Single column in preview waveform
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct WaveformColumn {
    /// Height 0-31 (5 bits)
    pub height: u8,
    /// Whiteness 0-7 (3 bits) - higher = more white/louder
    pub whiteness: u8,
}

impl WaveformColumn {
    /// Encode to PWAV byte format: height in bits 7-3, whiteness in bits 2-0
    pub fn to_byte(&self) -> u8 {
        ((self.height & 0x1F) << 3) | (self.whiteness & 0x07)
    }
    
    /// Decode from PWAV byte
    pub fn from_byte(byte: u8) -> Self {
        Self {
            height: (byte >> 3) & 0x1F,
            whiteness: byte & 0x07,
        }
    }
}

/// Detail color waveform (PWV5 format - 150 entries/second)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WaveformDetail {
    /// Color entries at 150/second rate
    pub entries: Vec<WaveformColorEntry>,
}

/// Color waveform entry (PWV5 format)
/// RGB represents frequency bands: Red=bass, Green=mids, Blue=highs
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct WaveformColorEntry {
    /// Red channel 0-7 (3 bits) - bass energy (20-200Hz)
    pub red: u8,
    /// Green channel 0-7 (3 bits) - mid energy (200Hz-4kHz)
    pub green: u8,
    /// Blue channel 0-7 (3 bits) - high energy (4-20kHz)
    pub blue: u8,
    /// Height 0-31 (5 bits) - overall amplitude
    pub height: u8,
}

impl WaveformColorEntry {
    /// Encode to PWV5 2-byte format (big-endian for ANLZ)
    /// Bits 15-13: red, 12-10: green, 9-7: blue, 6-2: height, 1-0: unused
    pub fn to_bytes(&self) -> [u8; 2] {
        let value: u16 = 
            ((self.red as u16 & 0x07) << 13) |
            ((self.green as u16 & 0x07) << 10) |
            ((self.blue as u16 & 0x07) << 7) |
            ((self.height as u16 & 0x1F) << 2);
        value.to_be_bytes() // ANLZ files are big-endian
    }
    
    /// Decode from PWV5 bytes
    pub fn from_bytes(bytes: [u8; 2]) -> Self {
        let value = u16::from_be_bytes(bytes);
        Self {
            red: ((value >> 13) & 0x07) as u8,
            green: ((value >> 10) & 0x07) as u8,
            blue: ((value >> 7) & 0x07) as u8,
            height: ((value >> 2) & 0x1F) as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_key_camelot() {
        // A minor = 8A (relative minor of C major)
        let am = Key::new(9, false);
        assert_eq!(am.to_camelot(), "8A");

        // C major = 8B
        let c = Key::new(0, true);
        assert_eq!(c.to_camelot(), "8B");

        // C minor = 5A
        let cm = Key::new(0, false);
        assert_eq!(cm.to_camelot(), "5A");
    }
    
    #[test]
    fn test_key_rekordbox_id() {
        // C minor should be 1
        let cm = Key::new(0, false);
        assert_eq!(cm.to_rekordbox_id(), 1);
        
        // C major should be 13
        let c = Key::new(0, true);
        assert_eq!(c.to_rekordbox_id(), 13);
    }
    
    #[test]
    fn test_waveform_encoding() {
        let entry = WaveformColorEntry {
            red: 5,
            green: 3,
            blue: 7,
            height: 20,
        };
        let bytes = entry.to_bytes();
        let decoded = WaveformColorEntry::from_bytes(bytes);
        assert_eq!(entry.red, decoded.red);
        assert_eq!(entry.green, decoded.green);
        assert_eq!(entry.blue, decoded.blue);
        assert_eq!(entry.height, decoded.height);
    }
    
    #[test]
    fn test_beat_grid_generation() {
        let grid = BeatGrid::constant_tempo(128.0, 100.0, 10_000.0);
        assert!(!grid.is_empty());
        // At 128 BPM, ~468.75ms per beat, so ~21 beats in ~10 seconds
        assert!(grid.len() > 20);
        assert_eq!(grid.beats[0].beat_number, 1);
        assert_eq!(grid.beats[0].tempo_100, 12800);
    }
    
    #[test]
    fn test_file_type_from_extension() {
        assert_eq!(FileType::from_extension("mp3"), FileType::Mp3);
        assert_eq!(FileType::from_extension("MP3"), FileType::Mp3);
        assert_eq!(FileType::from_extension("flac"), FileType::Flac);
        assert_eq!(FileType::from_extension("unknown"), FileType::Unknown);
    }
}
