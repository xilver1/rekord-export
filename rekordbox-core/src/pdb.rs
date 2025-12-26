//! PDB (DeviceSQL) database generation
//!
//! The export.pdb file is a little-endian database with a specific page structure.
//! This module generates valid PDB files that CDJs can read.
//!
//! Reference: https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html

use std::collections::HashMap;
use std::io::Write;

use crate::error::{Error, Result};
use crate::page::{PageBuilder, PageType, TablePointer, FileHeader, PAGE_SIZE, HEAP_START};
use crate::string::{encode_string, encode_isrc, encoded_length};
use crate::track::TrackAnalysis;

/// Row subtypes for offset size determination
const SUBTYPE_NEAR: u16 = 0x0060; // 1-byte offsets (artist, album short)
const SUBTYPE_FAR: u16 = 0x0064;  // 2-byte offsets (artist, album long)
const SUBTYPE_TRACK: u16 = 0x0024; // Track rows always use 2-byte offsets

/// High-level database builder
pub struct PdbBuilder {
    tracks: Vec<TrackInfo>,
    artists: HashMap<String, u32>,
    albums: HashMap<(String, u32), u32>, // (album_name, artist_id) -> album_id
    genres: HashMap<String, u32>,
    keys: HashMap<u8, u32>, // rekordbox_key_id -> row_id
    playlists: Vec<PlaylistInfo>,
    next_artist_id: u32,
    next_album_id: u32,
    next_genre_id: u32,
    next_key_id: u32,
}

/// Internal track representation
struct TrackInfo {
    analysis: TrackAnalysis,
    artist_id: u32,
    album_id: u32,
    genre_id: u32,
    key_id: u32,
    analyze_path: String,
}

/// Playlist information
pub struct PlaylistInfo {
    pub id: u32,
    pub parent_id: u32,
    pub name: String,
    pub is_folder: bool,
    pub sort_order: u32,
    pub track_ids: Vec<u32>,
}

