//! ANLZ file generation (.DAT, .EXT, .2EX)
//!
//! ANLZ files are big-endian and contain tagged sections:
//! - PMAI: File header
//! - PQTZ: Beat grid
//! - PWAV: Preview waveform (monochrome)
//! - PWV5: Detail waveform (color)
//! - PPTH: File path

use binrw::{binrw, BinWrite};
use std::io::{Cursor, Write};

use crate::track::{BeatGrid, Waveform, Beat};
use crate::error::Result;

/// ANLZ file header magic
const PMAI_MAGIC: &[u8; 4] = b"PMAI";
const PQTZ_MAGIC: &[u8; 4] = b"PQTZ";
const PWAV_MAGIC: &[u8; 4] = b"PWAV";
const PWV5_MAGIC: &[u8; 4] = b"PWV5";
const PPTH_MAGIC: &[u8; 4] = b"PPTH";

/// Generate a complete ANLZ .DAT file
pub fn generate_dat_file(
    beat_grid: &BeatGrid,
    waveform: &Waveform,
    file_path: &str,
) -> Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(64 * 1024); // Pre-allocate 64KB
    
    // We'll write sections to a temporary buffer first to calculate total size
    let pqtz_section = generate_pqtz_section(beat_grid)?;
    let pwav_section = generate_pwav_section(&waveform.preview)?;
    let pwv5_section = generate_pwv5_section(&waveform.detail)?;
    let ppth_section = generate_ppth_section(file_path)?;
    
    // Calculate total file size
    let sections_size = pqtz_section.len() + pwav_section.len() + 
                        pwv5_section.len() + ppth_section.len();
    let total_size = 28 + sections_size; // PMAI header is 28 bytes
    
    // Write PMAI header
    buffer.extend_from_slice(PMAI_MAGIC);
    buffer.extend_from_slice(&24u32.to_be_bytes()); // Header length (after magic)
    buffer.extend_from_slice(&(total_size as u32).to_be_bytes()); // File length
    // Remaining header bytes (padding/reserved)
    buffer.extend_from_slice(&[0u8; 16]);
    
    // Write sections
    buffer.extend_from_slice(&pqtz_section);
    buffer.extend_from_slice(&pwav_section);
    buffer.extend_from_slice(&pwv5_section);
    buffer.extend_from_slice(&ppth_section);
    
    Ok(buffer)
}

/// Generate PQTZ (beat grid) section
fn generate_pqtz_section(beat_grid: &BeatGrid) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    
    // Section header
    buffer.extend_from_slice(PQTZ_MAGIC);
    
    // Header length (24 bytes after magic for PQTZ)
    buffer.extend_from_slice(&24u32.to_be_bytes());
    
    // Total section length (header + beat entries)
    // Each beat entry is 8 bytes
    let beat_data_size = beat_grid.beats.len() * 8;
    let section_size = 24 + beat_data_size;
    buffer.extend_from_slice(&(section_size as u32).to_be_bytes());
    
    // Unknown fields (observed from real files)
    buffer.extend_from_slice(&[0u8; 4]); // Unknown1
    buffer.extend_from_slice(&[0u8; 4]); // Unknown2
    
    // Number of beats
    buffer.extend_from_slice(&(beat_grid.beats.len() as u32).to_be_bytes());
    
    // Write beat entries
    for beat in &beat_grid.beats {
        // Beat number (1-4)
        buffer.extend_from_slice(&(beat.beat_number as u16).to_be_bytes());
        // Tempo as BPM × 100
        buffer.extend_from_slice(&beat.tempo_100.to_be_bytes());
        // Time in milliseconds
        buffer.extend_from_slice(&(beat.time_ms as u32).to_be_bytes());
    }
    
    Ok(buffer)
}

