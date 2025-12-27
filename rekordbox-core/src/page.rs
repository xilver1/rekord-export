//! Page allocation for Pioneer DeviceSQL databases
//!
//! Pages are 4096 bytes with:
//! - Fixed header at offset 0x00-0x1F (32 bytes common header)
//! - For DATA pages: DataPageHeader at 0x20-0x27, heap from 0x28
//! - For INDEX pages: IndexHeader at 0x20-0x3B, index entries from 0x3C
//!
//! Every table requires:
//! 1. An INDEX page (flags 0x64) that points to the first data page
//! 2. One or more DATA pages (flags 0x24 or 0x34) with actual row content
//!
//! Row group structure (36 bytes per group, from rekordcrate):
//! - Bytes 0-31: row_offsets[0..16] (16 × u16, stored in REVERSE order)
//!   - row_offsets[15] = offset for row 0 (bit 0)
//!   - row_offsets[14] = offset for row 1 (bit 1)
//!   - etc.
//! - Bytes 32-33: presence_flags (u16 bitmask of which rows exist)
//! - Bytes 34-35: unknown/padding (u16)

use crate::error::{Error, Result};

/// Page size in bytes (always 4096 for Pioneer databases)
pub const PAGE_SIZE: usize = 4096;

/// Offset where heap data begins (for data pages)
pub const HEAP_START: usize = 0x28;

/// Size of each row group in the backward-growing index
/// 2 (padding) + 2 (flags) + 16*2 (offsets) = 36 bytes
pub const ROW_GROUP_SIZE: usize = 36;

/// Maximum rows per group
pub const ROWS_PER_GROUP: usize = 16;

/// Page flags
pub const PAGE_FLAGS_INDEX: u8 = 0x64;  // Index page
pub const PAGE_FLAGS_DATA: u8 = 0x24;   // Normal data page
pub const PAGE_FLAGS_DATA_TRACK: u8 = 0x34; // Data page (tracks use this)

/// Magic value for empty table index NextPage
pub const EMPTY_TABLE_MARKER: u32 = 0x03FFFFFF;

/// Page types (table types)
/// All 20 tables (types 0-19) must be present for rekordbox PC compatibility
/// Values from Kaitai struct spec: rekordbox_pdb.ksy
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageType {
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
    HistoryPlaylists = 11,  // Was incorrectly 13
    HistoryEntries = 12,    // Was incorrectly 14
    Artwork = 13,           // Was incorrectly 15
    Unknown14 = 14,
    Unknown15 = 15,
    Columns = 16,           // Was incorrectly 17
    Unknown17 = 17,         // uk17 in spec
    Unknown18 = 18,
    History = 19,           // Was incorrectly Unknown19
}

impl PageType {
    /// Get all 20 table types in order (required for rekordbox PC)
    /// Order matches Kaitai spec: rekordbox_pdb.ksy
    pub fn all_types() -> &'static [PageType] {
        &[
            PageType::Tracks,           // 0
            PageType::Genres,           // 1
            PageType::Artists,          // 2
            PageType::Albums,           // 3
            PageType::Labels,           // 4
            PageType::Keys,             // 5
            PageType::Colors,           // 6
            PageType::PlaylistTree,     // 7
            PageType::PlaylistEntries,  // 8
            PageType::Unknown9,         // 9
            PageType::Unknown10,        // 10
            PageType::HistoryPlaylists, // 11
            PageType::HistoryEntries,   // 12
            PageType::Artwork,          // 13
            PageType::Unknown14,        // 14
            PageType::Unknown15,        // 15
            PageType::Columns,          // 16
            PageType::Unknown17,        // 17
            PageType::Unknown18,        // 18
            PageType::History,          // 19
        ]
    }
    
    /// Get all table types that should be included in a minimal export
    pub fn required_types() -> &'static [PageType] {
        Self::all_types()
    }
}

/// Index page builder - creates the required index page for each table
pub struct IndexPageBuilder {
    data: Vec<u8>,
    page_index: u32,
    page_type: PageType,
}