impl PdbBuilder {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            artists: HashMap::new(),
            albums: HashMap::new(),
            genres: HashMap::new(),
            keys: HashMap::new(),
            playlists: Vec::new(),
            next_artist_id: 1,
            next_album_id: 1,
            next_genre_id: 1,
            next_key_id: 1,
        }
    }
    
    /// Add a track and return its ID
    pub fn add_track(&mut self, analysis: &TrackAnalysis, analyze_path: &str) -> u32 {
        let track_id = analysis.id;
        
        // Get or create artist ID
        let artist_id = self.get_or_create_artist(&analysis.artist);
        
        // Get or create album ID (associated with artist)
        let album_id = analysis.album.as_ref()
            .map(|a| self.get_or_create_album(a, artist_id))
            .unwrap_or(0);
        
        // Get or create genre ID  
        let genre_id = analysis.genre.as_ref()
            .map(|g| self.get_or_create_genre(g))
            .unwrap_or(0);
        
        // Get or create key ID
        let key_id = analysis.key
            .map(|k| self.get_or_create_key(k.to_rekordbox_id(), &k.name()))
            .unwrap_or(0);
        
        self.tracks.push(TrackInfo {
            analysis: analysis.clone(),
            artist_id,
            album_id,
            genre_id,
            key_id,
            analyze_path: analyze_path.to_string(),
        });
        
        track_id
    }
    
    /// Add a playlist
    pub fn add_playlist(&mut self, id: u32, parent_id: u32, name: &str, track_ids: Vec<u32>) {
        self.playlists.push(PlaylistInfo {
            id,
            parent_id,
            name: name.to_string(),
            is_folder: false,
            sort_order: self.playlists.len() as u32,
            track_ids,
        });
    }
    
    /// Add a playlist folder
    pub fn add_folder(&mut self, id: u32, parent_id: u32, name: &str) {
        self.playlists.push(PlaylistInfo {
            id,
            parent_id,
            name: name.to_string(),
            is_folder: true,
            sort_order: self.playlists.len() as u32,
            track_ids: Vec::new(),
        });
    }
    
    fn get_or_create_artist(&mut self, name: &str) -> u32 {
        if name.is_empty() {
            return 0;
        }
        if let Some(&id) = self.artists.get(name) {
            return id;
        }
        let id = self.next_artist_id;
        self.next_artist_id += 1;
        self.artists.insert(name.to_string(), id);
        id
    }
    
    fn get_or_create_album(&mut self, name: &str, artist_id: u32) -> u32 {
        if name.is_empty() {
            return 0;
        }
        let key = (name.to_string(), artist_id);
        if let Some(&id) = self.albums.get(&key) {
            return id;
        }
        let id = self.next_album_id;
        self.next_album_id += 1;
        self.albums.insert(key, id);
        id
    }
    
    fn get_or_create_genre(&mut self, name: &str) -> u32 {
        if name.is_empty() {
            return 0;
        }
        if let Some(&id) = self.genres.get(name) {
            return id;
        }
        let id = self.next_genre_id;
        self.next_genre_id += 1;
        self.genres.insert(name.to_string(), id);
        id
    }
    
    fn get_or_create_key(&mut self, rekordbox_id: u8, _name: &str) -> u32 {
        if let Some(&id) = self.keys.get(&rekordbox_id) {
            return id;
        }
        let id = self.next_key_id;
        self.next_key_id += 1;
        self.keys.insert(rekordbox_id, id);
        id
    }
    
    /// Build the complete PDB file
    pub fn build(&self) -> Result<Vec<u8>> {
        let mut all_pages: Vec<Vec<u8>> = Vec::new();
        let mut header = FileHeader::new();
        
        // Reserve page 0 for header
        all_pages.push(vec![0u8; PAGE_SIZE]);
        let mut next_page_index = 1u32;
        
        // Build each table
        // Order matters - tables are identified by type ID
        
        // Tracks (type 0)
        let (track_pages, track_first, track_last) = 
            self.build_track_pages(&mut next_page_index)?;
        if !track_pages.is_empty() {
            header.add_table(TablePointer::new(PageType::Tracks, track_first, track_last));
            all_pages.extend(track_pages);
        }
        
        // Genres (type 1)
        let (genre_pages, genre_first, genre_last) = 
            self.build_genre_pages(&mut next_page_index)?;
        if !genre_pages.is_empty() {
            header.add_table(TablePointer::new(PageType::Genres, genre_first, genre_last));
            all_pages.extend(genre_pages);
        }
        
        // Artists (type 2)
        let (artist_pages, artist_first, artist_last) = 
            self.build_artist_pages(&mut next_page_index)?;
        if !artist_pages.is_empty() {
            header.add_table(TablePointer::new(PageType::Artists, artist_first, artist_last));
            all_pages.extend(artist_pages);
        }
        
        // Albums (type 3)
        let (album_pages, album_first, album_last) = 
            self.build_album_pages(&mut next_page_index)?;
        if !album_pages.is_empty() {
            header.add_table(TablePointer::new(PageType::Albums, album_first, album_last));
            all_pages.extend(album_pages);
        }
        
        // Keys (type 5)
        let (key_pages, key_first, key_last) = 
            self.build_key_pages(&mut next_page_index)?;
        if !key_pages.is_empty() {
            header.add_table(TablePointer::new(PageType::Keys, key_first, key_last));
            all_pages.extend(key_pages);
        }
        
        // Playlist tree (type 7)
        let (tree_pages, tree_first, tree_last) = 
            self.build_playlist_tree_pages(&mut next_page_index)?;
        if !tree_pages.is_empty() {
            header.add_table(TablePointer::new(PageType::PlaylistTree, tree_first, tree_last));
            all_pages.extend(tree_pages);
        }
        
        // Playlist entries (type 8)
        let (entry_pages, entry_first, entry_last) = 
            self.build_playlist_entry_pages(&mut next_page_index)?;
        if !entry_pages.is_empty() {
            header.add_table(TablePointer::new(PageType::PlaylistEntries, entry_first, entry_last));
            all_pages.extend(entry_pages);
        }
        
        // Update header with final page count
        header.next_unused_page = next_page_index;
        all_pages[0] = header.to_page();
        
        // Flatten to single buffer
        let mut output = Vec::with_capacity(all_pages.len() * PAGE_SIZE);
        for page in all_pages {
            output.extend_from_slice(&page);
        }
        
        Ok(output)
    }
    
    /// Build track table pages
    fn build_track_pages(&self, next_idx: &mut u32) -> Result<(Vec<Vec<u8>>, u32, u32)> {
        if self.tracks.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }
        
        let first_page = *next_idx;
        let mut pages: Vec<Vec<u8>> = Vec::new();
        let mut current_page = PageBuilder::new(*next_idx, PageType::Tracks);
        *next_idx += 1;
        
        for track in &self.tracks {
            let row_data = self.build_track_row(track)?;
            
            // Check if row fits in current page
            if current_page.would_overflow(row_data.len()) {
                // Finalize current page, start new one
                let next = *next_idx;
                pages.push(current_page.finalize(next));
                current_page = PageBuilder::new(next, PageType::Tracks);
                *next_idx += 1;
            }
            
            current_page.write_row(&row_data)?;
        }
        
        // Finalize last page
        let last_page = current_page.page_index();
        pages.push(current_page.finalize(0xFFFFFFFF));
        
        Ok((pages, first_page, last_page))
    }
    
    /// Build a single track row
    fn build_track_row(&self, track: &TrackInfo) -> Result<Vec<u8>> {
        let analysis = &track.analysis;
        
        // Track row has fixed fields + 21 string offsets
        // We need to calculate the total size first to determine string offsets
        
        // Fixed part: 0x5E bytes (94 bytes) before string offsets
        // Then 21 × 2-byte offsets = 42 bytes
        // Total fixed header: 136 bytes
        const FIXED_SIZE: usize = 0x5E;
        const STRING_COUNT: usize = 21;
        const HEADER_SIZE: usize = FIXED_SIZE + STRING_COUNT * 2;
        
        // Build all strings
        let strings: Vec<Vec<u8>> = vec![
            encode_isrc(""), // 0: ISRC
            encode_string(""), // 1: lyricist
            encode_string(""), // 2: unknown (version?)
            encode_string(""), // 3: unknown
            encode_string(""), // 4: unknown
            encode_string(""), // 5: message
            encode_string(""), // 6: publish_track_info
            encode_string(""), // 7: autoload_hotcues
            encode_string(""), // 8: unknown
            encode_string(""), // 9: unknown
            encode_string(""), // 10: date_added
            encode_string(analysis.year.map(|y| format!("{}-01-01", y)).as_deref().unwrap_or("")), // 11: release_date
            encode_string(""), // 12: mix_name
            encode_string(""), // 13: unknown
            encode_string(&track.analyze_path), // 14: analyze_path
            encode_string(""), // 15: analyze_date
            encode_string(analysis.comment.as_deref().unwrap_or("")), // 16: comment
            encode_string(&analysis.title), // 17: title
            encode_string(""), // 18: unknown
            encode_string(&analysis.file_path.split('/').last().unwrap_or(&analysis.file_path)), // 19: filename
            encode_string(&analysis.file_path), // 20: file_path
        ];
        
        // Calculate offsets (relative to row start)
        let mut string_offsets = Vec::with_capacity(STRING_COUNT);
        let mut current_offset = HEADER_SIZE;
        for s in &strings {
            string_offsets.push(current_offset as u16);
            current_offset += s.len();
        }
        
        // Build the row
        let mut row = Vec::with_capacity(current_offset);
        
        // Fixed fields (0x00 - 0x5D)
        // 0x00-0x01: subtype (0x0024 for track with 2-byte offsets)
        row.extend_from_slice(&SUBTYPE_TRACK.to_le_bytes());
        
        // 0x02-0x03: index_shift
        row.extend_from_slice(&0u16.to_le_bytes());
        
        // 0x04-0x07: bitmask
        row.extend_from_slice(&0u32.to_le_bytes());
        
        // 0x08-0x0B: sample_rate
        row.extend_from_slice(&analysis.sample_rate.to_le_bytes());
        
        // 0x0C-0x0F: composer_id
        row.extend_from_slice(&0u32.to_le_bytes());
        
        // 0x10-0x13: file_size
        row.extend_from_slice(&(analysis.file_size as u32).to_le_bytes());
        
        // 0x14-0x17: unknown2
        row.extend_from_slice(&0u32.to_le_bytes());
        
        // 0x18-0x19: u3 (usually 19048)
        row.extend_from_slice(&19048u16.to_le_bytes());
        
        // 0x1A-0x1B: u4 (usually 30967)
        row.extend_from_slice(&30967u16.to_le_bytes());
        
        // 0x1C-0x1F: artwork_id
        row.extend_from_slice(&0u32.to_le_bytes());
        
        // 0x20-0x23: key_id
        row.extend_from_slice(&track.key_id.to_le_bytes());
        
        // 0x24-0x27: original_artist_id
        row.extend_from_slice(&0u32.to_le_bytes());
        
        // 0x28-0x2B: label_id
        row.extend_from_slice(&0u32.to_le_bytes());
        
        // 0x2C-0x2F: remixer_id
        row.extend_from_slice(&0u32.to_le_bytes());
        
        // 0x30-0x33: bitrate
        let bitrate = 320u32; // TODO: extract from file
        row.extend_from_slice(&bitrate.to_le_bytes());
        
        // 0x34-0x37: track_number
        row.extend_from_slice(&analysis.track_number.unwrap_or(0).to_le_bytes());
        
        // 0x38-0x3B: tempo (BPM × 100)
        let tempo = (analysis.bpm * 100.0) as u32;
        row.extend_from_slice(&tempo.to_le_bytes());
        
        // 0x3C-0x3F: genre_id
        row.extend_from_slice(&track.genre_id.to_le_bytes());
        
        // 0x40-0x43: album_id
        row.extend_from_slice(&track.album_id.to_le_bytes());
        
        // 0x44-0x47: artist_id
        row.extend_from_slice(&track.artist_id.to_le_bytes());
        
        // 0x48-0x4B: id
        row.extend_from_slice(&analysis.id.to_le_bytes());
        
        // 0x4C-0x4D: disc_number
        row.extend_from_slice(&1u16.to_le_bytes());
        
        // 0x4E-0x4F: play_count
        row.extend_from_slice(&0u16.to_le_bytes());
        
        // 0x50-0x51: year
        row.extend_from_slice(&analysis.year.unwrap_or(0).to_le_bytes());
        
        // 0x52-0x53: sample_depth
        row.extend_from_slice(&analysis.bit_depth.to_le_bytes());
        
        // 0x54-0x55: duration (seconds)
        row.extend_from_slice(&(analysis.duration_secs as u16).to_le_bytes());
        
        // 0x56-0x57: u5 (usually 29)
        row.extend_from_slice(&29u16.to_le_bytes());
        
        // 0x58: color_id
        row.push(0);
        
        // 0x59: rating
        row.push(0);
        
        // 0x5A-0x5B: file_type
        row.extend_from_slice(&(analysis.file_type as u16).to_le_bytes());
        
        // 0x5C-0x5D: u7 (string prefix, always 0x0003)
        row.extend_from_slice(&0x0003u16.to_le_bytes());
        
        // 0x5E onwards: string offsets (21 × 2 bytes)
        for offset in &string_offsets {
            row.extend_from_slice(&offset.to_le_bytes());
        }
        
        // Append string data
        for s in &strings {
            row.extend_from_slice(s);
        }
        
        Ok(row)
    }
    
    /// Build artist table pages
    fn build_artist_pages(&self, next_idx: &mut u32) -> Result<(Vec<Vec<u8>>, u32, u32)> {
        if self.artists.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }
        
        let first_page = *next_idx;
        let mut pages: Vec<Vec<u8>> = Vec::new();
        let mut current_page = PageBuilder::new(*next_idx, PageType::Artists);
        *next_idx += 1;
        
        // Sort artists by ID for deterministic output
        let mut artists: Vec<_> = self.artists.iter().collect();
        artists.sort_by_key(|(_, &id)| id);
        
        for (name, &id) in artists {
            let row_data = self.build_artist_row(id, name);
            
            if current_page.would_overflow(row_data.len()) {
                let next = *next_idx;
                pages.push(current_page.finalize(next));
                current_page = PageBuilder::new(next, PageType::Artists);
                *next_idx += 1;
            }
            
            current_page.write_row(&row_data)?;
        }
        
        let last_page = current_page.page_index();
        pages.push(current_page.finalize(0xFFFFFFFF));
        
        Ok((pages, first_page, last_page))
    }
    
    /// Build a single artist row
    fn build_artist_row(&self, id: u32, name: &str) -> Vec<u8> {
        let name_encoded = encode_string(name);
        let name_len = name_encoded.len();
        
        // Use near (1-byte) or far (2-byte) offset based on row size
        let use_near = name_len <= 200;
        
        let mut row = Vec::new();
        
        if use_near {
            // subtype: 0x0060
            row.extend_from_slice(&SUBTYPE_NEAR.to_le_bytes());
            // index_shift
            row.extend_from_slice(&0u16.to_le_bytes());
            // id
            row.extend_from_slice(&id.to_le_bytes());
            // 0x03 marker
            row.push(0x03);
            // name offset (1 byte): row header is 9 bytes, so name at offset 9
            row.push(9);
        } else {
            // subtype: 0x0064
            row.extend_from_slice(&SUBTYPE_FAR.to_le_bytes());
            // index_shift
            row.extend_from_slice(&0u16.to_le_bytes());
            // id
            row.extend_from_slice(&id.to_le_bytes());
            // 0x0003 marker
            row.extend_from_slice(&0x0003u16.to_le_bytes());
            // name offset (2 bytes): row header is 12 bytes
            row.extend_from_slice(&12u16.to_le_bytes());
        }
        
        // Append name string
        row.extend_from_slice(&name_encoded);
        
        row
    }
    
    /// Build album table pages
    fn build_album_pages(&self, next_idx: &mut u32) -> Result<(Vec<Vec<u8>>, u32, u32)> {
        if self.albums.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }
        
        let first_page = *next_idx;
        let mut pages: Vec<Vec<u8>> = Vec::new();
        let mut current_page = PageBuilder::new(*next_idx, PageType::Albums);
        *next_idx += 1;
        
        // Sort albums by ID
        let mut albums: Vec<_> = self.albums.iter().collect();
        albums.sort_by_key(|(_, &id)| id);
        
        for ((name, artist_id), &id) in albums {
            let row_data = self.build_album_row(id, *artist_id, name);
            
            if current_page.would_overflow(row_data.len()) {
                let next = *next_idx;
                pages.push(current_page.finalize(next));
                current_page = PageBuilder::new(next, PageType::Albums);
                *next_idx += 1;
            }
            
            current_page.write_row(&row_data)?;
        }
        
        let last_page = current_page.page_index();
        pages.push(current_page.finalize(0xFFFFFFFF));
        
        Ok((pages, first_page, last_page))
    }
    
    /// Build a single album row
    fn build_album_row(&self, id: u32, artist_id: u32, name: &str) -> Vec<u8> {
        let name_encoded = encode_string(name);
        let name_len = name_encoded.len();
        
        let use_near = name_len <= 200;
        
        let mut row = Vec::new();
        
        if use_near {
            // subtype: 0x0080
            row.extend_from_slice(&0x0080u16.to_le_bytes());
            // index_shift
            row.extend_from_slice(&0u16.to_le_bytes());
            // unknown2 (4 bytes)
            row.extend_from_slice(&0u32.to_le_bytes());
            // artist_id
            row.extend_from_slice(&artist_id.to_le_bytes());
            // id
            row.extend_from_slice(&id.to_le_bytes());
            // unknown3 (4 bytes)
            row.extend_from_slice(&0u32.to_le_bytes());
            // 0x03 marker
            row.push(0x03);
            // name offset (1 byte): header is 21 bytes, name at 21
            row.push(21);
        } else {
            // subtype: 0x0084
            row.extend_from_slice(&0x0084u16.to_le_bytes());
            // index_shift
            row.extend_from_slice(&0u16.to_le_bytes());
            // unknown2
            row.extend_from_slice(&0u32.to_le_bytes());
            // artist_id
            row.extend_from_slice(&artist_id.to_le_bytes());
            // id
            row.extend_from_slice(&id.to_le_bytes());
            // unknown3
            row.extend_from_slice(&0u32.to_le_bytes());
            // 0x0003 marker
            row.extend_from_slice(&0x0003u16.to_le_bytes());
            // name offset (2 bytes): header is 24 bytes
            row.extend_from_slice(&24u16.to_le_bytes());
        }
        
        row.extend_from_slice(&name_encoded);
        
        row
    }
    
    /// Build genre table pages
    fn build_genre_pages(&self, next_idx: &mut u32) -> Result<(Vec<Vec<u8>>, u32, u32)> {
        if self.genres.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }
        
        let first_page = *next_idx;
        let mut pages: Vec<Vec<u8>> = Vec::new();
        let mut current_page = PageBuilder::new(*next_idx, PageType::Genres);
        *next_idx += 1;
        
        let mut genres: Vec<_> = self.genres.iter().collect();
        genres.sort_by_key(|(_, &id)| id);
        
        for (name, &id) in genres {
            let row_data = self.build_genre_row(id, name);
            
            if current_page.would_overflow(row_data.len()) {
                let next = *next_idx;
                pages.push(current_page.finalize(next));
                current_page = PageBuilder::new(next, PageType::Genres);
                *next_idx += 1;
            }
            
            current_page.write_row(&row_data)?;
        }
        
        let last_page = current_page.page_index();
        pages.push(current_page.finalize(0xFFFFFFFF));
        
        Ok((pages, first_page, last_page))
    }
    
    /// Build a single genre row
    /// Structure: id (4 bytes) + name (DeviceSQL string)
    fn build_genre_row(&self, id: u32, name: &str) -> Vec<u8> {
        let mut row = Vec::new();
        row.extend_from_slice(&id.to_le_bytes());
        row.extend_from_slice(&encode_string(name));
        row
    }
    
    /// Build key table pages
    fn build_key_pages(&self, next_idx: &mut u32) -> Result<(Vec<Vec<u8>>, u32, u32)> {
        if self.keys.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }
        
        let first_page = *next_idx;
        let mut pages: Vec<Vec<u8>> = Vec::new();
        let mut current_page = PageBuilder::new(*next_idx, PageType::Keys);
        *next_idx += 1;
        
        // Build all 24 standard keys
        let key_names = [
            "Cm", "Gm", "Dm", "Am", "Em", "Bm", "F#m", "C#m", "G#m", "D#m", "A#m", "Fm",
            "C", "G", "D", "A", "E", "B", "F#", "C#", "G#", "D#", "A#", "F",
        ];
        
        for (i, name) in key_names.iter().enumerate() {
            let id = (i + 1) as u32;
            let row_data = self.build_key_row(id, name);
            
            if current_page.would_overflow(row_data.len()) {
                let next = *next_idx;
                pages.push(current_page.finalize(next));
                current_page = PageBuilder::new(next, PageType::Keys);
                *next_idx += 1;
            }
            
            current_page.write_row(&row_data)?;
        }
        
        let last_page = current_page.page_index();
        pages.push(current_page.finalize(0xFFFFFFFF));
        
        Ok((pages, first_page, last_page))
    }
    
    /// Build a single key row
    /// Structure: id (4 bytes) + id2 (4 bytes) + name (DeviceSQL string)
    fn build_key_row(&self, id: u32, name: &str) -> Vec<u8> {
        let mut row = Vec::new();
        row.extend_from_slice(&id.to_le_bytes());
        row.extend_from_slice(&id.to_le_bytes()); // id2 is same as id
        row.extend_from_slice(&encode_string(name));
        row
    }
    
    /// Build playlist tree pages
    fn build_playlist_tree_pages(&self, next_idx: &mut u32) -> Result<(Vec<Vec<u8>>, u32, u32)> {
        if self.playlists.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }
        
        let first_page = *next_idx;
        let mut pages: Vec<Vec<u8>> = Vec::new();
        let mut current_page = PageBuilder::new(*next_idx, PageType::PlaylistTree);
        *next_idx += 1;
        
        for playlist in &self.playlists {
            let row_data = self.build_playlist_tree_row(playlist);
            
            if current_page.would_overflow(row_data.len()) {
                let next = *next_idx;
                pages.push(current_page.finalize(next));
                current_page = PageBuilder::new(next, PageType::PlaylistTree);
                *next_idx += 1;
            }
            
            current_page.write_row(&row_data)?;
        }
        
        let last_page = current_page.page_index();
        pages.push(current_page.finalize(0xFFFFFFFF));
        
        Ok((pages, first_page, last_page))
    }
    
    /// Build a single playlist tree row
    fn build_playlist_tree_row(&self, playlist: &PlaylistInfo) -> Vec<u8> {
        let name_encoded = encode_string(&playlist.name);
        
        let mut row = Vec::new();
        
        // parent_id (4 bytes)
        row.extend_from_slice(&playlist.parent_id.to_le_bytes());
        
        // unknown (4 bytes)
        row.extend_from_slice(&0u32.to_le_bytes());
        
        // sort_order (4 bytes)
        row.extend_from_slice(&playlist.sort_order.to_le_bytes());
        
        // id (4 bytes)
        row.extend_from_slice(&playlist.id.to_le_bytes());
        
        // raw_is_folder (4 bytes)
        row.extend_from_slice(&(if playlist.is_folder { 1u32 } else { 0u32 }).to_le_bytes());
        
        // name (DeviceSQL string)
        row.extend_from_slice(&name_encoded);
        
        row
    }
    
    /// Build playlist entry pages
    fn build_playlist_entry_pages(&self, next_idx: &mut u32) -> Result<(Vec<Vec<u8>>, u32, u32)> {
        // Collect all entries
        let entries: Vec<_> = self.playlists.iter()
            .filter(|p| !p.is_folder)
            .flat_map(|p| {
                p.track_ids.iter().enumerate().map(move |(idx, &track_id)| {
                    (idx as u32, track_id, p.id)
                })
            })
            .collect();
        
        if entries.is_empty() {
            return Ok((Vec::new(), 0, 0));
        }
        
        let first_page = *next_idx;
        let mut pages: Vec<Vec<u8>> = Vec::new();
        let mut current_page = PageBuilder::new(*next_idx, PageType::PlaylistEntries);
        *next_idx += 1;
        
        for (entry_index, track_id, playlist_id) in entries {
            let row_data = self.build_playlist_entry_row(entry_index, track_id, playlist_id);
            
            if current_page.would_overflow(row_data.len()) {
                let next = *next_idx;
                pages.push(current_page.finalize(next));
                current_page = PageBuilder::new(next, PageType::PlaylistEntries);
                *next_idx += 1;
            }
            
            current_page.write_row(&row_data)?;
        }
        
        let last_page = current_page.page_index();
        pages.push(current_page.finalize(0xFFFFFFFF));
        
        Ok((pages, first_page, last_page))
    }
    
    /// Build a single playlist entry row
    fn build_playlist_entry_row(&self, entry_index: u32, track_id: u32, playlist_id: u32) -> Vec<u8> {
        let mut row = Vec::new();
        row.extend_from_slice(&entry_index.to_le_bytes());
        row.extend_from_slice(&track_id.to_le_bytes());
        row.extend_from_slice(&playlist_id.to_le_bytes());
        row
    }
}

