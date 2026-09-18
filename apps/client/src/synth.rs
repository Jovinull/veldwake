//! The sound of a fight, generated rather than recorded.
//!
//! No samples, no assets, no files. A hit is a noise burst through a fast
//! envelope with a low body under it and a short metallic ring over it; a whiff
//! is a band of noise that rises and falls as a blade goes past. Both are a few
//! hundred bytes of arithmetic, which is the only kind of audio this repository
//! is allowed to have: `AGENTS.md` keeps it free of routine manually authored
//! art and audio, and ADR-0007 records the boundary.
//!
//! **Nothing here knows what a device is.** This module has no cpal, no
//! threads, no clock and no I/O: it is a function from voice requests and a
//! sample rate to a buffer of `f32`. That is what makes the claims about it
//! testable at all — every assertion about amplitude, envelope, decay,
//! frequency content and voice limits is made headlessly, against the same code
//! the device plays.
//!
//! **Nothing here allocates.** The voices are a fixed array, the noise is an
//! integer generator, and [`Synth::render`] writes into a caller's slice. It is
//! called from the real-time audio callback, so that is a requirement rather
//! than a preference.

/// How many voices can sound at once.
///
/// Eight. Two combatants, three sounds each within a decay, and room left over;
/// past that the oldest voice is taken, so a pathological burst degrades into
/// fewer simultaneous sounds rather than into a reallocation or a click.
pub const MAX_VOICES: usize = 8;

/// The loudest a single voice starts at.
///
/// Voices sum, so eight at once would clip a `1.0` peak. `0.22` keeps eight
/// simultaneous full-intensity voices inside `±1.76` before the limiter, and
/// the limiter then holds the output inside `±1`.
const VOICE_PEAK: f32 = 0.22;

/// Where the output is held, whatever the voices do.
///
/// A hard ceiling applied after the sum. Clipping is inaudible here because the
/// limiter is a `tanh`-shaped soft knee rather than a clamp, and a clamp is
/// what would make eight coincident hits buzz.
const LIMIT: f32 = 0.92;

/// What kind of sound one request asks for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceKind {
    /// A blade landing on a body.
    Hit,
    /// A blade going past one.
    Whiff,
}

impl VoiceKind {
    /// The two-bit code the queue packs.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Hit => 0,
            Self::Whiff => 1,
        }
    }

    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Hit),
            1 => Some(Self::Whiff),
            _ => None,
        }
    }
}

/// One sound, as asked for rather than as rendered.
///
/// Deliberately tiny and `Copy`: it has to survive being packed into a single
/// `u32` and crossing to the audio thread without a heap or a lock. `intensity`
/// is how hard, in `[0, 1]`; `weight` is how heavy the body of the sound is, in
/// `[0, 1]`, which is what makes a hit on a big adversary read lower than one on
/// the player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceParams {
    pub kind: VoiceKind,
    pub intensity: f32,
    pub weight: f32,
}

impl VoiceParams {
    #[must_use]
    pub fn new(kind: VoiceKind, intensity: f32, weight: f32) -> Self {
        Self {
            kind,
            intensity: clamp01(intensity),
            weight: clamp01(weight),
        }
    }

    /// Packs into one `u32` for the queue: two bits of kind, then seven bits
    /// each of intensity and weight.
    ///
    /// Seven bits is a step of `1/127`, far finer than a listener can tell and
    /// far coarser than the arithmetic needs, which is the right trade for a
    /// value that has to cross a thread boundary without a lock.
    #[must_use]
    pub fn pack(self) -> u32 {
        let quantise = |value: f32| (clamp01(value) * 127.0 + 0.5) as u32 & 0x7f;
        self.kind.code() | (quantise(self.intensity) << 2) | (quantise(self.weight) << 9)
    }

    /// The inverse, rejecting a code that is not a kind.
    #[must_use]
    pub fn unpack(packed: u32) -> Option<Self> {
        let kind = VoiceKind::from_code(packed & 0x3)?;
        let dequantise = |bits: u32| f32::from(u8::try_from(bits & 0x7f).unwrap_or(0)) / 127.0;
        Some(Self {
            kind,
            intensity: dequantise(packed >> 2),
            weight: dequantise(packed >> 9),
        })
    }
}

/// A deterministic noise source.
///
/// xorshift32, seeded per voice from a counter, so the same fight produces the
/// same samples on every run and on every host. A real random source would make
/// an offline capture of the audio impossible to compare against another.
#[derive(Clone, Copy, Debug)]
struct Noise(u32);

