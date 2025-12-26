//! rekordbox-server: Audio analysis and Pioneer export generation
//!
//! This server runs on the NAS (Dell Wyse 5070) and handles:
//! - Audio file analysis (BPM, waveforms, beat grids)
//! - PDB database generation
//! - ANLZ file generation
//! - Communication with CLI client via Unix socket

mod analyzer;
mod config;
mod export;
mod server;
mod waveform;

use std::path::PathBuf;

use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use rekordbox_core::AnalysisCache;
use config::Config;

#[derive(Parser, Debug)]
#[command(name = "rekordbox-server")]
#[command(about = "Pioneer DJ export server for NAS deployment")]
struct Args {
    /// Music directory to analyze
    #[arg(short, long, default_value = "/mnt/ssd/pre-export")]
    music_dir: PathBuf,
    
    /// Cache directory for analysis results
    #[arg(short, long, default_value = "/var/cache/rekordbox")]
    cache_dir: PathBuf,
    
    /// Unix socket path for IPC
    #[arg(short, long, default_value = "/tmp/rekordbox.sock")]
    socket: PathBuf,
    
    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    foreground: bool,
    
    /// Export directly to path without running server
    #[arg(short, long)]
    export: Option<PathBuf>,
    
    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    // Setup logging
    let level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .compact()
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    
    info!("rekordbox-server starting");
    info!("Music directory: {:?}", args.music_dir);
    info!("Cache directory: {:?}", args.cache_dir);
    
    // Initialize cache
    let cache = AnalysisCache::new(&args.cache_dir)?;
    
    let config = Config {
        music_dir: args.music_dir,
        cache_dir: args.cache_dir,
        output_dir: args.export.clone(),
        socket_path: args.socket,
        max_concurrent: 1, // Single-threaded for memory efficiency
    };
    
    // If --export is specified, run export directly and exit
    if let Some(output_path) = args.export {
        info!("Running direct export to {:?}", output_path);
        
        let tracks = analyzer::analyze_directory(&config, &cache).await?;
        export::export_usb(&tracks, &config.music_dir, &output_path)?;
        
        info!("Export complete");
        return Ok(());
    }
    
    // Otherwise run as server
    server::run(config, cache).await
}