impl IndexPageBuilder {
    /// Create a new index page
    pub fn new(page_index: u32, page_type: PageType) -> Self {
        let data = vec![0u8; PAGE_SIZE];
        Self {
            data,
            page_index,
            page_type,
        }
    }
    
    /// Finalize the index page
    /// - next_page: the data page that follows (or EMPTY_TABLE_MARKER if empty)
    /// - has_data: whether there's actual data in the data page
    pub fn finalize(mut self, data_page_index: u32, has_data: bool) -> Vec<u8> {
        // Common header (0x00-0x1F)
        // Bytes 0-3: Always zero
        // Bytes 4-7: page_index
        self.data[4..8].copy_from_slice(&self.page_index.to_le_bytes());
        
        // Bytes 8-11: page_type
        self.data[8..12].copy_from_slice(&(self.page_type as u32).to_le_bytes());
        
        // Bytes 12-15: next_page (points to data page)
        self.data[12..16].copy_from_slice(&data_page_index.to_le_bytes());
        
        // Bytes 16-19: transaction/version (1 for index pages)
        self.data[16..20].copy_from_slice(&1u32.to_le_bytes());
        
        // Bytes 20-23: unknown2 (zero)
        // Bytes 24-26: packed row counts (zero for index)
        
        // Byte 27: page_flags (0x64 for index page)
        self.data[27] = PAGE_FLAGS_INDEX;
        
        // Bytes 28-29: free_size (0 for index)
        // Bytes 30-31: used_size (0 for index)
        
        // Index header starts at 0x20
        // Bytes 0x20-0x21: Unknown1 (0x1fff)
        self.data[0x20..0x22].copy_from_slice(&0x1fffu16.to_le_bytes());
        
        // Bytes 0x22-0x23: Unknown2 (0x1fff)
        self.data[0x22..0x24].copy_from_slice(&0x1fffu16.to_le_bytes());
        
        // Bytes 0x24-0x25: Unknown3 (0x03ec)
        self.data[0x24..0x26].copy_from_slice(&0x03ecu16.to_le_bytes());
        
        // Bytes 0x26-0x27: NextOffset (0)
        
        // Bytes 0x28-0x2B: PageIndex (same as header)
        self.data[0x28..0x2C].copy_from_slice(&self.page_index.to_le_bytes());
        
        // Bytes 0x2C-0x2F: NextPage (EMPTY_TABLE_MARKER if no data, else data page)
        let index_next_page = if has_data { data_page_index } else { EMPTY_TABLE_MARKER };
        self.data[0x2C..0x30].copy_from_slice(&index_next_page.to_le_bytes());
        
        // Bytes 0x30-0x33: Unknown5 (0x03ffffff)
        self.data[0x30..0x34].copy_from_slice(&0x03FFFFFFu32.to_le_bytes());
        
        // Bytes 0x34-0x37: Unknown6 (0)
        
        // Bytes 0x38-0x39: NumEntries (0)
        // Bytes 0x3A-0x3B: FirstEmptyEntry (0x1fff)
        self.data[0x3A..0x3C].copy_from_slice(&0x1fffu16.to_le_bytes());
        
        // Fill rest of page with 0x1ffffff8 (index entry marker)
        for i in (0x3C..PAGE_SIZE - 20).step_by(4) {
            self.data[i..i+4].copy_from_slice(&0x1FFFFFF8u32.to_le_bytes());
        }
        // Last 20 bytes stay zero (observed in real files)
        
        self.data
    }
}

/// A single data page being built
pub struct PageBuilder {
    /// Raw page data
    data: Vec<u8>,
    /// Current heap write position (offset from page start)
    heap_pos: usize,
    /// Number of rows written
    row_count: usize,
    /// Page index in file
    page_index: u32,
    /// Page/table type
    page_type: PageType,
    /// Row offsets (relative to HEAP_START)
    row_offsets: Vec<u16>,
}

impl PageBuilder {
    /// Create a new data page
    pub fn new(page_index: u32, page_type: PageType) -> Self {
        let data = vec![0u8; PAGE_SIZE];
        
        Self {
            data,
            heap_pos: HEAP_START,
            row_count: 0,
            page_index,
            page_type,
            row_offsets: Vec::new(),
        }
    }
    