impl Default for PdbBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::*;
    
    fn make_test_track(id: u32, title: &str, artist: &str) -> TrackAnalysis {
        TrackAnalysis {
            id,
            file_path: format!("Contents/{}.mp3", title),
            title: title.to_string(),
            artist: artist.to_string(),
            album: Some("Test Album".to_string()),
            genre: Some("Electronic".to_string()),
            duration_secs: 180.0,
            sample_rate: 44100,
            bit_depth: 16,
            bpm: 128.0,
            key: Some(Key::new(9, false)), // Am
            beat_grid: BeatGrid::default(),
            waveform: Waveform::default(),
            file_size: 5_000_000,
            file_hash: 0x12345678,
            year: Some(2024),
            comment: None,
            track_number: Some(1),
            file_type: FileType::Mp3,
        }
    }
    
    #[test]
    fn test_pdb_builder_basic() {
        let mut builder = PdbBuilder::new();
        
        let track = make_test_track(1, "Test Track", "Test Artist");
        builder.add_track(&track, "PIONEER/USBANLZ/P000/00000001/ANLZ0000.DAT");
        
        let data = builder.build().unwrap();
        
        // Should be at least header + one track page
        assert!(data.len() >= PAGE_SIZE * 2);
        assert_eq!(data.len() % PAGE_SIZE, 0);
        
        // Check header magic (page size at offset 4)
        let page_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        assert_eq!(page_size, PAGE_SIZE as u32);
    }
    
    #[test]
    fn test_pdb_with_playlists() {
        let mut builder = PdbBuilder::new();
        
        let track1 = make_test_track(1, "Track 1", "Artist A");
        let track2 = make_test_track(2, "Track 2", "Artist B");
        
        builder.add_track(&track1, "PIONEER/USBANLZ/P000/00000001/ANLZ0000.DAT");
        builder.add_track(&track2, "PIONEER/USBANLZ/P000/00000002/ANLZ0000.DAT");
        
        builder.add_playlist(1, 0, "My Playlist", vec![1, 2]);
        
        let data = builder.build().unwrap();
        assert!(data.len() >= PAGE_SIZE * 2);
    }
}
