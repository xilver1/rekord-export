//! rekordbox-core: Pioneer DJ format structures with write support
//!
//! This crate provides binary serialization for:
//! - export.pdb (DeviceSQL database) - little-endian
//! - ANLZ files (.DAT, .EXT, .2EX) - big-endian
//!
//! Based on Deep Symmetry's reverse engineering and rekordcrate's structures.

pub mod pdb;
pub mod anlz;
pub mod track;
pub mod cache;
pub mod error;

pub use error::{Error, Result};
pub use track::{TrackAnalysis, BeatGrid, Waveform, Key};
