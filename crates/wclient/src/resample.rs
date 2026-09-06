//! Audio resampling module.
//!
//! This module provides linear interpolation resampling for mono audio streams
//! to bridge differences between device hardware sample rates and the WebRTC network
//! sample rate.

/// Linear interpolation resampler for streaming mono audio samples (`f32`).
#[derive(Debug)]
pub struct Resampler {
    src_rate: f64,
    dst_rate: f64,
    phase: f64,
    last_sample: Option<f32>,
}

impl Resampler {
    /// Create a new `Resampler` converting from `src_rate` Hz to `dst_rate` Hz.
    pub fn new(src_rate: u32, dst_rate: u32) -> Self {
        Self {
            src_rate: src_rate as f64,
            dst_rate: dst_rate as f64,
            phase: 0.0,
            last_sample: None,
        }
    }

    /// Resample a slice of mono float samples (`f32`) from `src_rate` to `dst_rate`.
    /// Maintains phase and overlap state across streaming calls.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        if (self.src_rate - self.dst_rate).abs() < 1e-3 {
            self.last_sample = input.last().copied();
            return input.to_vec();
        }

        let step = self.src_rate / self.dst_rate;
        let prev = self.last_sample.unwrap_or(input[0]);
        let n = input.len();
        let limit = n as f64;

        let mut output = Vec::with_capacity((limit / step) as usize + 4);
        let mut out_idx = 0;

        loop {
            let current_phase = self.phase + (out_idx as f64) * step;
            if current_phase >= limit {
                self.phase = current_phase - limit;
                if self.phase < 0.0 {
                    self.phase = 0.0;
                }
                break;
            }

            let idx = current_phase as usize;
            let frac = (current_phase - idx as f64) as f32;

            let s0 = if idx == 0 { prev } else { input[idx - 1] };
            let s1 = if idx < n { input[idx] } else { input[n - 1] };

            let sample = s0 * (1.0 - frac) + s1 * frac;
            output.push(sample);

            out_idx += 1;
        }

        self.last_sample = input.last().copied();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_sample_rate() {
        let mut resampler = Resampler::new(48000, 48000);
        let input = vec![0.1, 0.2, 0.3, 0.4];
        let output = resampler.process(&input);
        assert_eq!(input, output);
    }

    #[test]
    fn test_upsampling() {
        let mut resampler = Resampler::new(16000, 48000);
        let input = vec![1.0, 2.0];
        let output = resampler.process(&input);
        // Expect 6 output samples for 2 input samples at 3x sample rate ratio
        assert_eq!(output.len(), 6);
    }

    #[test]
    fn test_downsampling() {
        let mut resampler = Resampler::new(48000, 24000);
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let output = resampler.process(&input);
        // Expect 2 output samples for 4 input samples at 0.5x sample rate ratio
        assert_eq!(output.len(), 2);
    }
}
