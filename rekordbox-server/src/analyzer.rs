//! Audio analysis pipeline
//!
//! Memory-efficient audio processing using Symphonia for decoding.
//! Processes audio in chunks to minimize memory usage on the Dell Wyse.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs::File;

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tracing::{info, warn, debug};
use walkdir::WalkDir;

use rekordbox_core::cache::{AnalysisCache, compute_file_hash};
use rekordbox_core::track::*;
use crate::config::Config;
use crate::waveform::WaveformGenerator;

/// Analyze all audio files in a directory
pub async fn analyze_directory(
    config: &Config,
    cache: &AnalysisCache,
) -> anyhow::Result<Vec<TrackAnalysis>> {
    let mut results = Vec::new();
    let mut playlists: HashMap<String, Vec<u32>> = HashMap::new();
    let mut track_id = 1u32;
    
    // Scan music directory
    for entry in WalkDir::new(&config.music_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        
        // Check if audio file
        if !is_audio_file(path) {
            continue;
        }
        
        // Get playlist name from parent directory
        let playlist_name = path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("default")
            .to_string();
        
        // Compute file hash for cache lookup
        let file_hash = compute_file_hash(path)?;
        
        // Check cache first
        if let Some(mut cached) = cache.get(file_hash) {
            debug!("Cache hit for {:?}", path);
            cached.id = track_id;
            
            playlists.entry(playlist_name).or_default().push(track_id);
            results.push(cached);
            track_id += 1;
            continue;
        }
        
        info!("Analyzing: {:?}", path);
        
        // Analyze track
        match analyze_track(path, track_id, file_hash).await {
            Ok(analysis) => {
                // Cache the result
                if let Err(e) = cache.put(&analysis) {
                    warn!("Failed to cache analysis: {}", e);
                }
                
                playlists.entry(playlist_name).or_default().push(track_id);
                results.push(analysis);
                track_id += 1;
            }
            Err(e) => {
                warn!("Failed to analyze {:?}: {}", path, e);
            }
        }
    }
    
    info!("Found {} playlists with {} total tracks", playlists.len(), results.len());
    
    Ok(results)
}

/// Analyze a single audio track
async fn analyze_track(
    path: &Path,
    track_id: u32,
    file_hash: u64,
) -> anyhow::Result<TrackAnalysis> {
    // Open audio file
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    
    // Probe format
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    
    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    
    let mut format = probed.format;
    
    // Get track info
    let track = format.default_track()
        .ok_or_else(|| anyhow::anyhow!("No default track"))?;
    
    let sample_rate = track.codec_params.sample_rate
        .ok_or_else(|| anyhow::anyhow!("Unknown sample rate"))?;
    let channels = track.codec_params.channels
        .map(|c| c.count())
        .unwrap_or(2);
    let bit_depth = track.codec_params.bits_per_sample.unwrap_or(16);
    
    // Create decoder
    let mut decoder = symphonia::default::get_codecs().make(
        &track.codec_params,
        &DecoderOptions::default(),
    )?;
    
    // Extract metadata
    let (title, artist, album, genre) = extract_metadata(&mut format, path);
    
    // Collect samples for analysis (downsample to mono float)
    // Process in chunks to limit memory usage
    let mut samples: Vec<f32> = Vec::new();
    let mut total_samples = 0u64;
    
    // Memory limit: ~50MB of samples (12.5M samples at 4 bytes each)
    // At 44.1kHz, that's ~4.7 minutes of audio for BPM detection
    // For longer tracks, we'll analyze a representative section
    const MAX_SAMPLES: usize = 12_500_000;
    
    while samples.len() < MAX_SAMPLES {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e)) 
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        
        let decoded = decoder.decode(&packet)?;
        total_samples += decoded.frames() as u64;
        
        // Convert to mono f32
        append_as_mono_f32(&decoded, &mut samples);
    }
    
    let duration_secs = total_samples as f64 / sample_rate as f64;
    debug!("Decoded {} samples, duration: {:.1}s", total_samples, duration_secs);
    
    // BPM detection
    let bpm = detect_bpm(&samples, sample_rate)?;
    info!("Detected BPM: {:.1}", bpm);
    
    // Key detection (placeholder - needs implementation)
    let key = detect_key(&samples, sample_rate);
    
    // Generate beat grid
    let first_beat_ms = detect_first_beat(&samples, sample_rate, bpm);
    let beat_grid = BeatGrid::constant_tempo(bpm, first_beat_ms, duration_secs * 1000.0);
    
    // Generate waveforms
    let waveform_gen = WaveformGenerator::new(sample_rate);
    let waveform = waveform_gen.generate(&samples, duration_secs);
    
    // Build relative file path for database
    let file_path = path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| format!("Contents/{}", n))
        .unwrap_or_default();
    
    let file_size = std::fs::metadata(path)?.len();
    
    Ok(TrackAnalysis {
        id: track_id,
        file_path,
        title,
        artist,
        album,
        genre,
        duration_secs,
        sample_rate,
        bit_depth,
        bpm,
        key,
        beat_grid,
        waveform,
        file_size,
        file_hash,
    })
}

