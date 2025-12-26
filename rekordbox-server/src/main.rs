//! Rekordbox USB Export Server
//!
//! Runs as a service on the NAS, analyzing audio files and generating
//! Pioneer-compatible USB exports.

mod analyzer;
mod config;
mod export;
mod server;
mod waveform;

use std::path::PathBuf;
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use config::Config;

#[derive(Parser)]
#[command(name = "rekordbox-server")]
#[command(about = "Analyze music and export to Pioneer USB format")]
struct Cli {
    /// Path to music directory (pre-export folder)
    #[arg(short, long, default_value = "/mnt/music/pre-export")]
    music_dir: PathBuf,
    
    /// Path to cache directory (on SSD)
    #[arg(short, long, default_value = "/mnt/ssd/rekordbox-cache")]
    cache_dir: PathBuf,
    
    /// Output directory for USB export
    #[arg(short, long)]
    output_dir: Option<PathBuf>,
    
    /// Server socket path for CLI communication
    #[arg(long, default_value = "/tmp/rekordbox.sock")]
    socket: PathBuf,
    
    /// Run analysis only (don't start server)
    #[arg(long)]
    analyze_only: bool,
    
    /// Export to USB immediately after analysis
    #[arg(long)]
    export: bool,
    
    /// Maximum concurrent analysis tasks (memory-aware)
    #[arg(long, default_value = "2")]
    max_concurrent: usize,
    
    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main(flavor = "current_thread")] // Single-threaded for memory efficiency
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    // Setup logging
    let level = if cli.verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .compact()
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    
    info!("Rekordbox Export Server starting");
    info!("Music directory: {:?}", cli.music_dir);
    info!("Cache directory: {:?}", cli.cache_dir);
    
    let config = Config {
        music_dir: cli.music_dir,
        cache_dir: cli.cache_dir,
        output_dir: cli.output_dir,
        socket_path: cli.socket,
        max_concurrent: cli.max_concurrent,
    };
    
    // Initialize cache
    let cache = rekordbox_core::cache::AnalysisCache::new(&config.cache_dir)?;
    info!("Cache initialized: {:?}", cache.stats()?);
    
    if cli.analyze_only {
        // One-shot analysis
        let results = analyzer::analyze_directory(&config, &cache).await?;
        info!("Analyzed {} tracks", results.len());
        
        if cli.export {
            if let Some(output_dir) = &config.output_dir {
                export::export_usb(&results, output_dir)?;
                info!("Export complete: {:?}", output_dir);
            } else {
                anyhow::bail!("--export requires --output-dir");
            }
        }
    } else {
        // Run as server
        server::run(config, cache).await?;
    }
    
    Ok(())
}
