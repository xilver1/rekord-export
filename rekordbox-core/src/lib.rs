//! rekordbox-core: Pioneer DJ format structures with write support
//!
//! This crate provides binary serialization for:
//! - export.pdb (DeviceSQL database) - little-endian
//! - ANLZ files (.DAT, .EXT) - big-endian
//!
//! Based on Deep Symmetry's reverse engineering documentation:
//! https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/

pub mod error;
pub mod string;
pub mod page;
pub mod pdb;
pub mod anlz;
pub mod track;
pub mod cache;
pub mod validate;

// Re-exports for convenience
pub use error::{Error, Result};
pub use track::{TrackAnalysis, BeatGrid, Beat, Waveform, WaveformPreview, WaveformDetail, 
                WaveformColumn, WaveformColorEntry, Key, FileType};
pub use pdb::PdbBuilder;
pub use anlz::{generate_dat_file, generate_ext_file, generate_anlz_path};
pub use cache::{AnalysisCache, CacheStats, compute_file_hash};
pub use validate::{validate_pdb, validate_and_print, ValidationResult, PdbStats};
