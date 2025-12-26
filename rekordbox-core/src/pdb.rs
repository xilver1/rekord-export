//! PDB (DeviceSQL) database generation
//!
//! The export.pdb file is a little-endian database with a specific page structure.
//! Based on rekordcrate's structures and REX Go implementation.

use binrw::{binrw, BinWrite};
use std::io::{Cursor, Write, Seek, SeekFrom};
use std::collections::HashMap;

use crate::track::TrackAnalysis;
use crate::error::Result;

/// Page size in bytes (always 4096 for Pioneer databases)
const PAGE_SIZE: usize = 4096;

/// Database file header
#[binrw]
#[brw(little)]
pub struct PdbHeader {
    /// Always 0
    pub unknown1: u32,
    /// Page size (4096)
    pub page_size: u32,
    /// Total number of pages
    pub num_pages: u32,
    /// Unknown (observed as 0)
    pub unknown2: u32,
    /// Next unused page
    pub next_unused_page: u32,
    /// Unknown sequence
    pub unknown3: u32,
    /// Sequence counter
    pub sequence: u32,
    /// Gap/padding
    #[brw(pad_after = 4)]
    pub unknown4: u32,
    /// Table pointers (one per table type)
    pub tables: [TablePointer; 20],
}

/// Pointer to a table's pages
#[binrw]
#[brw(little)]
#[derive(Default, Clone, Copy)]
pub struct TablePointer {
    /// Table type ID
    pub table_type: u32,
    /// Empty flag
    pub empty_candidate: u32,
    /// First page index
    pub first_page: u32,
    /// Last page index  
    pub last_page: u32,
}

/// Table types in DeviceSQL
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableType {
    Tracks = 0,
    Genres = 1,
    Artists = 2,
    Albums = 3,
    Labels = 4,
    Keys = 5,
    Colors = 6,
    PlaylistTree = 7,
    PlaylistEntries = 8,
    Unknown9 = 9,
    Unknown10 = 10,
    HistoryPlaylists = 11,
    HistoryEntries = 12,
    Artwork = 13,
    Unknown14 = 14,
    Unknown15 = 15,
    Columns = 16,
    SortOrders = 17,
    Unknown18 = 18,
    Unknown19 = 19,
}

/// Page header
#[binrw]
#[brw(little)]
pub struct PageHeader {
    /// Page index (0-based)
    pub page_index: u32,
    /// Page type
    pub page_type: u32,
    /// Next page index (0xFFFFFFFF if none)
    pub next_page: u32,
    /// Unknown
    pub unknown1: u32,
    /// Unknown
    pub unknown2: u32,
    /// Number of row groups in this page
    pub num_row_groups: u8,
    /// Unknown
    pub unknown3: u8,
    /// Unknown
    pub unknown4: u16,
    /// Row group size (used entry area before heap)
    pub row_group_size: u16,
    /// Unknown
    pub unknown5: u16,
    /// Heap position
    pub heap_pos: u16,
    /// Padding to align
    pub padding: u16,
}

/// High-level database builder
pub struct PdbBuilder {
    tracks: Vec<TrackRow>,
    artists: HashMap<String, u32>,
    albums: HashMap<String, u32>,
    genres: HashMap<String, u32>,
    playlists: Vec<PlaylistInfo>,
    next_artist_id: u32,
    next_album_id: u32,
    next_genre_id: u32,
}

/// Internal track row representation
struct TrackRow {
    id: u32,
    title: String,
    artist_id: u32,
    album_id: u32,
    genre_id: u32,
    duration_secs: u32,
    bpm_100: u16,
    key_id: u8,
    file_path: String,
    analyze_path: String,
    file_size: u32,
    sample_rate: u32,
    bitrate: u32,
}

/// Playlist information
pub struct PlaylistInfo {
    pub name: String,
    pub track_ids: Vec<u32>,
}

