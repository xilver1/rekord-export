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
    /// BPM (beats per minute)
    pub bpm: f64,
    /// Musical key
    pub key: Option<Key>,
    /// Beat grid data
    pub beat_grid: BeatGrid,
    /// Waveform data (preview + detail)
    pub waveform: Waveform,
    /// File size in bytes
    pub file_size: u64,
    /// XXH3 hash of file for cache invalidation
    pub file_hash: u64,
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
    /// Convert to Camelot wheel notation (1A-12B)
    pub fn to_camelot(&self) -> String {
        // Camelot mapping: minor keys are 'A', major keys are 'B'
        // The wheel position depends on pitch class
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
        let open_key_map_minor = [1, 8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6];
        let open_key_map_major = [1, 8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6];
        
        let pos = if self.is_major {
            open_key_map_major[self.pitch_class as usize]
        } else {
            open_key_map_minor[self.pitch_class as usize]
        };
        
        let suffix = if self.is_major { "d" } else { "m" };
        format!("{}{}", pos, suffix)
    }
    
    /// Convert to Rekordbox's internal key ID (1-24)
    pub fn to_rekordbox_id(&self) -> u8 {
        // Rekordbox uses 1-12 for minor, 13-24 for major (roughly)
        // This mapping is based on observed export.pdb values
        if self.is_major {
            13 + self.pitch_class
        } else {
            1 + self.pitch_class
        }
    }
}

/// Beat grid containing all beat positions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatGrid {
    /// Tempo in BPM (may vary for dynamic tempo tracks)
    pub bpm: f64,
    /// First beat position in milliseconds from track start
    pub first_beat_ms: f64,
    /// Beat positions: Vec of (beat_number 1-4, time_ms)
    /// beat_number indicates position within bar (1=downbeat)
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

/// Waveform data for both preview and detail displays
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Waveform {
    /// Preview waveform (400 entries, monochrome)
    pub preview: WaveformPreview,
    /// Detail color waveform (150 entries/second)
    pub detail: WaveformDetail,
}

/// Preview waveform (PWAV format - 400 bytes total)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformPreview {
    /// 400 columns, each with height (0-31) and whiteness (0-7)
    pub columns: Vec<WaveformColumn>,
}

/// Single column in preview waveform
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformDetail {
    /// Color entries at 150/second rate
    pub entries: Vec<WaveformColorEntry>,
}

/// Color waveform entry (PWV5 format)
/// RGB represents frequency bands: Red=bass, Green=mids, Blue=highs
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    /// Encode to PWV5 2-byte format
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
        // A minor = 5A
        let am = Key { pitch_class: 9, is_major: false };
        assert_eq!(am.to_camelot(), "5A");
        
        // C major = 8B
        let c = Key { pitch_class: 0, is_major: true };
        assert_eq!(c.to_camelot(), "8B");
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
        // At 128 BPM, ~468.75ms per beat, so ~21 beats in 10 seconds
        assert!(grid.len() > 20);
        assert_eq!(grid.beats[0].beat_number, 1);
        assert_eq!(grid.beats[0].tempo_100, 12800);
    }
}
