//! The device, and the one-way street the game speaks to it through.
//!
//! Two things live here and they are deliberately separate:
//!
//! - [`SoundQueue`], a lock-free single-producer single-consumer ring of packed
//!   requests. The game writes; the audio callback reads. Nothing else touches
//!   it.
//! - [`AudioDevice`], a thin adapter that opens a cpal output stream and, in
//!   its callback, drains the queue into a [`Synth`](crate::synth::Synth) and
//!   renders. It is the only code in the repository that knows cpal exists.
//!
//! # The real-time contract
//!
//! The callback runs on a thread the driver owns, under a deadline measured in
//! milliseconds, and missing it is an audible glitch rather than a slow frame.
//! So the callback **never allocates, never locks, never logs, never blocks and
//! never panics**, and every one of those is a property of the code rather than
//! an intention:
//!
//! - *No allocation*: the synth's voices are a fixed array, the queue is a
//!   fixed array, and the callback writes into the buffer cpal hands it.
//! - *No lock*: the queue is atomics only. A `Mutex` would be simpler and would
//!   be a priority inversion waiting to happen — the game thread can be
//!   preempted holding it, and then the audio thread waits for a scheduler.
//! - *No log*: `tracing` allocates and can take a subscriber's lock. Everything
//!   the callback wants to report is an `AtomicU64` the game thread reads and
//!   logs on its own time.
//! - *No panic*: the only indexing is into fixed arrays by a masked index, and
//!   every arithmetic path is checked or saturating.
//!
//! The queue is atomics rather than `std::sync::mpsc` for the same reason. An
//! `mpsc` receiver is not documented to be allocation-free, and a channel that
//! *probably* does not allocate is not a real-time channel. Nothing here needs
//! `unsafe` to say that, which matters: the workspace forbids it.
//!
//! # No device is not an error
//!
//! A host with no sound card, no default output, or a driver that refuses the
//! format still plays the game. The failure is reported once, counted, and then
//! the game runs silently. That is also what CI does, since a build machine has
//! no audio device and must not fail for it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tracing::{info, warn};

use crate::synth::{MAX_VOICES, Synth, VoiceParams};

/// Requests the queue can hold before it starts refusing them.
///
/// Sixty-four. A frame at four ticks can produce at most a handful of sounds,
/// and the callback drains the whole queue every time it runs, so this is two
/// orders of magnitude more than the steady state needs. It is a power of two
/// so the index arithmetic is a mask.
pub const QUEUE_SLOTS: usize = 64;

const QUEUE_MASK: u32 = QUEUE_SLOTS as u32 - 1;

/// A lock-free single-producer single-consumer ring of packed sound requests.
///
/// One producer: the game thread. One consumer: the audio callback. That is a
/// precondition rather than something enforced, and it is what lets the indices
/// be plain monotonic counters with no compare-and-swap anywhere.
///
/// The orderings are the textbook ones for this structure. The producer
/// publishes a slot with a `Release` store to `write`, which the consumer pairs
/// with an `Acquire` load; the consumer frees a slot with a `Release` store to
/// `read`, which the producer pairs with an `Acquire` load. The slots
/// themselves are `Relaxed`, because the index stores are what order them.
#[derive(Debug)]
pub struct SoundQueue {
    slots: [AtomicU32; QUEUE_SLOTS],
    write: AtomicU32,
    read: AtomicU32,
    /// Requests a full queue refused. Never silent: this is what says the
    /// bound was too small rather than leaving it to be guessed at.
    dropped: AtomicU64,
    /// Requests the callback consumed, so a queue that is filling up and one
    /// that is being drained can be told apart.
    consumed: AtomicU64,
}

impl Default for SoundQueue {
    fn default() -> Self {
        Self {
            slots: [const { AtomicU32::new(0) }; QUEUE_SLOTS],
            write: AtomicU32::new(0),
            read: AtomicU32::new(0),
            dropped: AtomicU64::new(0),
            consumed: AtomicU64::new(0),
        }
    }
}

