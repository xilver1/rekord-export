//! Server configuration

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    /// Root music directory (pre-export folder)
    pub music_dir: PathBuf,
    /// Cache directory for analysis results
    pub cache_dir: PathBuf,
    /// Output directory for USB export
    pub output_dir: Option<PathBuf>,
    /// Unix socket path for IPC
    pub socket_path: PathBuf,
    /// Max concurrent analysis tasks
    pub max_concurrent: usize,
}
