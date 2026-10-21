// Eden DAW — Analytic responses used by rack device displays.
//
// These functions only describe what the DSP in `effects/` does so a display can
// draw it; they never feed audio. Each response is checked against the real
// effect in the tests below, so a display cannot drift from the sound.

use super::dsp_primitives::fast_tan;
use super::effects::{early_unit_ms, EARLY_TAP_PRIMES};

/// Lowest cutoff of the LP/HP filters in Hz (normalized cutoff 0.0).
pub const FILTER_MIN_CUTOFF_HZ: f64 = 20.0;
/// Octaves spanned by the normalized cutoff control: log2(20000 / 20).
/// Matches the `9.965784284662087` constant in `lp_filter.rs` / `hp_filter.rs`.
pub const FILTER_CUTOFF_OCTAVES: f64 = 9.965784284662087;
/// Highest cutoff as a fraction of the sample rate, as clamped by the filters.
pub const FILTER_MAX_CUTOFF_FRACTION_OF_SR: f64 = 0.49;
/// Resonance is clamped below 1.0 inside `svf_tick` to keep the filter stable.
pub const SVF_MAX_RESONANCE: f64 = 0.99;

/// Which output of the shared state-variable filter a device uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvfOutput {
    LowPass,
    HighPass,
}

/// Cutoff in Hz for a normalized 0..1 cutoff control, as the filters compute it.
pub fn filter_cutoff_hz(cutoff_norm: f64, sample_rate: f64) -> f64 {
    (FILTER_MIN_CUTOFF_HZ * (cutoff_norm * FILTER_CUTOFF_OCTAVES).exp2())
        .clamp(FILTER_MIN_CUTOFF_HZ, sample_rate * FILTER_MAX_CUTOFF_FRACTION_OF_SR)
}

/// Magnitude in dB at `freq_hz` of the filter implemented by `svf_tick`
/// (output gain not included).
///
/// `svf_tick` is a trapezoidal-integration ("topology-preserving transform")
/// state-variable filter, the family described in Zavalishin's *The Art of VA
/// Filter Design* and in Simper's (Cytomic) trapezoidal SVF notes. Eden's source
/// does not record which reference it follows and this module was written without
/// consulting them online, so the transfer functions below are derived from the
/// code itself and checked numerically against it (see the tests).
///
/// With `g = tan(pi*fc/fs)` and damping `k = 2 - 2*resonance`, the response in the
/// normalized variable `s = j * tan(pi*f/fs) / g` is
///
/// * low-pass  `1 / (s^2 + k*s + 1)`
/// * high-pass `s^2 / (s^2 + k*s + 1)`
///
/// Eden computes `g` with `fast_tan` (a Pade approximation), so the same function
/// is used here.
pub fn svf_magnitude_db(
    output: SvfOutput,
    freq_hz: f64,
    cutoff_hz: f64,
    resonance: f64,
    sample_rate: f64,
) -> f64 {
    let g = fast_tan(std::f64::consts::PI * cutoff_hz / sample_rate);
    let k = 2.0 - 2.0 * resonance.clamp(0.0, SVF_MAX_RESONANCE);
    let r = (std::f64::consts::PI * freq_hz / sample_rate).tan() / g;
    let r2 = r * r;
    let denominator = ((1.0 - r2) * (1.0 - r2) + k * k * r2).sqrt();
    let numerator = match output {
        SvfOutput::LowPass => 1.0,
        SvfOutput::HighPass => r2,
    };
    20.0 * (numerator / denominator).max(1e-9).log10()
}

/// One early reflection of the reverb: delay after the pre-delayed input and its
/// level relative to the strongest tap (`process_early` weights tap `i` by `1/(i+2)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarlyTap {
    pub time_ms: f64,
    pub relative_level: f64,
}

