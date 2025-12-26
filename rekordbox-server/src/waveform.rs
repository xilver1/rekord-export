//! Waveform generation for Pioneer displays
//!
//! Generates both preview (PWAV) and detail (PWV5) waveforms using FFT
//! for frequency band separation (bass/mid/high → red/green/blue).

use rustfft::{FftPlanner, num_complex::Complex};
use rekordbox_core::{Waveform, WaveformPreview, WaveformDetail, WaveformColumn, WaveformColorEntry};

/// Waveform generator with FFT support
pub struct WaveformGenerator {
    sample_rate: u32,
}

impl WaveformGenerator {
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate }
    }
    
    /// Generate both preview and detail waveforms
    pub fn generate(&self, samples: &[f32], duration_secs: f64) -> Waveform {
        let preview = self.generate_preview(samples);
        let detail = self.generate_detail(samples, duration_secs);
        
        Waveform { preview, detail }
    }
    
    /// Generate 400-column preview waveform (PWAV format)
    fn generate_preview(&self, samples: &[f32]) -> WaveformPreview {
        let mut columns = Vec::with_capacity(400);
        
        if samples.is_empty() {
            return WaveformPreview {
                columns: vec![WaveformColumn { height: 0, whiteness: 0 }; 400],
            };
        }
        
        // Divide samples into 400 segments
        let segment_size = samples.len() / 400;
        if segment_size == 0 {
            return WaveformPreview {
                columns: vec![WaveformColumn { height: 0, whiteness: 0 }; 400],
            };
        }
        
        for i in 0..400 {
            let start = i * segment_size;
            let end = std::cmp::min(start + segment_size, samples.len());
            let segment = &samples[start..end];
            
            if segment.is_empty() {
                columns.push(WaveformColumn { height: 0, whiteness: 0 });
                continue;
            }
            
            // Calculate RMS amplitude
            let rms: f32 = (segment.iter().map(|s| s * s).sum::<f32>() 
                           / segment.len() as f32).sqrt();
            
            // Calculate peak for "whiteness" (loudness variation)
            let peak: f32 = segment.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
            
            // Scale to 0-31 range for height (boost for visibility)
            let height = (rms * 31.0 * 4.0).min(31.0) as u8;
            
            // Whiteness based on peak-to-RMS ratio (crest factor)
            let crest = if rms > 0.001 { peak / rms } else { 1.0 };
            let whiteness = ((crest - 1.0) / 2.0).clamp(0.0, 7.0) as u8;
            
            columns.push(WaveformColumn { height, whiteness });
        }
        
        WaveformPreview { columns }
    }
    
    /// Generate detail color waveform (PWV5 format, 150 entries/second)
    fn generate_detail(&self, samples: &[f32], duration_secs: f64) -> WaveformDetail {
        // 150 entries per second
        let num_entries = (duration_secs * 150.0).ceil() as usize;
        let num_entries = num_entries.max(1);
        let mut entries = Vec::with_capacity(num_entries);
        
        if samples.is_empty() {
            return WaveformDetail {
                entries: vec![WaveformColorEntry::default(); num_entries],
            };
        }
        
        // FFT setup
        let fft_size = 1024;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);
        
        // Samples per waveform entry
        let samples_per_entry = self.sample_rate as usize / 150;
        if samples_per_entry == 0 {
            return WaveformDetail {
                entries: vec![WaveformColorEntry::default(); num_entries],
            };
        }
        
        // Hann window
        let window: Vec<f32> = (0..fft_size)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos()))
            .collect();
        
        // Frequency bin ranges for each color
        let bin_hz = self.sample_rate as f32 / fft_size as f32;
        let bass_start = (20.0 / bin_hz).ceil() as usize;
        let bass_end = (200.0 / bin_hz) as usize;
        let mid_end = (4000.0 / bin_hz) as usize;
        let high_end = std::cmp::min((20000.0 / bin_hz) as usize, fft_size / 2);
        
        for entry_idx in 0..num_entries {
            let sample_start = entry_idx * samples_per_entry;
            
            if sample_start >= samples.len() {
                entries.push(WaveformColorEntry::default());
                continue;
            }
            
            // Get FFT window of samples
            let mut fft_buffer: Vec<Complex<f32>> = (0..fft_size)
                .map(|i| {
                    let sample_idx = sample_start + i;
                    let sample = if sample_idx < samples.len() {
                        samples[sample_idx]
                    } else {
                        0.0
                    };
                    Complex::new(sample * window[i], 0.0)
                })
                .collect();
            
            // Run FFT
            fft.process(&mut fft_buffer);
            
            // Calculate magnitude for each frequency band
            let bass_range = bass_start.max(1)..=bass_end.min(fft_size / 2);
            let mid_range = (bass_end + 1)..=mid_end.min(fft_size / 2);
            let high_range = (mid_end + 1)..=high_end.min(fft_size / 2);
            
            let bass_energy: f32 = if bass_range.is_empty() { 0.0 } else {
                fft_buffer[bass_range.clone()]
                    .iter()
                    .map(|c| c.norm())
                    .sum::<f32>() / (bass_range.end() - bass_range.start() + 1) as f32
            };
            
            let mid_energy: f32 = if mid_range.is_empty() { 0.0 } else {
                fft_buffer[mid_range.clone()]
                    .iter()
                    .map(|c| c.norm())
                    .sum::<f32>() / (mid_range.end() - mid_range.start() + 1) as f32
            };
            
            let high_energy: f32 = if high_range.is_empty() { 0.0 } else {
                fft_buffer[high_range.clone()]
                    .iter()
                    .map(|c| c.norm())
                    .sum::<f32>() / (high_range.end() - high_range.start() + 1) as f32
            };
            
            // Calculate overall amplitude for height
            let segment_end = std::cmp::min(sample_start + samples_per_entry, samples.len());
            let amplitude = if sample_start < segment_end {
                let segment = &samples[sample_start..segment_end];
                (segment.iter().map(|s| s * s).sum::<f32>() / segment.len() as f32).sqrt()
            } else {
                0.0
            };
            
            // Scale to 0-7 range for colors (3 bits each)
            let boost = 8.0;
            let red = (bass_energy * boost).clamp(0.0, 7.0) as u8;
            let green = (mid_energy * boost * 2.0).clamp(0.0, 7.0) as u8;
            let blue = (high_energy * boost * 4.0).clamp(0.0, 7.0) as u8;
            
            // Height 0-31
            let height = (amplitude * 31.0 * 4.0).clamp(0.0, 31.0) as u8;
            
            entries.push(WaveformColorEntry { red, green, blue, height });
        }
        
        WaveformDetail { entries }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_preview_generation() {
        let gen = WaveformGenerator::new(44100);
        
        // Generate 1 second of sine wave
        let samples: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        
        let preview = gen.generate_preview(&samples);
        
        assert_eq!(preview.columns.len(), 400);
        // All columns should have some amplitude
        assert!(preview.columns.iter().any(|c| c.height > 0));
    }
    
    #[test]
    fn test_detail_generation() {
        let gen = WaveformGenerator::new(44100);
        
        // Generate 1 second of sine wave
        let samples: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();
        
        let detail = gen.generate_detail(&samples, 1.0);
        
        // 1 second at 150 entries/sec = 150 entries
        assert_eq!(detail.entries.len(), 150);
    }
    
    #[test]
    fn test_empty_samples() {
        let gen = WaveformGenerator::new(44100);
        let waveform = gen.generate(&[], 0.0);
        
        assert_eq!(waveform.preview.columns.len(), 400);
        assert!(waveform.detail.entries.len() >= 1);
    }
}