/// Generate PWAV (preview waveform) section - 400 bytes
fn generate_pwav_section(preview: &crate::track::WaveformPreview) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    
    buffer.extend_from_slice(PWAV_MAGIC);
    buffer.extend_from_slice(&20u32.to_be_bytes()); // Header length
    buffer.extend_from_slice(&((20 + 400) as u32).to_be_bytes()); // Total section length
    
    // Unknown header bytes
    buffer.extend_from_slice(&[0u8; 8]);
    
    // Waveform data - exactly 400 bytes
    // Pad or truncate to 400
    let mut waveform_data = Vec::with_capacity(400);
    for (i, col) in preview.columns.iter().take(400).enumerate() {
        waveform_data.push(col.to_byte());
    }
    // Pad if less than 400
    while waveform_data.len() < 400 {
        waveform_data.push(0);
    }
    buffer.extend_from_slice(&waveform_data);
    
    Ok(buffer)
}

/// Generate PWV5 (detail color waveform) section
fn generate_pwv5_section(detail: &crate::track::WaveformDetail) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    
    buffer.extend_from_slice(PWV5_MAGIC);
    buffer.extend_from_slice(&20u32.to_be_bytes()); // Header length
    
    // Each entry is 2 bytes
    let data_size = detail.entries.len() * 2;
    let section_size = 20 + data_size;
    buffer.extend_from_slice(&(section_size as u32).to_be_bytes());
    
    // Entry count
    buffer.extend_from_slice(&(detail.entries.len() as u32).to_be_bytes());
    
    // Unknown
    buffer.extend_from_slice(&[0u8; 4]);
    
    // Waveform entries
    for entry in &detail.entries {
        buffer.extend_from_slice(&entry.to_bytes());
    }
    
    Ok(buffer)
}

/// Generate PPTH (file path) section
fn generate_ppth_section(file_path: &str) -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    
    buffer.extend_from_slice(PPTH_MAGIC);
    buffer.extend_from_slice(&16u32.to_be_bytes()); // Header length
    
    // Path is encoded as UTF-16BE with length prefix
    let path_utf16: Vec<u16> = file_path.encode_utf16().collect();
    let path_bytes: Vec<u8> = path_utf16.iter()
        .flat_map(|c| c.to_be_bytes())
        .collect();
    
    // Section size: header + length field (4 bytes) + path bytes
    let section_size = 16 + 4 + path_bytes.len();
    buffer.extend_from_slice(&(section_size as u32).to_be_bytes());
    
    // Path length in characters
    buffer.extend_from_slice(&(path_utf16.len() as u32).to_be_bytes());
    
    // Path data
    buffer.extend_from_slice(&path_bytes);
    
    Ok(buffer)
}

/// ANLZ path generation helper
/// Converts track ID to the standard Pioneer path format:
/// PIONEER/USBANLZ/P0xx/[hex]/ANLZ0000.DAT
pub fn generate_anlz_path(track_id: u32) -> String {
    // Pioneer uses a hierarchical structure based on track ID
    let dir1 = format!("P{:03}", (track_id / 256) % 1000);
    let dir2 = format!("{:08X}", track_id);
    format!("PIONEER/USBANLZ/{}/{}/ANLZ0000.DAT", dir1, dir2)
}

/// Generate .EXT file (extended analysis for Nexus+ players)
pub fn generate_ext_file(
    beat_grid: &BeatGrid,
    waveform: &Waveform,
    file_path: &str,
) -> Result<Vec<u8>> {
    // .EXT has same structure as .DAT but may include additional sections
    // For now, generate same content - can be extended later
    generate_dat_file(beat_grid, waveform, file_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{WaveformPreview, WaveformDetail, WaveformColumn, WaveformColorEntry};
    
    #[test]
    fn test_anlz_path_generation() {
        assert_eq!(
            generate_anlz_path(1),
            "PIONEER/USBANLZ/P000/00000001/ANLZ0000.DAT"
        );
        assert_eq!(
            generate_anlz_path(256),
            "PIONEER/USBANLZ/P001/00000100/ANLZ0000.DAT"
        );
    }
    
    #[test]
    fn test_pqtz_generation() {
        use crate::track::BeatGrid;
        let grid = BeatGrid::constant_tempo(128.0, 0.0, 5000.0);
        let section = generate_pqtz_section(&grid).unwrap();
        
        // Check magic
        assert_eq!(&section[0..4], b"PQTZ");
    }
}
