//! Lightweight CLI client for rekordbox-server over unix socket

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "rbx")]
#[command(about = "Rekordbox USB Export CLI")]
#[command(version)]
struct Cli {
    /// Server socket path
    #[arg(short, long, default_value = "/tmp/rekordbox.sock")]
    socket: PathBuf,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Analyze {
        /// Uses server default if not specified
        #[arg(short, long)]
        path: Option<String>,
    },
    
    Export {
        /// Output directory (USB mount point)
        output: String,
    },
    
    Status,
    List,
    Cache {
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Serialize)]
struct Request {
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
}

#[derive(Deserialize)]
struct Response {
    success: bool,
    message: Option<String>,
    data: Option<serde_json::Value>,
}

fn main() {
    let cli = Cli::parse();
    
    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let request = match cli.command {
        Commands::Analyze { path } => Request {
            method: "analyze".into(),
            path,
            output: None,
        },
        Commands::Export { output } => Request {
            method: "export".into(),
            path: None,
            output: Some(output),
        },
        Commands::Status => Request {
            method: "status".into(),
            path: None,
            output: None,
        },
        Commands::List => Request {
            method: "list_tracks".into(),
            path: None,
            output: None,
        },
        Commands::Cache { clear } => Request {
            method: if clear { "cache_clear" } else { "cache_stats" }.into(),
            path: None,
            output: None,
        },
    };
    
    let mut stream = UnixStream::connect(&cli.socket)
        .map_err(|e| format!("Cannot connect to server at {:?}: {}", cli.socket, e))?;
    
    let request_json = serde_json::to_string(&request)?;
    writeln!(stream, "{}", request_json)?;
    stream.flush()?;
    
    let mut reader = BufReader::new(&stream);
    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;
    
    let response: Response = serde_json::from_str(&response_line)?;
    
    if response.success {
        if let Some(msg) = response.message {
            println!("✓ {}", msg);
        }
        
        if let Some(data) = response.data {
            print_data(&data, &request.method);
        }
    } else {
        if let Some(msg) = response.message {
            eprintln!("✗ {}", msg);
        }
        std::process::exit(1);
    }
    
    Ok(())
}

fn print_data(data: &serde_json::Value, method: &str) {
    match method {
        "list_tracks" | "analyze" => {
            if let Some(tracks) = data.as_array().or_else(|| data.get("tracks").and_then(|t| t.as_array())) {
                println!("\nTracks:");
                for track in tracks {
                    let id = track.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                    let title = track.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                    let artist = track.get("artist").and_then(|v| v.as_str()).unwrap_or("?");
                    let bpm = track.get("bpm").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let key = track.get("key").and_then(|v| v.as_str()).unwrap_or("-");
                    
                    println!("  {:3}. {} - {} [{:.0} BPM, {}]", id, artist, title, bpm, key);
                }
            }
        }
        "cache_stats" => {
            let entries = data.get("entries").and_then(|v| v.as_u64()).unwrap_or(0);
            let size_mb = data.get("size_mb").and_then(|v| v.as_f64()).unwrap_or(0.0);
            println!("  Entries: {}", entries);
            println!("  Size: {:.2} MB", size_mb);
        }
        _ => {
            // Pretty print JSON for unknown methods
            println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
        }
    }
}