impl SoundQueue {
    /// Offers a request. Returns whether it was taken.
    ///
    /// Called from the game thread only. Never blocks and never waits for the
    /// callback: a full queue means the audio thread has stalled, and the right
    /// response to that is to drop a sound and count it, not to stall the frame
    /// as well.
    pub fn push(&self, params: VoiceParams) -> bool {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        if write.wrapping_sub(read) >= QUEUE_MASK {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.slots[(write & QUEUE_MASK) as usize].store(params.pack(), Ordering::Relaxed);
        self.write.store(write.wrapping_add(1), Ordering::Release);
        true
    }

    /// Takes the next request, if there is one.
    ///
    /// Called from the audio callback only. Allocation-free, lock-free, and
    /// wait-free: it either has a request or it does not.
    pub fn pop(&self) -> Option<VoiceParams> {
        let read = self.read.load(Ordering::Relaxed);
        let write = self.write.load(Ordering::Acquire);
        if read == write {
            return None;
        }
        let packed = self.slots[(read & QUEUE_MASK) as usize].load(Ordering::Relaxed);
        self.read.store(read.wrapping_add(1), Ordering::Release);
        self.consumed.fetch_add(1, Ordering::Relaxed);
        VoiceParams::unpack(packed)
    }

    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn consumed(&self) -> u64 {
        self.consumed.load(Ordering::Relaxed)
    }

    /// How many requests are waiting. A number for the report, not a decision:
    /// it can change the moment it is read.
    #[must_use]
    pub fn waiting(&self) -> u32 {
        self.write
            .load(Ordering::Acquire)
            .wrapping_sub(self.read.load(Ordering::Acquire))
    }
}

/// What the callback has done, for the game thread to read and log.
///
/// Atomics rather than a channel because the callback may not allocate and may
/// not log, and because a stale number is fine: these are reported on a
/// five-second cadence.
#[derive(Debug, Default)]
pub struct AudioCounters {
    /// Times the callback has run.
    pub callbacks: AtomicU64,
    /// Frames the callback has rendered.
    pub frames: AtomicU64,
    /// Voices started and voices displaced by the voice limit.
    pub started: AtomicU64,
    pub displaced: AtomicU64,
    /// Largest number of voices ever sounding at once.
    pub voice_high_water: AtomicU64,
    /// Largest buffer the device ever asked for, in frames.
    pub buffer_high_water: AtomicU64,
    /// Loudest sample ever rendered, scaled by `1e6` so it fits an integer.
    pub peak_micros: AtomicU64,
    /// Device errors reported after the stream opened.
    pub errors: AtomicU64,
}

/// The device, or the honest absence of one.
pub struct AudioDevice {
    queue: Arc<SoundQueue>,
    counters: Arc<AudioCounters>,
    /// Kept alive because dropping it closes the stream. Nothing reads it.
    _stream: Option<cpal::Stream>,
    sample_rate: u32,
    channels: u16,
    open: bool,
}

impl AudioDevice {
    /// Opens the default output, or reports once and stays silent.
    ///
    /// Called only when an encounter is actually running: with the encounter
    /// off, nothing constructs this, so no device is opened, no audio thread
    /// exists and the process holds no handle on the sound card.
    #[must_use]
    pub fn open() -> Self {
        let queue = Arc::new(SoundQueue::default());
        let counters = Arc::new(AudioCounters::default());
        let silent = |reason: &str| {
            // Once, at startup, and never again: a warning per frame about a
            // missing sound card is worse than no sound.
            warn!(reason, "no audio device; the fight will be silent");
            Self {
                queue: Arc::clone(&queue),
                counters: Arc::clone(&counters),
                _stream: None,
                sample_rate: 0,
                channels: 0,
                open: false,
            }
        };

        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return silent("the host reported no default output device");
        };
        let config = match device.default_output_config() {
            Ok(config) => config,
            Err(error) => {
                return silent(&format!("the device has no usable output config: {error}"));
            }
        };
        let sample_rate = config.sample_rate();
        let channels = config.channels();
        let format = config.sample_format();
        if format != cpal::SampleFormat::F32 {
            // Deliberately narrow: this plays `f32` or it plays nothing. A
            // conversion path for every sample format cpal supports is a pile
            // of code for a case the audited host does not have, and silence is
            // an honest outcome rather than a hidden one.
            return silent(&format!("the device wants {format:?} samples, not f32"));
        }