    /// Create an empty data page (all zeros, used for tables with no content)
    pub fn empty_page() -> Vec<u8> {
        vec![0u8; PAGE_SIZE]
    }
    
    /// Calculate how much space is available for new data
    fn available_space(&self) -> usize {
        let num_groups = (self.row_count / ROWS_PER_GROUP) + 1;
        let index_size = num_groups * ROW_GROUP_SIZE;
        let index_start = PAGE_SIZE - index_size;
        
        if self.heap_pos >= index_start {
            0
        } else {
            index_start - self.heap_pos
        }
    }
    
    /// Check if adding data of given size would overflow
    pub fn would_overflow(&self, data_size: usize) -> bool {
        // Account for potential new row group if we're at a boundary
        let new_row_count = self.row_count + 1;
        let num_groups = (new_row_count / ROWS_PER_GROUP) + 1;
        let index_size = num_groups * ROW_GROUP_SIZE;
        let index_start = PAGE_SIZE - index_size;
        
        self.heap_pos + data_size > index_start
    }
    
    /// Write raw bytes to the heap, returns offset relative to HEAP_START
    pub fn write_heap(&mut self, data: &[u8]) -> Result<u16> {
        if self.would_overflow(data.len()) {
            return Err(Error::PageOverflow(format!(
                "Cannot write {} bytes, only {} available",
                data.len(),
                self.available_space()
            )));
        }
        
        let offset = (self.heap_pos - HEAP_START) as u16;
        self.data[self.heap_pos..self.heap_pos + data.len()].copy_from_slice(data);
        self.heap_pos += data.len();
        
        Ok(offset)
    }
    
    /// Add a row to the page
    /// The row data should already be written to the heap
    /// This just records the offset in the row index
    pub fn add_row(&mut self, heap_offset: u16) -> Result<()> {
        self.row_offsets.push(heap_offset);
        self.row_count += 1;
        Ok(())
    }
    
    /// Write row data and add to index in one step
    pub fn write_row(&mut self, data: &[u8]) -> Result<u16> {
        let offset = self.write_heap(data)?;
        self.add_row(offset)?;
        Ok(offset)
    }
    
    /// Get current heap position (for calculating string offsets within a row)
    pub fn heap_position(&self) -> usize {
        self.heap_pos
    }
    
    /// Finalize the page and return the complete data
    pub fn finalize(mut self, next_page: u32) -> Vec<u8> {
        // Write page header
        self.write_header(next_page);
        
        // Write row index (backwards from end)
        self.write_row_index();
        
        self.data
    }
    
    fn write_header(&mut self, next_page: u32) {
        // Page header per Kaitai spec (rekordbox_pdb.ksy)
        // Total header size: 0x28 (40 bytes)
        
        // 0x00-0x03: gap/zeros (already zero)
        
        // 0x04-0x07: page_index
        self.data[0x04..0x08].copy_from_slice(&self.page_index.to_le_bytes());
        
        // 0x08-0x0B: type (page_type)
        self.data[0x08..0x0C].copy_from_slice(&(self.page_type as u32).to_le_bytes());
        
        // 0x0C-0x0F: next_page (0xFFFFFFFF if none)
        self.data[0x0C..0x10].copy_from_slice(&next_page.to_le_bytes());
        
        // 0x10-0x13: sequence/transaction (use 0)
        // Already zero
        
        // 0x14-0x17: unknown (4 bytes, zeros)
        // Already zero
        
        // 0x18: num_rows_small (u8) - row count if < 256
        let num_rows_small = std::cmp::min(self.row_count, 255) as u8;
        self.data[0x18] = num_rows_small;
        
        // 0x19: bitmask (u8) - unknown purpose, use 0
        // Already zero
        
        // 0x1A: unknown (u8)
        // Already zero
        
        // 0x1B: page_flags (u8)
        // Tracks use 0x34, most other tables use 0x24
        self.data[0x1B] = match self.page_type {
            PageType::Tracks => PAGE_FLAGS_DATA_TRACK,
            _ => PAGE_FLAGS_DATA,
        };
        
        // 0x1C-0x1D: free_size (u16)
        let free_size = self.available_space() as u16;
        self.data[0x1C..0x1E].copy_from_slice(&free_size.to_le_bytes());
        
        // 0x1E-0x1F: used_size (u16)
        let used_size = (self.heap_pos - HEAP_START) as u16;
        self.data[0x1E..0x20].copy_from_slice(&used_size.to_le_bytes());
        
        // 0x20-0x21: unknown (u16) - often related to something, use 0
        // Already zero
        
        // 0x22-0x23: num_rows_large (u16) - row count if >= 256
        let num_rows_large = if self.row_count > 255 { self.row_count as u16 } else { 0 };
        self.data[0x22..0x24].copy_from_slice(&num_rows_large.to_le_bytes());
        
        // 0x24-0x25: unknown (u16)
        // Already zero
        
        // 0x26-0x27: unknown (u16)
        // Already zero
        
        // Heap starts at 0x28
    }
    
