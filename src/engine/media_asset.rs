//! Immutable decoded media shared by clip instances, previews and render snapshots.
//! Channel order follows Hound's WavSpec and CPAL's interleaved frame contract:
//! https://github.com/ruuda/hound/blob/master/src/lib.rs
//! https://github.com/RustAudio/cpal/blob/master/examples/beep.rs
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone)]
pub struct AudioAsset {
    pub samples: Arc<Vec<f32>>,
    pub channels: usize,
    pub sample_rate: u32,
    mono: OnceLock<Arc<Vec<f32>>>,
}
impl AudioAsset {
    pub fn empty() -> Self {
        Self {
            samples: Arc::new(Vec::new()),
            channels: 1,
            sample_rate: 44100,
            mono: OnceLock::new(),
        }
    }
    /// Only snapshot preparation/sampler loading calls this, never the callback.
    pub fn mono_samples(&self) -> Arc<Vec<f32>> {
        if self.channels == 1 {
            return self.samples.clone();
        }
        self.mono
            .get_or_init(|| {
                Arc::new(
                    self.samples
                        .chunks_exact(self.channels)
                        .map(|frame| frame.iter().sum::<f32>() / self.channels as f32)
                        .collect(),
                )
            })
            .clone()
    }
    pub fn pin_for_retirement(&self, retired: &mut Vec<Arc<Vec<f32>>>) {
        retired.push(self.samples.clone());
        if let Some(mono) = self.mono.get() {
            retired.push(mono.clone());
        }
    }
}

pub fn load_audio_asset(path: &std::path::Path) -> Result<AudioAsset, String> {
    let (mut samples, mut channels, sample_rate) = super::load_audio_interleaved(path)?;
    if channels == 0 || sample_rate == 0 {
        return Err("Invalid audio channel count/sample rate".into());
    }
    // Preserve the legacy downmix for unsupported surround layouts; mono/stereo
    // assets now keep their original frame layout instead of averaging L and R.
    if channels > 2 {
        samples = samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();
        channels = 1;
    }
    Ok(AudioAsset {
        samples: Arc::new(samples),
        channels,
        sample_rate,
        mono: OnceLock::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_stereo_is_shared_without_cancelling_antiphase_channels() {
        let path = std::env::temp_dir().join(format!(
            "eden-stereo-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..1000 {
            writer.write_sample(0.5_f32).unwrap();
            writer.write_sample(-0.5_f32).unwrap();
        }
        writer.finalize().unwrap();
        let asset = load_audio_asset(&path).unwrap();
        assert_eq!(asset.channels, 2);
        assert_eq!(asset.samples.len(), 2000);
        assert_eq!(&asset.samples[..2], &[0.5, -0.5]);
        let clone = asset.clone();
        assert!(Arc::ptr_eq(&asset.samples, &clone.samples));
        // Only the existing mono sampler explicitly requests a downmix.
        let mono = asset.mono_samples();
        assert_eq!(mono.len(), 1000);
        assert!(mono.iter().all(|sample| *sample == 0.0));
        assert!(Arc::ptr_eq(&mono, &asset.mono_samples()));
        std::fs::remove_file(path).unwrap();
    }
}