        let mut synth = Synth::new(sample_rate as f32);
        let synth_rate = synth.sample_rate();
        let callback_queue = Arc::clone(&queue);
        let callback_counters = Arc::clone(&counters);
        let error_counters = Arc::clone(&counters);
        let stream = device.build_output_stream(
            config.config(),
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Everything below is allocation-free, lock-free and
                // panic-free. See the module documentation for why that is a
                // requirement and not an aspiration.
                while let Some(params) = callback_queue.pop() {
                    synth.start(params);
                }
                let frames = synth.render(out, usize::from(channels));
                let counters = &callback_counters;
                counters.callbacks.fetch_add(1, Ordering::Relaxed);
                counters.frames.fetch_add(frames as u64, Ordering::Relaxed);
                counters.started.store(synth.started(), Ordering::Relaxed);
                counters
                    .displaced
                    .store(synth.displaced(), Ordering::Relaxed);
                counters
                    .voice_high_water
                    .fetch_max(synth.live() as u64, Ordering::Relaxed);
                counters
                    .buffer_high_water
                    .fetch_max(frames as u64, Ordering::Relaxed);
                counters
                    .peak_micros
                    .fetch_max((synth.peak() * 1.0e6) as u64, Ordering::Relaxed);
            },
            move |error| {
                // Also the audio thread, so also no logging: count it and let
                // the game thread say so.
                error_counters.errors.fetch_add(1, Ordering::Relaxed);
                let _ = error;
            },
            None,
        );
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => return silent(&format!("the output stream did not build: {error}")),
        };
        if let Err(error) = stream.play() {
            return silent(&format!("the output stream did not start: {error}"));
        }
        let name = device
            .description()
            .map_or_else(|_| "unnamed".to_owned(), |about| about.name().to_owned());
        info!(
            device = name,
            sample_rate,
            synth_sample_rate = synth_rate,
            channels,
            voices = MAX_VOICES,
            queue_slots = QUEUE_SLOTS,
            "audio device open"
        );
        Self {
            queue,
            counters,
            _stream: Some(stream),
            sample_rate,
            channels,
            open: true,
        }
    }

    /// Asks for a sound. Silently and cheaply does nothing with no device.
    pub fn play(&self, params: VoiceParams) {
        if self.open {
            self.queue.push(params);
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.channels
    }

    #[must_use]
    pub fn queue(&self) -> &SoundQueue {
        &self.queue
    }

    #[must_use]
    pub fn counters(&self) -> &AudioCounters {
        &self.counters
    }
}

#[cfg(test)]
mod tests {
    use super::{QUEUE_SLOTS, SoundQueue};
    use crate::synth::{VoiceKind, VoiceParams};

    fn hit(intensity: f32) -> VoiceParams {
        VoiceParams::new(VoiceKind::Hit, intensity, 0.5)
    }

    #[test]
    fn an_empty_queue_has_nothing_to_give() {
        let queue = SoundQueue::default();
        assert!(queue.pop().is_none());
        assert_eq!(queue.waiting(), 0);
        assert_eq!(queue.dropped(), 0);
        assert_eq!(queue.consumed(), 0);
    }