/// Convert decoded audio to mono f32, appending to buffer
fn append_as_mono_f32(buffer: &AudioBufferRef, output: &mut Vec<f32>) {
    match buffer {
        AudioBufferRef::F32(buf) => {
            let channels = buf.spec().channels.count();
            for frame in 0..buf.frames() {
                let mut sum = 0.0f32;
                for ch in 0..channels {
                    sum += buf.chan(ch)[frame];
                }
                output.push(sum / channels as f32);
            }
        }
        AudioBufferRef::S16(buf) => {
            let channels = buf.spec().channels.count();
            for frame in 0..buf.frames() {
                let mut sum = 0.0f32;
                for ch in 0..channels {
                    sum += buf.chan(ch)[frame] as f32 / 32768.0;
                }
                output.push(sum / channels as f32);
            }
        }
        AudioBufferRef::S32(buf) => {
            let channels = buf.spec().channels.count();
            for frame in 0..buf.frames() {
                let mut sum = 0.0f32;
                for ch in 0..channels {
                    sum += buf.chan(ch)[frame] as f32 / 2147483648.0;
                }
                output.push(sum / channels as f32);
            }
        }
        _ => {
            // Handle other formats by iterating
            debug!("Unsupported sample format, skipping");
        }
    }
}

/// Detect BPM using autocorrelation
/// This is a simplified implementation - consider stratum-dsp for production
fn detect_bpm(samples: &[f32], sample_rate: u32) -> anyhow::Result<f64> {
    // Use first ~30 seconds for BPM detection
    let analysis_samples = std::cmp::min(samples.len(), (sample_rate * 30) as usize);
    let samples = &samples[..analysis_samples];
    
    // Simple onset detection via envelope following
    let hop_size = sample_rate as usize / 100; // 10ms hops
    let mut envelope = Vec::new();
    
    for chunk in samples.chunks(hop_size) {
        let rms: f32 = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        envelope.push(rms);
    }
    
    // Normalize envelope
    let max_env = envelope.iter().cloned().fold(0.0f32, f32::max);
    if max_env > 0.0 {
        for e in &mut envelope {
            *e /= max_env;
        }
    }
    
    // Autocorrelation for tempo detection
    // Search BPM range 60-200
    let env_rate = 100.0; // Envelope sample rate (10ms = 100Hz)
    let min_lag = (env_rate * 60.0 / 200.0) as usize; // 200 BPM
    let max_lag = (env_rate * 60.0 / 60.0) as usize;  // 60 BPM
    
    let mut best_bpm = 120.0;
    let mut best_correlation = 0.0f32;
    
    for lag in min_lag..=max_lag {
        let mut correlation = 0.0f32;
        let count = envelope.len() - lag;
        
        for i in 0..count {
            correlation += envelope[i] * envelope[i + lag];
        }
        correlation /= count as f32;
        
        if correlation > best_correlation {
            best_correlation = correlation;
            let bpm = env_rate * 60.0 / lag as f64;
            best_bpm = bpm;
        }
    }
    
    // Round to common BPM values
    let rounded = (best_bpm * 2.0).round() / 2.0; // Round to 0.5 BPM
    
    Ok(rounded)
}

/// Detect musical key (placeholder - needs full implementation)
fn detect_key(_samples: &[f32], _sample_rate: u32) -> Option<Key> {
    // TODO: Implement chromagram-based key detection
    // For now, return None
    None
}

/// Find first beat position in milliseconds
fn detect_first_beat(samples: &[f32], sample_rate: u32, bpm: f64) -> f64 {
    // Look for first significant onset in first few seconds
    let search_samples = std::cmp::min(samples.len(), (sample_rate * 5) as usize);
    let hop_size = sample_rate as usize / 200; // 5ms hops
    
    let mut onset_strength = Vec::new();
    let mut prev_energy = 0.0f32;
    
    for chunk in samples[..search_samples].chunks(hop_size) {
        let energy: f32 = chunk.iter().map(|s| s * s).sum();
        let onset = (energy - prev_energy).max(0.0);
        onset_strength.push(onset);
        prev_energy = energy;
    }
    
    // Find first strong onset
    let threshold = onset_strength.iter().cloned().fold(0.0f32, f32::max) * 0.3;
    
    for (i, &strength) in onset_strength.iter().enumerate() {
        if strength > threshold {
            let sample_pos = i * hop_size;
            return sample_pos as f64 / sample_rate as f64 * 1000.0;
        }
    }
    
    // Default: beat at start
    0.0
}

/// Extract metadata from audio file
fn extract_metadata(
    format: &mut Box<dyn symphonia::core::formats::FormatReader>,
    path: &Path,
) -> (String, String, Option<String>, Option<String>) {
    let mut title = path.file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let mut artist = "Unknown Artist".to_string();
    let mut album = None;
    let mut genre = None;
    
    // Try to get metadata from format
    if let Some(metadata) = format.metadata().current() {
        for tag in metadata.tags() {
            match tag.std_key {
                Some(symphonia::core::meta::StandardTagKey::TrackTitle) => {
                    title = tag.value.to_string();
                }
                Some(symphonia::core::meta::StandardTagKey::Artist) => {
                    artist = tag.value.to_string();
                }
                Some(symphonia::core::meta::StandardTagKey::Album) => {
                    album = Some(tag.value.to_string());
                }
                Some(symphonia::core::meta::StandardTagKey::Genre) => {
                    genre = Some(tag.value.to_string());
                }
                _ => {}
            }
        }
    }
    
    (title, artist, album, genre)
}

/// Check if path is a supported audio file
fn is_audio_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    
    matches!(ext.as_deref(), Some("mp3" | "flac" | "wav" | "aiff" | "aif" | "m4a" | "aac"))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_audio_file() {
        assert!(is_audio_file(Path::new("test.mp3")));
        assert!(is_audio_file(Path::new("TEST.FLAC")));
        assert!(!is_audio_file(Path::new("test.txt")));
        assert!(!is_audio_file(Path::new("test")));
    }
}