    fn write_row_index(&mut self) {
        // Row group structure (36 bytes, from rekordcrate):
        // - Bytes 0-31: row_offsets[0..16] (16 × u16, stored in REVERSE order)
        // - Bytes 32-33: presence_flags (u16)
        // - Bytes 34-35: unknown/padding (u16)
        //
        // Row offsets are stored in reverse: row_offsets[15] = offset for row 0 (bit 0)
        //                                    row_offsets[14] = offset for row 1 (bit 1)
        //                                    etc.
        
        // Always write at least one row group, even for empty pages
        let num_groups = if self.row_offsets.is_empty() {
            1
        } else {
            (self.row_offsets.len() + ROWS_PER_GROUP - 1) / ROWS_PER_GROUP
        };
        
        for group_idx in 0..num_groups {
            let group_start = PAGE_SIZE - (group_idx + 1) * ROW_GROUP_SIZE;
            
            let first_row = group_idx * ROWS_PER_GROUP;
            let rows_in_group = if first_row >= self.row_offsets.len() {
                0
            } else {
                std::cmp::min(
                    ROWS_PER_GROUP,
                    self.row_offsets.len() - first_row
                )
            };
            
            // Presence flags: bits 0..(N-1) set for N rows
            let presence_flags: u16 = if rows_in_group > 0 {
                ((1u32 << rows_in_group) - 1) as u16
            } else {
                0
            };
            
            // Write row offsets in REVERSE order
            // row_offsets[15] = offset for row 0 (bit 0)
            // row_offsets[14] = offset for row 1 (bit 1)
            // etc.
            for i in 0..rows_in_group {
                let row_idx = first_row + i;
                // Store in reverse: row i goes to array position (15 - i)
                let array_pos = ROWS_PER_GROUP - 1 - i;
                let offset_pos = group_start + array_pos * 2;
                self.data[offset_pos..offset_pos + 2]
                    .copy_from_slice(&self.row_offsets[row_idx].to_le_bytes());
            }
            
            // Write presence_flags at byte 32
            self.data[group_start + 32..group_start + 34]
                .copy_from_slice(&presence_flags.to_le_bytes());
            
            // Bytes 34-35 are padding (already zero)
        }
    }
    
    /// Get number of rows in this page
    pub fn row_count(&self) -> usize {
        self.row_count
    }
    
    /// Get page index
    pub fn page_index(&self) -> u32 {
        self.page_index
    }
}

/// Table pointer in file header
#[derive(Debug, Clone, Copy, Default)]
pub struct TablePointer {
    pub table_type: u32,
    pub empty_candidate: u32,
    pub first_page: u32,  // Index page
    pub last_page: u32,   // Last data page (or index page if no data)
}

