//! Analysis cache using filesystem storage
//!
//! Stores analysis results on disk (SSD) keyed by file hash.
//! This is critical for memory-constrained environments like the Dell Wyse.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use crate::track::TrackAnalysis;
use crate::error::{Error, Result};

/// File-based cache for track analysis results
pub struct AnalysisCache {
    cache_dir: PathBuf,
}

impl AnalysisCache {
    /// Create a new cache at the given directory
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }
    
    /// Generate a cache key from file path and content hash
    fn cache_key(file_hash: u64) -> String {
        format!("{:016x}.json", file_hash)
    }
    
    /// Get cached analysis if it exists and is valid
    pub fn get(&self, file_hash: u64) -> Option<TrackAnalysis> {
        let key = Self::cache_key(file_hash);
        let path = self.cache_dir.join(&key);
        
        if !path.exists() {
            return None;
        }
        
        let file = File::open(&path).ok()?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).ok()
    }
    
    /// Store analysis result in cache
    pub fn put(&self, analysis: &TrackAnalysis) -> Result<()> {
        let key = Self::cache_key(analysis.file_hash);
        let path = self.cache_dir.join(&key);
        
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, analysis)
            .map_err(|e| Error::Cache(e.to_string()))?;
        
        Ok(())
    }
    
    /// Remove cached analysis
    pub fn invalidate(&self, file_hash: u64) -> Result<()> {
        let key = Self::cache_key(file_hash);
        let path = self.cache_dir.join(&key);
        
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
    
    /// Clear entire cache
    pub fn clear(&self) -> Result<()> {
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> Result<CacheStats> {
        let mut count = 0;
        let mut total_size = 0;
        
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                count += 1;
                total_size += entry.metadata()?.len();
            }
        }
        
        Ok(CacheStats {
            entry_count: count,
            total_size_bytes: total_size,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entry_count: usize,
    pub total_size_bytes: u64,
}

/// Compute file hash for cache invalidation
/// Uses XXH3 on a sample of the file (first 1MB + size) for speed
pub fn compute_file_hash<P: AsRef<Path>>(path: P) -> Result<u64> {
    let metadata = fs::metadata(&path)?;
    let file_size = metadata.len();
    
    // Read first 1MB (or entire file if smaller)
    let sample_size = std::cmp::min(file_size as usize, 1024 * 1024);
    let mut sample = vec![0u8; sample_size + 8];
    
    let mut file = File::open(&path)?;
    std::io::Read::read_exact(&mut file, &mut sample[..sample_size])?;
    
    // Append file size to sample for uniqueness
    sample[sample_size..].copy_from_slice(&file_size.to_le_bytes());
    
    Ok(xxh3_64(&sample))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_cache_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let cache = AnalysisCache::new(tmp.path()).unwrap();
        
        // Create a minimal analysis
        use crate::track::*;
        let analysis = TrackAnalysis {
            id: 1,
            file_path: "test.mp3".into(),
            title: "Test Track".into(),
            artist: "Test Artist".into(),
            album: None,
            genre: None,
            duration_secs: 180.0,
            sample_rate: 44100,
            bit_depth: 16,
            bpm: 128.0,
            key: None,
            beat_grid: BeatGrid {
                bpm: 128.0,
                first_beat_ms: 0.0,
                beats: vec![],
            },
            waveform: Waveform {
                preview: WaveformPreview { columns: vec![] },
                detail: WaveformDetail { entries: vec![] },
            },
            file_size: 5_000_000,
            file_hash: 0x12345678ABCDEF00,
        };
        
        // Store and retrieve
        cache.put(&analysis).unwrap();
        let retrieved = cache.get(analysis.file_hash).unwrap();
        
        assert_eq!(retrieved.id, analysis.id);
        assert_eq!(retrieved.title, analysis.title);
    }
}
