//! Audio DSP processing module (AEC, NS, AGC) complying with WPIP-03.
//!
//! Provides Acoustic Echo Cancellation (AEC), Noise Suppression (NS),
//! and Automatic Gain Control (AGC) for real-time audio streams.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Audio Digital Signal Processor (DSP) implementing AEC, NS, and AGC.
pub struct AudioDspProcessor {
    enable_aec: bool,
    enable_ns: bool,
    enable_agc: bool,
    // AEC state: Normalized Least Mean Squares (NLMS) filter weights
    aec_filter_weights: Vec<f32>,
    aec_far_end_history: VecDeque<f32>,
    aec_step_size: f32,
    // NS state: Exponential moving average noise floor estimation
    ns_noise_power: f32,
    ns_smoothing_factor: f32,
    // AGC state: Smooth gain multiplier and target amplitude
    agc_gain: f32,
    agc_target_level: f32,
    agc_max_gain: f32,
    agc_attack_rate: f32,
    agc_release_rate: f32,
}

impl AudioDspProcessor {
    /// Create a new AudioDspProcessor with default settings.
    pub fn new(enable_aec: bool, enable_ns: bool, enable_agc: bool) -> Self {
        let filter_size = 64; // Filter tap size for NLMS AEC
        Self {
            enable_aec,
            enable_ns,
            enable_agc,
            aec_filter_weights: vec![0.0; filter_size],
            aec_far_end_history: VecDeque::from(vec![0.0; filter_size]),
            aec_step_size: 0.1,
            ns_noise_power: 0.001,
            ns_smoothing_factor: 0.95,
            agc_gain: 1.0,
            agc_target_level: 0.25, // Target peak level (-12 dBFS equivalent)
            agc_max_gain: 4.0,
            agc_attack_rate: 0.05,
            agc_release_rate: 0.005,
        }
    }

    /// Record speaker output reference samples for AEC echo estimation.
    pub fn record_speaker_reference(&mut self, samples: &[f32]) {
        if !self.enable_aec {
            return;
        }
        for &sample in samples {
            self.aec_far_end_history.push_front(sample);
            if self.aec_far_end_history.len() > self.aec_filter_weights.len() {
                self.aec_far_end_history.pop_back();
            }
        }
    }

    /// Process microphone input samples applying AEC, NS, and AGC in pipeline.
    pub fn process_input(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let mut processed = *sample;

            // 1. Acoustic Echo Cancellation (AEC) via NLMS adaptive filtering
            if self.enable_aec {
                processed = self.apply_aec(processed);
            }

            // 2. Noise Suppression (NS) via spectral noise floor thresholding
            if self.enable_ns {
                processed = self.apply_ns(processed);
            }

            // 3. Automatic Gain Control (AGC) via dynamic amplitude normalization
            if self.enable_agc {
                processed = self.apply_agc(processed);
            }

            *sample = processed;
        }
    }

    /// Apply NLMS Acoustic Echo Cancellation.
    fn apply_aec(&mut self, mic_sample: f32) -> f32 {
        let filter_size = self.aec_filter_weights.len();
        if self.aec_far_end_history.len() < filter_size {
            return mic_sample;
        }

        // Compute estimated echo: y = w^T * x
        let mut estimated_echo = 0.0f32;
        for i in 0..filter_size {
            estimated_echo += self.aec_filter_weights[i] * self.aec_far_end_history[i];
        }

        // Echo-canceled error signal: e = d - y
        let error = mic_sample - estimated_echo;

        // NLMS Weight Update: w_{k+1} = w_k + mu * e * x / (||x||^2 + eps)
        let mut energy = 1e-6f32;
        for i in 0..filter_size {
            let x = self.aec_far_end_history[i];
            energy += x * x;
        }

        let norm_step = self.aec_step_size / energy;
        for i in 0..filter_size {
            self.aec_filter_weights[i] += norm_step * error * self.aec_far_end_history[i];
        }

        error
    }

    /// Apply stationary noise suppression via noise power tracking.
    fn apply_ns(&mut self, sample: f32) -> f32 {
        let current_power = sample * sample;

        // Track minimum stationary noise floor using exponential moving average
        if current_power < self.ns_noise_power {
            self.ns_noise_power = current_power;
        } else {
            self.ns_noise_power = self.ns_smoothing_factor * self.ns_noise_power
                + (1.0 - self.ns_smoothing_factor) * current_power;
        }

        let noise_std = self.ns_noise_power.sqrt();
        let noise_threshold = noise_std * 1.5;

        // Soft spectral noise gate
        let mag = sample.abs();
        if mag < noise_threshold {
            let gain = (mag / (noise_threshold + 1e-6)).powi(2);
            sample * gain
        } else {
            sample
        }
    }

    /// Apply Automatic Gain Control (AGC).
    fn apply_agc(&mut self, sample: f32) -> f32 {
        let mag = sample.abs();
        if mag > 1e-4 {
            let desired_gain = (self.agc_target_level / mag).min(self.agc_max_gain);
            if desired_gain < self.agc_gain {
                // Fast attack for loud signals
                self.agc_gain += self.agc_attack_rate * (desired_gain - self.agc_gain);
            } else {
                // Smooth release for quiet signals
                self.agc_gain += self.agc_release_rate * (desired_gain - self.agc_gain);
            }
        }

        // Apply gain and clip to prevent digital overdrive/clipping [-1.0, 1.0]
        (sample * self.agc_gain).clamp(-1.0, 1.0)
    }
}

/// Shared thread-safe handle for AudioDspProcessor.
#[derive(Clone)]
pub struct SharedAudioDspProcessor(pub Arc<Mutex<AudioDspProcessor>>);

impl SharedAudioDspProcessor {
    pub fn new(enable_aec: bool, enable_ns: bool, enable_agc: bool) -> Self {
        Self(Arc::new(Mutex::new(AudioDspProcessor::new(
            enable_aec, enable_ns, enable_agc,
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dsp_processor_creation() {
        let dsp = AudioDspProcessor::new(true, true, true);
        assert!(dsp.enable_aec);
        assert!(dsp.enable_ns);
        assert!(dsp.enable_agc);
    }

    #[test]
    fn test_dsp_aec_cancels_echo() {
        let mut dsp = AudioDspProcessor::new(true, false, false);
        // Feed speaker output and echo signal
        let speaker_signal = vec![0.5f32; 100];
        dsp.record_speaker_reference(&speaker_signal);

        let mut mic_signal = vec![0.5f32; 100];
        dsp.process_input(&mut mic_signal);

        // After NLMS adaptation, mic signal error should decrease
        assert!(mic_signal.last().unwrap().abs() < 0.5);
    }

    #[test]
    fn test_dsp_ns_suppresses_low_level_noise() {
        let mut dsp = AudioDspProcessor::new(false, true, false);
        let mut noise_samples = vec![0.001f32; 50];
        dsp.process_input(&mut noise_samples);
        for &s in &noise_samples {
            assert!(s.abs() <= 0.001);
        }
    }

    #[test]
    fn test_dsp_agc_normalizes_quiet_signal() {
        let mut dsp = AudioDspProcessor::new(false, false, true);
        let mut quiet_samples = vec![0.05f32; 100];
        dsp.process_input(&mut quiet_samples);
        assert!(*quiet_samples.last().unwrap() > 0.05);
    }
}