impl TablePointer {
    /// Create a new table pointer
    /// - first_page: the index page number
    /// - last_page: the last data page number (or same as first_page if no data pages)
    /// - empty_candidate: the next available page after this table's pages
    pub fn new(table_type: PageType, first_page: u32, last_page: u32, empty_candidate: u32) -> Self {
        Self {
            table_type: table_type as u32,
            empty_candidate,
            first_page,
            last_page,
        }
    }
    
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&self.table_type.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.empty_candidate.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.first_page.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.last_page.to_le_bytes());
        bytes
    }
}

/// File header builder
pub struct FileHeader {
    pub page_size: u32,
    pub num_tables: u32,
    pub next_unused_page: u32,
    pub unknown1: u32,  // Must be 5 for rekordbox PC compatibility
    pub sequence: u32,
    pub tables: Vec<TablePointer>,
}

impl FileHeader {
    pub fn new() -> Self {
        Self {
            page_size: PAGE_SIZE as u32,
            num_tables: 0,
            next_unused_page: 1,
            unknown1: 5,  // Required for rekordbox PC
            sequence: 1,
            tables: Vec::new(),
        }
    }
    
    pub fn add_table(&mut self, pointer: TablePointer) {
        self.tables.push(pointer);
        self.num_tables = self.tables.len() as u32;
    }
    
    pub fn to_page(&self) -> Vec<u8> {
        let mut page = vec![0u8; PAGE_SIZE];
        
        // Bytes 0-3: zero
        // Bytes 4-7: page_size
        page[4..8].copy_from_slice(&self.page_size.to_le_bytes());
        
        // Bytes 8-11: num_tables
        page[8..12].copy_from_slice(&self.num_tables.to_le_bytes());
        
        // Bytes 12-15: next_unused_page
        page[12..16].copy_from_slice(&self.next_unused_page.to_le_bytes());
        
        // Bytes 16-19: unknown1 (must be 5)
        page[16..20].copy_from_slice(&self.unknown1.to_le_bytes());
        
        // Bytes 20-23: sequence
        page[20..24].copy_from_slice(&self.sequence.to_le_bytes());
        
        // Bytes 24-27: unknown (zero)
        
        // Table pointers start at byte 28
        let mut offset = 28;
        for table in &self.tables {
            page[offset..offset + 16].copy_from_slice(&table.to_bytes());
            offset += 16;
        }
        
        page
    }
}

impl Default for FileHeader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_page_builder_basic() {
        let mut page = PageBuilder::new(1, PageType::Artists);
        
        // Write some test data
        let data = b"test row data";
        let offset = page.write_row(data).unwrap();
        
        assert_eq!(offset, 0);
        assert_eq!(page.row_count(), 1);
    }
    
    #[test]
    fn test_page_overflow_detection() {
        let page = PageBuilder::new(1, PageType::Artists);
        
        // Should not overflow for small data
        assert!(!page.would_overflow(100));
        
        // Should overflow for data larger than page
        assert!(page.would_overflow(PAGE_SIZE));
    }
    
    #[test]
    fn test_row_index_structure() {
        let mut page = PageBuilder::new(1, PageType::Artists);
        
        // Add 3 rows
        for i in 0..3 {
            let data = format!("row{}", i);
            page.write_row(data.as_bytes()).unwrap();
        }
        
        let finalized = page.finalize(0xFFFFFFFF);
        
        // Row group structure (36 bytes from end):
        // - Bytes 0-31: row_offsets[0..16]
        // - Bytes 32-33: presence_flags
        // - Bytes 34-35: padding
        let group_start = PAGE_SIZE - ROW_GROUP_SIZE;
        
        // Check presence flags at byte 32 of the group
        let flags = u16::from_le_bytes([
            finalized[group_start + 32],
            finalized[group_start + 33],
        ]);
        
        // 3 rows = bits 0, 1, 2 set = 0b111 = 7
        assert_eq!(flags, 0x0007);
        
        // Check row offsets are in reverse order
        // row_offsets[15] = row 0, row_offsets[14] = row 1, row_offsets[13] = row 2
        let offset_0 = u16::from_le_bytes([
            finalized[group_start + 30], // position 15 * 2
            finalized[group_start + 31],
        ]);
        assert_eq!(offset_0, 0); // Row 0 at heap offset 0
    }
}