impl Noise {
    const fn seeded(seed: u32) -> Self {
        // Never zero: xorshift is stuck there.
        Self(seed | 1)
    }

    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        // Into `[-1, 1)` from the top bits, which are the well-mixed ones.
        f32::from(i16::from_ne_bytes([(x >> 16) as u8, (x >> 24) as u8])) / 32768.0
    }
}

/// One sounding voice.
#[derive(Clone, Copy, Debug)]
struct Voice {
    params: VoiceParams,
    /// Samples rendered so far.
    age: u32,
    /// Samples this voice lasts.
    life: u32,
    noise: Noise,
    /// Phase accumulators, in radians.
    body_phase: f32,
    ring_phase: f32,
    /// One-pole state for the whiff's band.
    filter: f32,
}

impl Voice {
    fn finished(&self) -> bool {
        self.age >= self.life
    }
}

/// The whole synth: a fixed set of voices and the rate they are rendered at.
#[derive(Clone, Debug)]
pub struct Synth {
    sample_rate: f32,
    voices: [Option<Voice>; MAX_VOICES],
    /// Seeds the next voice's noise, so two voices started in the same tick do
    /// not render the identical waveform and cancel into something metallic.
    next_seed: u32,
    /// Voices started, and voices a full set had to displace.
    started: u64,
    displaced: u64,
    /// Loudest absolute sample rendered, for the evidence.
    peak: f32,
}

impl Synth {
    /// Builds a synth for a device's sample rate.
    ///
    /// A non-finite or absurd rate is replaced by `48_000` rather than
    /// propagated: a device that reports nonsense should produce ordinary sound,
    /// not a buffer of `NaN` handed to a driver.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && (8_000.0..=768_000.0).contains(&sample_rate)
        {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            sample_rate,
            voices: [None; MAX_VOICES],
            next_seed: 0x9e37_79b9,
            started: 0,
            displaced: 0,
            peak: 0.0,
        }
    }

    #[must_use]
    pub const fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    #[must_use]
    pub fn live(&self) -> usize {
        self.voices.iter().flatten().count()
    }

    #[must_use]
    pub const fn started(&self) -> u64 {
        self.started
    }

    #[must_use]
    pub const fn displaced(&self) -> u64 {
        self.displaced
    }

    #[must_use]
    pub const fn peak(&self) -> f32 {
        self.peak
    }

    /// Starts one voice.
    ///
    /// With every voice sounding, the **oldest** is displaced. Taking the
    /// newest instead would make a burst of hits drop the one the player just
    /// caused, which is the one that matters.
    pub fn start(&mut self, params: VoiceParams) {
        let life = self.life_samples(params);
        self.next_seed = self
            .next_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let voice = Voice {
            params,
            age: 0,
            life,
            noise: Noise::seeded(self.next_seed),
            body_phase: 0.0,
            ring_phase: 0.0,
            filter: 0.0,
        };
        self.started = self.started.saturating_add(1);
        if let Some(slot) = self.voices.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(voice);
            return;
        }
        // Full: displace the oldest.
        let mut oldest = 0;
        let mut oldest_age = 0;
        for (index, slot) in self.voices.iter().enumerate() {
            if let Some(existing) = slot
                && existing.age >= oldest_age
            {
                oldest_age = existing.age;
                oldest = index;
            }
        }
        self.voices[oldest] = Some(voice);
        self.displaced = self.displaced.saturating_add(1);
    }

    /// How long one voice lasts, in samples.
    fn life_samples(&self, params: VoiceParams) -> u32 {
        // A hit is short and a whiff is shorter: a quarter of a second against
        // a sixth. Heavier bodies ring a little longer.
        let seconds = match params.kind {
            VoiceKind::Hit => 0.11_f32.mul_add(params.weight, 0.19),
            VoiceKind::Whiff => 0.16,
        };
        let samples = seconds * self.sample_rate;
        if samples.is_finite() && samples > 0.0 {
            samples as u32
        } else {
            1
        }
    }

    /// Renders into an interleaved buffer and returns how many frames it wrote.
    ///
    /// Every frame is written, silence included, because a callback that leaves
    /// its buffer untouched plays whatever was there before. Allocation-free and
    /// panic-free: the only indexing is into the fixed voice array and into the
    /// caller's slice by a bounded chunk iterator.
    pub fn render(&mut self, out: &mut [f32], channels: usize) -> usize {
        let channels = channels.max(1);
        let mut frames = 0;
        for frame in out.chunks_mut(channels) {
            let mut sample = 0.0_f32;
            for slot in &mut self.voices {
                let Some(voice) = slot.as_mut() else {
                    continue;
                };
                sample += Self::voice_sample(voice, self.sample_rate);
                voice.age = voice.age.saturating_add(1);
                if voice.finished() {
                    *slot = None;
                }
            }
            // Soft knee rather than a clamp: eight coincident hits get quieter
            // rather than square.
            let limited = (sample / LIMIT).tanh() * LIMIT;
            let limited = if limited.is_finite() { limited } else { 0.0 };
            self.peak = self.peak.max(limited.abs());
            for channel in frame.iter_mut() {
                *channel = limited;
            }
            frames += 1;
        }
        frames
    }

    /// One voice's contribution to one frame, and its state advanced.
    fn voice_sample(voice: &mut Voice, sample_rate: f32) -> f32 {
        let life = voice.life.max(1);
        let t = f32::from(u16::try_from(voice.age.min(life)).unwrap_or(u16::MAX))
            / f32::from(u16::try_from(life).unwrap_or(u16::MAX));
        let step = 1.0 / sample_rate;
        let noise = voice.noise.next();
        match voice.params.kind {
            VoiceKind::Hit => {
                // Three parts: a very fast noise transient that is the impact
                // itself, a low body that carries the weight, and a short
                // metallic ring that says it was a blade.
                let transient = (-38.0 * t).exp();
                let body_hz = 150.0_f32.mul_add(-voice.params.weight, 210.0);
                let ring_hz = 900.0_f32.mul_add(-voice.params.weight, 2_100.0);
                voice.body_phase += std::f32::consts::TAU * body_hz * step;
                voice.ring_phase += std::f32::consts::TAU * ring_hz * step;
                let body = voice.body_phase.sin() * (-9.0 * t).exp();
                let ring = voice.ring_phase.sin() * (-16.0 * t).exp();
                let mixed =
                    0.30_f32.mul_add(ring, 0.55_f32.mul_add(noise * transient, 0.55 * body));
                mixed * VOICE_PEAK * voice.params.intensity
            }
            VoiceKind::Whiff => {
                // Noise through a one-pole low-pass whose corner rises and then
                // falls: a blade passing, with no impact in it anywhere.
                let sweep = (t * std::f32::consts::PI).sin();
                let corner = 900.0_f32.mul_add(sweep, 400.0);
                let alpha = (std::f32::consts::TAU * corner * step).clamp(0.0, 1.0);
                voice.filter += alpha * (noise - voice.filter);
                let envelope = sweep * (-2.2 * t).exp();
                voice.filter * envelope * VOICE_PEAK * voice.params.intensity
            }
        }
    }
}

