//! Page allocation for Pioneer DeviceSQL databases
//!
//! Pages are 4096 bytes with:
//! - Fixed header at offset 0x00-0x27
//! - Heap growing forward from offset 0x28
//! - Row index growing backward from page end
//!
//! Row index structure (per 16-row group, from end of page):
//! - 2 bytes padding
//! - 2 bytes presence flags (bitmask of which rows exist)
//! - 16 × 2-byte offsets pointing to row data in heap

use crate::error::{Error, Result};

/// Page size in bytes (always 4096 for Pioneer databases)
pub const PAGE_SIZE: usize = 4096;

/// Offset where heap data begins
pub const HEAP_START: usize = 0x28;

/// Size of each row group in the backward-growing index
/// 2 (padding) + 2 (flags) + 16*2 (offsets) = 36 bytes
pub const ROW_GROUP_SIZE: usize = 36;

/// Maximum rows per group
pub const ROWS_PER_GROUP: usize = 16;

/// Page types (table types)
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
    Artwork = 13,
    Columns = 16,
    HistoryPlaylists = 17,
    HistoryEntries = 18,
    History = 19,
}

/// A single page being built
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
    /// Create a new page
    pub fn new(page_index: u32, page_type: PageType) -> Self {
        let mut data = vec![0u8; PAGE_SIZE];
        
        Self {
            data,
            heap_pos: HEAP_START,
            row_count: 0,
            page_index,
            page_type,
            row_offsets: Vec::new(),
        }
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
        // Bytes 0-3: Always zero
        // Bytes 4-7: page_index
        self.data[4..8].copy_from_slice(&self.page_index.to_le_bytes());
        
        // Bytes 8-11: page_type
        self.data[8..12].copy_from_slice(&(self.page_type as u32).to_le_bytes());
        
        // Bytes 12-15: next_page (0xFFFFFFFF if none)
        self.data[12..16].copy_from_slice(&next_page.to_le_bytes());
        
        // Bytes 16-19: version/unknown (use 1)
        self.data[16..20].copy_from_slice(&1u32.to_le_bytes());
        
        // Bytes 20-23: unknown2 (zero)
        
        // Bytes 24-26: row counts (packed into 3 bytes)
        // First 13 bits: num_row_offsets (total ever allocated)
        // Last 11 bits: num_rows (currently valid)
        let num_row_offsets = self.row_count as u32;
        let num_rows = self.row_count as u32;
        let packed = (num_row_offsets & 0x1FFF) | ((num_rows & 0x7FF) << 13);
        self.data[24] = (packed & 0xFF) as u8;
        self.data[25] = ((packed >> 8) & 0xFF) as u8;
        self.data[26] = ((packed >> 16) & 0xFF) as u8;
        
        // Byte 27: page_flags (0x34 for normal data page)
        self.data[27] = 0x34;
        
        // Bytes 28-29: free_size
        let free_size = self.available_space() as u16;
        self.data[28..30].copy_from_slice(&free_size.to_le_bytes());
        
        // Bytes 30-31: used_size
        let used_size = (self.heap_pos - HEAP_START) as u16;
        self.data[30..32].copy_from_slice(&used_size.to_le_bytes());
        
        // Bytes 32-33: u5 (usually 0x4A48 or similar, use 0)
        // Bytes 34-35: unkrows (related to row count, use 0)
        // Bytes 36-37: u6 (0 for data pages)
        // Bytes 38-39: u7 (0 for data pages)
    }
    
    fn write_row_index(&mut self) {
        if self.row_offsets.is_empty() {
            return;
        }
        
        let num_groups = (self.row_offsets.len() + ROWS_PER_GROUP - 1) / ROWS_PER_GROUP;
        
        for group_idx in 0..num_groups {
            let group_start = PAGE_SIZE - (group_idx + 1) * ROW_GROUP_SIZE;
            
            // Padding (2 bytes) - already zero
            
            // Presence flags (2 bytes)
            let first_row = group_idx * ROWS_PER_GROUP;
            let rows_in_group = std::cmp::min(
                ROWS_PER_GROUP,
                self.row_offsets.len() - first_row
            );
            let presence_flags: u16 = (1u16 << rows_in_group) - 1;
            self.data[group_start + 2..group_start + 4]
                .copy_from_slice(&presence_flags.to_le_bytes());
            
            // Row offsets (16 × 2 bytes)
            for i in 0..rows_in_group {
                let row_idx = first_row + i;
                let offset_pos = group_start + 4 + (i * 2);
                self.data[offset_pos..offset_pos + 2]
                    .copy_from_slice(&self.row_offsets[row_idx].to_le_bytes());
            }
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
    pub first_page: u32,
    pub last_page: u32,
}

impl TablePointer {
    pub fn new(table_type: PageType, first_page: u32, last_page: u32) -> Self {
        Self {
            table_type: table_type as u32,
            empty_candidate: 0,
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
    pub sequence: u32,
    pub tables: Vec<TablePointer>,
}

impl FileHeader {
    pub fn new() -> Self {
        Self {
            page_size: PAGE_SIZE as u32,
            num_tables: 0,
            next_unused_page: 1,
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
        
        // Bytes 16-19: unknown (zero)
        
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
        
        // Check presence flags at end of page
        let group_start = PAGE_SIZE - ROW_GROUP_SIZE;
        let flags = u16::from_le_bytes([
            finalized[group_start + 2],
            finalized[group_start + 3],
        ]);
        
        // 3 rows = bits 0, 1, 2 set = 0b111 = 7
        assert_eq!(flags, 0x0007);
    }
}