    #[test]
    fn what_goes_in_comes_out_in_order() {
        let queue = SoundQueue::default();
        let sent: Vec<VoiceParams> = (0..10)
            .map(|index| hit(f32::from(u8::try_from(index).unwrap_or(0)) / 10.0))
            .collect();
        for params in &sent {
            assert!(queue.push(*params));
        }
        assert_eq!(queue.waiting(), 10);
        for expected in &sent {
            let got = match queue.pop() {
                Some(got) => got,
                None => panic!("the queue lost a request"),
            };
            assert_eq!(got.kind, expected.kind);
            assert!((got.intensity - expected.intensity).abs() < 0.01);
        }
        assert!(queue.pop().is_none());
        assert_eq!(queue.consumed(), 10);
        assert_eq!(queue.dropped(), 0);
    }

    #[test]
    fn a_full_queue_refuses_and_counts_rather_than_blocking() {
        // The bound, as a property. A stalled audio thread must cost sounds,
        // never a frame.
        let queue = SoundQueue::default();
        let mut taken = 0;
        for _ in 0..QUEUE_SLOTS * 4 {
            if queue.push(hit(1.0)) {
                taken += 1;
            }
        }
        assert!(taken < QUEUE_SLOTS * 4, "the queue never filled");
        assert_eq!(queue.dropped(), (QUEUE_SLOTS * 4 - taken) as u64);
        // And it recovers: draining makes room again.
        for _ in 0..taken {
            assert!(queue.pop().is_some());
        }
        assert!(queue.push(hit(1.0)), "a drained queue is still full");
    }

    #[test]
    fn the_ring_survives_wrapping_round_many_times() {
        // The indices are monotonic `u32`s masked into the array, so the only
        // way this breaks is at a wrap, and a wrap is thousands of hits away in
        // a real fight and one loop away here.
        let queue = SoundQueue::default();
        for round in 0..QUEUE_SLOTS * 40 {
            let params = hit(f32::from(u8::try_from(round % 100).unwrap_or(0)) / 100.0);
            assert!(queue.push(params), "round {round} was refused");
            let got = match queue.pop() {
                Some(got) => got,
                None => panic!("round {round} lost its request"),
            };
            assert!((got.intensity - params.intensity).abs() < 0.01);
        }
        assert_eq!(queue.dropped(), 0);
        assert_eq!(queue.waiting(), 0);
    }

    #[test]
    fn a_producer_and_a_consumer_on_two_threads_lose_nothing() {
        // The structure's whole point, exercised the way it is used: one thread
        // pushing while another pops. What is popped must be a prefix of what
        // was pushed, and nothing may be invented.
        use std::sync::Arc;
        use std::thread;

        let queue = Arc::new(SoundQueue::default());
        let producer = Arc::clone(&queue);
        let total = 20_000_u32;
        let writer = thread::spawn(move || {
            let mut sent = 0_u32;
            while sent < total {
                if producer.push(hit(f32::from(u8::try_from(sent % 128).unwrap_or(0)) / 127.0)) {
                    sent += 1;
                } else {
                    std::thread::yield_now();
                }
            }
        });
        let mut received = 0_u32;
        while received < total {
            if queue.pop().is_some() {
                received += 1;
            } else {
                std::thread::yield_now();
            }
        }
        match writer.join() {
            Ok(()) => {}
            Err(_) => panic!("the producer thread panicked"),
        }
        assert_eq!(received, total);
        assert_eq!(queue.consumed(), u64::from(total));
        assert!(queue.pop().is_none(), "the queue invented a request");
    }

    #[test]
    fn a_slot_that_is_not_a_kind_is_refused_rather_than_guessed_at() {
        // `pop` unpacks, and an unpack can fail. It returns `None` rather than
        // a default sound, so a corrupt word is silence and not a random noise.
        assert!(VoiceParams::unpack(0b10).is_none());
        let queue = SoundQueue::default();
        assert!(queue.push(hit(0.5)));
        assert!(queue.pop().is_some());
    }
}