fn clamp01(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::{LIMIT, MAX_VOICES, Synth, VoiceKind, VoiceParams};

    const RATE: f32 = 48_000.0;

    fn rendered(synth: &mut Synth, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames];
        assert_eq!(synth.render(&mut out, 1), frames);
        out
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |worst, s| worst.max(s.abs()))
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let total: f32 = samples.iter().map(|s| s * s).sum();
        (total / samples.len() as f32).sqrt()
    }

    /// Energy below and above a corner, by one-pole split. Crude and enough:
    /// the claim is only that a hit has more low energy than a whiff.
    fn split(samples: &[f32], corner_hz: f32) -> (f32, f32) {
        let alpha = (std::f32::consts::TAU * corner_hz / RATE).clamp(0.0, 1.0);
        let mut low = 0.0_f32;
        let mut lows = Vec::with_capacity(samples.len());
        let mut highs = Vec::with_capacity(samples.len());
        for sample in samples {
            low += alpha * (sample - low);
            lows.push(low);
            highs.push(sample - low);
        }
        (rms(&lows), rms(&highs))
    }

    #[test]
    fn an_idle_synth_writes_silence_rather_than_leaving_the_buffer_alone() {
        // A callback that returns without writing plays whatever the driver had
        // in the buffer, which is noise. Silence has to be written.
        let mut synth = Synth::new(RATE);
        let mut out = vec![0.7; 512];
        assert_eq!(synth.render(&mut out, 1), 512);
        assert!(out.iter().all(|sample| *sample == 0.0), "{:?}", &out[..4]);
        assert_eq!(synth.live(), 0);
        assert_eq!(synth.peak(), 0.0);
    }

    #[test]
    fn a_hit_makes_a_sound_that_starts_loud_and_decays_to_nothing() {
        let mut synth = Synth::new(RATE);
        synth.start(VoiceParams::new(VoiceKind::Hit, 1.0, 0.5));
        assert_eq!(synth.live(), 1);
        let samples = rendered(&mut synth, 24_000);
        assert!(
            samples.iter().all(|s| s.is_finite()),
            "a sample was not finite"
        );
        // Loud at the front.
        let attack = peak(&samples[..480]);
        assert!(attack > 0.02, "the attack was {attack}, which is inaudible");
        // Monotonically quieter, in tenth-of-a-second windows.
        let window = (RATE * 0.05) as usize;
        let mut previous = f32::MAX;
        for chunk in samples.chunks(window).take(5) {
            let level = rms(chunk);
            assert!(
                level < previous,
                "the envelope did not decay: {level} after {previous}"
            );
            previous = level;
        }
        // Gone by the end, and the voice freed itself.
        assert!(peak(&samples[12_000..]) < 1.0e-4, "the tail never ended");
        assert_eq!(synth.live(), 0, "a finished voice was not released");
    }

    #[test]
    fn nothing_clips_however_many_voices_sound_at_once() {
        // The claim the limiter exists for. Eight full-intensity hits started on
        // the same sample is the worst case the voice limit allows.
        let mut synth = Synth::new(RATE);
        for _ in 0..MAX_VOICES {
            synth.start(VoiceParams::new(VoiceKind::Hit, 1.0, 1.0));
        }
        assert_eq!(synth.live(), MAX_VOICES);
        let samples = rendered(&mut synth, 24_000);
        let worst = peak(&samples);
        assert!(worst <= LIMIT + 1.0e-6, "the output reached {worst}");
        assert!(worst > 0.05, "the limiter held by producing nothing");
        assert!(samples.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn a_ninth_voice_displaces_the_oldest_rather_than_being_lost() {
        let mut synth = Synth::new(RATE);
        for _ in 0..MAX_VOICES {
            synth.start(VoiceParams::new(VoiceKind::Hit, 1.0, 0.5));
        }
        // Age the set, so "oldest" means something.
        let _ = rendered(&mut synth, 256);
        assert_eq!(synth.displaced(), 0);
        synth.start(VoiceParams::new(VoiceKind::Hit, 1.0, 0.5));
        assert_eq!(synth.live(), MAX_VOICES, "the voice set grew");
        assert_eq!(synth.displaced(), 1);
        assert_eq!(synth.started(), u64::from(MAX_VOICES as u32) + 1);
    }

    #[test]
    fn a_whiff_does_not_sound_like_a_hit() {
        // The distinction §20 turns on: a miss may be heard, and must not be
        // heard as damage. A hit carries a low body; a whiff is a band of noise
        // with nothing under it.
        let mut hit = Synth::new(RATE);
        hit.start(VoiceParams::new(VoiceKind::Hit, 1.0, 0.5));
        let hit_samples = rendered(&mut hit, 12_000);

        let mut whiff = Synth::new(RATE);
        whiff.start(VoiceParams::new(VoiceKind::Whiff, 1.0, 0.5));
        let whiff_samples = rendered(&mut whiff, 12_000);

        let (hit_low, hit_high) = split(&hit_samples, 300.0);
        let (whiff_low, whiff_high) = split(&whiff_samples, 300.0);
        assert!(
            hit_low > whiff_low * 2.0,
            "the hit's low energy {hit_low:.5} is not clear of the whiff's {whiff_low:.5}"
        );
        assert!(
            whiff_high / whiff_low.max(1.0e-9) > hit_high / hit_low.max(1.0e-9),
            "the whiff is not the brighter of the two"
        );
        // And a whiff is quieter overall, so a miss never dominates a hit.
        assert!(
            peak(&whiff_samples) < peak(&hit_samples),
            "a miss was louder than a landed hit"
        );
    }

    #[test]
    fn a_heavier_body_sounds_lower_than_a_lighter_one() {
        // `weight` is the only thing separating a hit on the broad adversary
        // from one on the player, so it has to do something measurable.
        let render = |weight: f32| {
            let mut synth = Synth::new(RATE);
            synth.start(VoiceParams::new(VoiceKind::Hit, 1.0, weight));
            rendered(&mut synth, 12_000)
        };
        let (light_low, _) = split(&render(0.0), 180.0);
        let (heavy_low, _) = split(&render(1.0), 180.0);
        assert!(
            heavy_low > light_low,
            "heavy {heavy_low:.5} is not lower-pitched than light {light_low:.5}"
        );
    }

    #[test]
    fn intensity_scales_the_sound_rather_than_switching_it_on() {
        let level = |intensity: f32| {
            let mut synth = Synth::new(RATE);
            synth.start(VoiceParams::new(VoiceKind::Hit, intensity, 0.5));
            peak(&rendered(&mut synth, 12_000))
        };
        let quiet = level(0.25);
        let loud = level(1.0);
        assert!(quiet > 0.0, "a quarter-intensity hit was silent");
        assert!(loud > quiet * 2.0, "{loud} is not clearly above {quiet}");
        // Zero intensity is a request for nothing, and gets nothing.
        assert!(level(0.0) < 1.0e-6);
    }

    #[test]
    fn the_same_requests_give_the_same_samples() {
        // What makes an offline capture comparable rather than merely listenable.
        let run = || {
            let mut synth = Synth::new(RATE);
            let mut out = Vec::new();
            for index in 0..6 {
                let kind = if index % 2 == 0 {
                    VoiceKind::Hit
                } else {
                    VoiceKind::Whiff
                };
                synth.start(VoiceParams::new(kind, 0.8, 0.3));
                out.extend(rendered(&mut synth, 900));
            }
            out
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn a_device_that_reports_nonsense_still_produces_ordinary_sound() {
        for rate in [0.0, -48_000.0, f32::NAN, f32::INFINITY, 3.0, 1.0e9] {
            let mut synth = Synth::new(rate);
            assert!(synth.sample_rate().is_finite());
            assert!(synth.sample_rate() >= 8_000.0);
            synth.start(VoiceParams::new(VoiceKind::Hit, 1.0, 0.5));
            let samples = rendered(&mut synth, 4_800);
            assert!(
                samples.iter().all(|s| s.is_finite()),
                "rate {rate} gave a NaN"
            );
            assert!(peak(&samples) <= LIMIT + 1.0e-6);
        }
    }

    #[test]
    fn every_channel_count_gets_the_same_frames() {
        // Mono, stereo, and a surround device: the synth is mono and every
        // channel carries it, so no channel is ever left with stale data.
        for channels in [1_usize, 2, 6] {
            let mut synth = Synth::new(RATE);
            synth.start(VoiceParams::new(VoiceKind::Hit, 1.0, 0.5));
            let mut out = vec![9.0; 600 * channels];
            assert_eq!(synth.render(&mut out, channels), 600);
            for frame in out.chunks(channels) {
                assert!(frame.iter().all(|s| (*s - frame[0]).abs() < 1.0e-9));
                assert!(frame.iter().all(|s| *s != 9.0), "a channel was left stale");
            }
        }
        // A zero channel count is a broken device, not a divide by zero.
        let mut synth = Synth::new(RATE);
        let mut out = vec![0.0; 16];
        assert_eq!(synth.render(&mut out, 0), 16);
    }

    #[test]
    fn a_request_survives_being_packed_into_a_word() {
        for kind in [VoiceKind::Hit, VoiceKind::Whiff] {
            for intensity in 0..=10_u8 {
                for weight in 0..=10_u8 {
                    let params = VoiceParams::new(
                        kind,
                        f32::from(intensity) / 10.0,
                        f32::from(weight) / 10.0,
                    );
                    let round = match VoiceParams::unpack(params.pack()) {
                        Some(round) => round,
                        None => panic!("{params:?} did not survive packing"),
                    };
                    assert_eq!(round.kind, params.kind);
                    assert!((round.intensity - params.intensity).abs() < 0.01);
                    assert!((round.weight - params.weight).abs() < 0.01);
                }
            }
        }
        // A code that is not a kind is refused rather than guessed at.
        assert!(VoiceParams::unpack(2).is_none());
        assert!(VoiceParams::unpack(3).is_none());
    }

    #[test]
    fn an_impossible_request_is_clamped_rather_than_rendered() {
        // The rule is one rule: a value that is not a finite number is zero.
        // Not "clamp infinity to the top of the range" — a caller that hands
        // this an infinity has a bug, and the quietest, lightest sound is the
        // safest thing to make of it.
        let params = VoiceParams::new(VoiceKind::Hit, f32::NAN, f32::INFINITY);
        assert_eq!(params.intensity, 0.0);
        assert_eq!(params.weight, 0.0);
        // A finite value outside the range is an ordinary clamp.
        let clamped = VoiceParams::new(VoiceKind::Hit, 4.0, -1.0);
        assert_eq!(clamped.intensity, 1.0);
        assert_eq!(clamped.weight, 0.0);
        let mut synth = Synth::new(RATE);
        synth.start(VoiceParams::new(VoiceKind::Hit, 100.0, -5.0));
        let samples = rendered(&mut synth, 4_800);
        assert!(samples.iter().all(|s| s.is_finite()));
        assert!(peak(&samples) <= LIMIT + 1.0e-6);
    }
}