/// Early-reflection taps of the reverb for a `size` parameter value, in time order.
pub fn reverb_early_taps(size: f64) -> Vec<EarlyTap> {
    let unit_ms = early_unit_ms(size);
    let strongest = 1.0 / 2.0;
    EARLY_TAP_PRIMES
        .iter()
        .enumerate()
        .map(|(i, &prime)| EarlyTap {
            time_ms: prime * unit_ms,
            relative_level: (1.0 / (i as f64 + 2.0)) / strongest,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{EffectModule, FxHpFilter, FxLpFilter};

    const SAMPLE_RATE_HZ: f64 = 44100.0;
    /// Time given to the effect's parameter smoothers to reach their targets.
    const SETTLE_SECONDS: f64 = 1.0;
    /// Time over which output power is measured once settled.
    const MEASURE_SECONDS: f64 = 0.25;
    /// Display accuracy required against the real filter, in dB.
    const TOLERANCE_DB: f64 = 0.5;

    fn measured_gain_db(effect: &mut dyn EffectModule, params: &[(String, f32)], freq_hz: f64) -> f64 {
        let step = 2.0 * std::f64::consts::PI * freq_hz / SAMPLE_RATE_HZ;
        let total = ((SETTLE_SECONDS + MEASURE_SECONDS) * SAMPLE_RATE_HZ) as usize;
        let measure_from = (SETTLE_SECONDS * SAMPLE_RATE_HZ) as usize;
        let (mut in_power, mut out_power) = (0.0, 0.0);
        for n in 0..total {
            let x = (step * n as f64).sin() * 0.1;
            let (y, _) = effect.process(x, x, params, SAMPLE_RATE_HZ);
            if n >= measure_from {
                in_power += x * x;
                out_power += y * y;
            }
        }
        10.0 * (out_power / in_power).log10()
    }

    fn params(cutoff: f32, reso: f32) -> Vec<(String, f32)> {
        vec![
            ("cutoff".into(), cutoff),
            ("reso".into(), reso),
            ("output_db".into(), 0.0),
        ]
    }

    #[test]
    fn display_matches_real_low_pass_filter() {
        for &(cutoff, reso) in &[(0.5_f32, 0.0_f32), (0.6, 0.5), (0.7, 0.9)] {
            let p = params(cutoff, reso);
            let cutoff_hz = filter_cutoff_hz(cutoff as f64, SAMPLE_RATE_HZ);
            for &freq in &[100.0, 400.0, 1000.0, 3000.0, 8000.0] {
                let mut real = FxLpFilter::new();
                let shown =
                    svf_magnitude_db(SvfOutput::LowPass, freq, cutoff_hz, reso as f64, SAMPLE_RATE_HZ);
                let actual = measured_gain_db(&mut real, &p, freq);
                assert!(
                    (shown - actual).abs() < TOLERANCE_DB,
                    "LP cutoff={cutoff} reso={reso} f={freq}: shown {shown:.2} dB, real {actual:.2} dB"
                );
            }
        }
    }

    #[test]
    fn display_matches_real_high_pass_filter() {
        for &(cutoff, reso) in &[(0.5_f32, 0.0_f32), (0.6, 0.5), (0.7, 0.9)] {
            let p = params(cutoff, reso);
            let cutoff_hz = filter_cutoff_hz(cutoff as f64, SAMPLE_RATE_HZ);
            for &freq in &[100.0, 400.0, 1000.0, 3000.0, 8000.0] {
                let mut real = FxHpFilter::new();
                let shown =
                    svf_magnitude_db(SvfOutput::HighPass, freq, cutoff_hz, reso as f64, SAMPLE_RATE_HZ);
                let actual = measured_gain_db(&mut real, &p, freq);
                assert!(
                    (shown - actual).abs() < TOLERANCE_DB,
                    "HP cutoff={cutoff} reso={reso} f={freq}: shown {shown:.2} dB, real {actual:.2} dB"
                );
            }
        }
    }

    #[test]
    fn resonance_raises_the_peak_at_cutoff() {
        let cutoff_hz = filter_cutoff_hz(0.6, SAMPLE_RATE_HZ);
        let flat = svf_magnitude_db(SvfOutput::LowPass, cutoff_hz, cutoff_hz, 0.0, SAMPLE_RATE_HZ);
        let resonant = svf_magnitude_db(SvfOutput::LowPass, cutoff_hz, cutoff_hz, 0.9, SAMPLE_RATE_HZ);
        assert!(resonant > flat + 6.0, "flat {flat:.1} dB vs resonant {resonant:.1} dB");
    }

    #[test]
    fn reverb_early_taps_match_real_impulse_response() {
        use crate::modules::FxReverb;
        const SIZE: f32 = 24.0;
        /// Allowed disagreement between displayed and measured tap time.
        const TAP_TOLERANCE_SAMPLES: f64 = 2.0;
        let params: Vec<(String, f32)> = [
            ("mix", 100.0), ("early", 100.0), ("early_send", 0.0), ("late", 0.0),
            ("size", SIZE), ("predelay", 0.0), ("width", 100.0), ("output_db", 0.0),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
        let mut reverb = FxReverb::new(SAMPLE_RATE_HZ as u32);
        let length = (0.3 * SAMPLE_RATE_HZ) as usize;
        let response: Vec<f64> = (0..length)
            .map(|n| reverb.process(if n == 0 { 1.0 } else { 0.0 }, 0.0, &params, SAMPLE_RATE_HZ).0)
            .collect();
        for tap in reverb_early_taps(SIZE as f64) {
            let expected = tap.time_ms * SAMPLE_RATE_HZ / 1000.0;
            let lo = (expected - TAP_TOLERANCE_SAMPLES).floor().max(0.0) as usize;
            let hi = (expected + TAP_TOLERANCE_SAMPLES).ceil() as usize;
            let peak = response[lo..=hi].iter().fold(0.0_f64, |m, v| m.max(v.abs()));
            let background = response[hi + 4..hi + 40].iter().fold(0.0_f64, |m, v| m.max(v.abs()));
            assert!(peak > 0.0 && peak > 4.0 * background,
                "no reflection near {:.2} ms (peak {:e}, background {:e})", tap.time_ms, peak, background);
        }
    }
}