impl PdbBuilder {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            artists: HashMap::new(),
            albums: HashMap::new(),
            genres: HashMap::new(),
            playlists: Vec::new(),
            next_artist_id: 1,
            next_album_id: 1,
            next_genre_id: 1,
        }
    }
    
    /// Add a track and return its ID
    pub fn add_track(&mut self, analysis: &TrackAnalysis, analyze_path: &str) -> u32 {
        let track_id = self.tracks.len() as u32 + 1;
        
        // Get or create artist ID
        let artist_id = self.get_or_create_artist(&analysis.artist);
        
        // Get or create album ID
        let album_id = analysis.album.as_ref()
            .map(|a| self.get_or_create_album(a))
            .unwrap_or(0);
        
        // Get or create genre ID  
        let genre_id = analysis.genre.as_ref()
            .map(|g| self.get_or_create_genre(g))
            .unwrap_or(0);
        
        // Key ID
        let key_id = analysis.key.map(|k| k.to_rekordbox_id()).unwrap_or(0);
        
        self.tracks.push(TrackRow {
            id: track_id,
            title: analysis.title.clone(),
            artist_id,
            album_id,
            genre_id,
            duration_secs: analysis.duration_secs as u32,
            bpm_100: (analysis.bpm * 100.0) as u16,
            key_id,
            file_path: analysis.file_path.clone(),
            analyze_path: analyze_path.to_string(),
            file_size: analysis.file_size as u32,
            sample_rate: analysis.sample_rate,
            bitrate: 320, // TODO: calculate from actual file
        });
        
        track_id
    }
    
    /// Add a playlist
    pub fn add_playlist(&mut self, name: &str, track_ids: Vec<u32>) {
        self.playlists.push(PlaylistInfo {
            name: name.to_string(),
            track_ids,
        });
    }
    
    fn get_or_create_artist(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.artists.get(name) {
            return id;
        }
        let id = self.next_artist_id;
        self.next_artist_id += 1;
        self.artists.insert(name.to_string(), id);
        id
    }
    
    fn get_or_create_album(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.albums.get(name) {
            return id;
        }
        let id = self.next_album_id;
        self.next_album_id += 1;
        self.albums.insert(name.to_string(), id);
        id
    }
    
    fn get_or_create_genre(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.genres.get(name) {
            return id;
        }
        let id = self.next_genre_id;
        self.next_genre_id += 1;
        self.genres.insert(name.to_string(), id);
        id
    }
    
    /// Build the complete PDB file
    pub fn build(&self) -> Result<Vec<u8>> {
        let mut pages: Vec<Vec<u8>> = Vec::new();
        
        // Page 0: Header page
        let header_page = self.build_header_page()?;
        pages.push(header_page);
        
        // Build table pages
        // For simplicity, we'll create minimal tables
        // A full implementation would need proper page allocation like REX
        
        // Tracks table (starts at page 1)
        let track_pages = self.build_track_pages()?;
        pages.extend(track_pages);
        
        // Artists table
        let artist_pages = self.build_artist_pages()?;
        pages.extend(artist_pages);
        
        // Albums table
        let album_pages = self.build_album_pages()?;
        pages.extend(album_pages);
        
        // Genres table
        let genre_pages = self.build_genre_pages()?;
        pages.extend(genre_pages);
        
        // Playlist pages
        let playlist_pages = self.build_playlist_pages()?;
        pages.extend(playlist_pages);
        
        // Flatten to bytes
        let total_size = pages.len() * PAGE_SIZE;
        let mut output = Vec::with_capacity(total_size);
        for page in pages {
            output.extend_from_slice(&page);
            // Pad to page size
            let padding = PAGE_SIZE - page.len();
            output.extend(std::iter::repeat(0u8).take(padding));
        }
        
        // Update header with final page counts
        self.update_header(&mut output)?;
        
        Ok(output)
    }
    
    fn build_header_page(&self) -> Result<Vec<u8>> {
        let mut page = vec![0u8; PAGE_SIZE];
        
        // Write header structure
        // This is a simplified version - full impl needs proper table pointers
        let mut cursor = Cursor::new(&mut page[..]);
        
        // Unknown1
        cursor.write_all(&0u32.to_le_bytes())?;
        // Page size
        cursor.write_all(&(PAGE_SIZE as u32).to_le_bytes())?;
        // Num pages (will be updated)
        cursor.write_all(&1u32.to_le_bytes())?;
        // Unknown2
        cursor.write_all(&0u32.to_le_bytes())?;
        // Next unused page
        cursor.write_all(&1u32.to_le_bytes())?;
        // Unknown3
        cursor.write_all(&0u32.to_le_bytes())?;
        // Sequence
        cursor.write_all(&1u32.to_le_bytes())?;
        // Unknown4
        cursor.write_all(&0u32.to_le_bytes())?;
        
        Ok(page)
    }
    
    fn build_track_pages(&self) -> Result<Vec<Vec<u8>>> {
        // Simplified: build one page with all tracks
        // Real implementation needs row group management
        let mut page = vec![0u8; PAGE_SIZE];
        
        // Page header
        let mut cursor = Cursor::new(&mut page[..]);
        cursor.write_all(&0u32.to_le_bytes())?; // Page index
        cursor.write_all(&(TableType::Tracks as u32).to_le_bytes())?;
        cursor.write_all(&0xFFFFFFFFu32.to_le_bytes())?; // No next page
        
        // TODO: Write track rows with proper DeviceSQL format
        // This requires implementing row groups and heap allocation
        
        Ok(vec![page])
    }
    
    fn build_artist_pages(&self) -> Result<Vec<Vec<u8>>> {
        let mut page = vec![0u8; PAGE_SIZE];
        Ok(vec![page])
    }
    
    fn build_album_pages(&self) -> Result<Vec<Vec<u8>>> {
        let mut page = vec![0u8; PAGE_SIZE];
        Ok(vec![page])
    }
    
    fn build_genre_pages(&self) -> Result<Vec<Vec<u8>>> {
        let mut page = vec![0u8; PAGE_SIZE];
        Ok(vec![page])
    }
    
    fn build_playlist_pages(&self) -> Result<Vec<Vec<u8>>> {
        let mut pages = Vec::new();
        
        // Playlist tree page
        let tree_page = vec![0u8; PAGE_SIZE];
        pages.push(tree_page);
        
        // Playlist entries page
        let entries_page = vec![0u8; PAGE_SIZE];
        pages.push(entries_page);
        
        Ok(pages)
    }
    
    fn update_header(&self, output: &mut [u8]) -> Result<()> {
        let num_pages = output.len() / PAGE_SIZE;
        output[8..12].copy_from_slice(&(num_pages as u32).to_le_bytes());
        Ok(())
    }
}

impl Default for PdbBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// DeviceSQL string encoding
/// First byte indicates format:
/// - 0x00: UTF-8
/// - 0x01: Invalid (?)
/// - 0x02: UTF-8 with BOM
/// - 0x03: UTF-16LE
/// - 0x90: "Long string" format with offset
pub fn encode_device_sql_string(s: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(s.len() + 2);
    result.push(0x03); // UTF-16LE format (most compatible)
    
    let utf16: Vec<u8> = s.encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();
    
    result.push((utf16.len() / 2) as u8); // Character count
    result.extend(utf16);
    result.extend_from_slice(&[0, 0]); // Null terminator
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_device_sql_string() {
        let encoded = encode_device_sql_string("Test");
        assert_eq!(encoded[0], 0x03); // UTF-16LE marker
        assert_eq!(encoded[1], 4); // 4 characters
    }
    
    #[test]
    fn test_pdb_builder() {
        let builder = PdbBuilder::new();
        let data = builder.build().unwrap();
        assert!(data.len() >= PAGE_SIZE);
        assert_eq!(data.len() % PAGE_SIZE, 0);
    }
}
