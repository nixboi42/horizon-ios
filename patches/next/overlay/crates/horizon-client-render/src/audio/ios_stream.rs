//! Bounded incremental Vorbis playback. The worker owns the file and codec;
//! the callback only interpolates PCM already in the ring, never waits for it.
#[cfg(target_os = "ios")]
use super::{ChannelId, Mixer, State};
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

const FRAMES: usize = 32_768;
struct Ring {
    samples: VecDeque<f32>,
    channels: usize,
    done: bool,
}
struct Shared {
    ring: Mutex<Ring>,
    cancelled: AtomicBool,
}
pub(super) struct Playback {
    shared: Arc<Shared>,
    phase: f64,
}
impl Drop for Playback {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Release);
    }
}
impl Playback {
    fn new(channels: usize) -> Self {
        Self {
            shared: Arc::new(Shared {
                ring: Mutex::new(Ring {
                    samples: VecDeque::with_capacity(FRAMES * channels),
                    channels,
                    done: false,
                }),
                cancelled: AtomicBool::new(false),
            }),
            phase: 0.0,
        }
    }
    /// Returns true only after EOF and the last queued frame. Underruns emit
    /// silence and preserve phase; pause doesn't call this, so it holds position.
    pub(super) fn mix(
        &mut self,
        out: &mut [f32],
        channels: usize,
        step: f64,
        gains: [f32; 3],
    ) -> bool {
        let Ok(mut ring) = self.shared.ring.try_lock() else {
            return false;
        };
        let n = ring.channels;
        for frame in out.chunks_mut(channels) {
            while self.phase >= 1.0 && ring.samples.len() >= n {
                for _ in 0..n {
                    ring.samples.pop_front();
                }
                self.phase -= 1.0;
            }
            if ring.samples.len() < n * 2 {
                if !ring.done {
                    return false;
                }
                if ring.samples.len() < n {
                    return true;
                }
            }
            let t = self.phase as f32;
            let sample = |ch: usize| {
                let a = ring.samples[ch];
                let b = ring.samples.get(n + ch).copied().unwrap_or(a);
                a + (b - a) * t
            };
            let l = sample(0);
            let r = sample(n - 1);
            if channels >= 2 {
                frame[0] += l * gains[0];
                frame[1] += r * gains[1];
            } else {
                frame[0] += (l + r) * 0.5 * gains[2];
            }
            self.phase += step;
        }
        false
    }
}

#[cfg(target_os = "ios")]
pub(super) fn start(
    mixer: &Arc<Mutex<Mixer>>,
    channel: ChannelId,
    file: std::path::PathBuf,
    looping: bool,
) {
    let playback = Playback::new(2);
    let shared = playback.shared.clone();
    let weak = Arc::downgrade(mixer);
    if let Ok(mut m) = mixer.lock() {
        if m.channels.values().filter(|c| c.stream.is_some()).count() >= 4 {
            if let Some(c) = m.channels.get_mut(&channel) {
                c.state = State::Stopped;
            }
            return;
        }
        let Some(c) = m.channels.get_mut(&channel) else {
            return;
        };
        c.stream = Some(playback);
    }
    let result = std::thread::Builder::new()
        .name("ios music decoder".into())
        .spawn(move || {
            let mut started = false;
            loop {
                let ok = decode_pass(&file, &shared, |rate, channels| {
                    if started {
                        return;
                    }
                    started = true;
                    if let Some(mixer) = weak.upgrade()
                        && let Ok(mut m) = mixer.lock()
                        && let Some(c) = m.channels.get_mut(&channel)
                        && c.state == State::Initial
                    {
                        // The mixer uses this metadata for pitch, spatialization and gains.
                        c.samples = Some((Arc::new(Vec::new()), rate, channels));
                        c.state = State::Playing;
                    }
                });
                if !ok || !looping || shared.cancelled.load(Ordering::Acquire) {
                    break;
                }
            }
            if let Ok(mut r) = shared.ring.lock() {
                r.done = true;
            }
            if !started
                && let Some(mixer) = weak.upgrade()
                && let Ok(mut m) = mixer.lock()
                && let Some(c) = m.channels.get_mut(&channel)
            {
                c.state = State::Stopped;
            }
        });
    if result.is_err()
        && let Ok(mut m) = mixer.lock()
        && let Some(c) = m.channels.get_mut(&channel)
    {
        c.stream = None;
        c.state = State::Stopped;
    }
}

#[cfg(any(target_os = "ios", test))]
fn decode_pass(path: &std::path::Path, shared: &Shared, mut ready: impl FnMut(u32, u16)) -> bool {
    use symphonia::core::{
        audio::SampleBuffer, codecs::DecoderOptions, formats::FormatOptions, io::MediaSourceStream,
        meta::MetadataOptions, probe::Hint,
    };
    let run = || -> Option<bool> {
        let file = std::fs::File::open(path).ok()?;
        let mut hint = Hint::new();
        hint.with_extension("ogg");
        let mut format = symphonia::default::get_probe()
            .format(
                &hint,
                MediaSourceStream::new(Box::new(file), Default::default()),
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .ok()?
            .format;
        let track = format.default_track()?;
        let id = track.id;
        let rate = track.codec_params.sample_rate?;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .ok()?;
        let mut any = false;
        while !shared.cancelled.load(Ordering::Acquire) {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(_) => break,
            };
            if packet.track_id() != id {
                continue;
            }
            let decoded = match decoder.decode(&packet) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let spec = *decoded.spec();
            let input_channels = spec.channels.count();
            if input_channels == 0 || spec.rate != rate {
                return None;
            }
            let n = input_channels.min(2);
            let mut pcm = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            pcm.copy_interleaved_ref(decoded);
            if pcm.samples().is_empty() {
                continue;
            }
            let mut offset = 0;
            while offset < pcm.samples().len() {
                if shared.cancelled.load(Ordering::Acquire) {
                    return Some(false);
                }
                {
                    let mut ring = shared.ring.lock().ok()?;
                    ring.channels = n;
                    let room = FRAMES.saturating_sub(ring.samples.len() / n);
                    let frames = room.min((pcm.samples().len() - offset) / input_channels);
                    for frame in pcm.samples()[offset..offset + frames * input_channels]
                        .chunks(input_channels)
                    {
                        if input_channels <= 2 {
                            ring.samples.extend(frame);
                        } else {
                            let mut sums = [0.0; 2];
                            let mut counts = [0; 2];
                            for (i, value) in frame.iter().enumerate() {
                                sums[i % 2] += value;
                                counts[i % 2] += 1;
                            }
                            ring.samples.push_back(sums[0] / counts[0] as f32);
                            ring.samples.push_back(sums[1] / counts[1] as f32);
                        }
                    }
                    offset += frames * input_channels;
                }
                if !any {
                    ready(rate, n as u16);
                    any = true;
                }
                if offset < pcm.samples().len() {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
        Some(any)
    };
    let mut run = run;
    run().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_interpolates_and_drains_final_frame() {
        let mut p = Playback::new(1);
        {
            let mut r = p.shared.ring.lock().unwrap();
            r.samples.extend([0.0, 1.0, 0.0]);
            r.done = true;
        }
        let mut out = [0.0; 8];
        assert!(!p.mix(&mut out, 2, 0.5, [0.25, 0.75, 1.0]));
        assert_eq!(out, [0.0, 0.0, 0.125, 0.375, 0.25, 0.75, 0.125, 0.375]);
        assert!(p.mix(&mut [0.0; 8], 2, 1.0, [1.0; 3]));
    }
    #[test]
    fn underrun_resumes_without_discarding_last_sample() {
        let mut p = Playback::new(2);
        p.shared.ring.lock().unwrap().samples.extend([1.0, -1.0]);
        let mut out = [0.0; 2];
        assert!(!p.mix(&mut out, 2, 1.0, [1.0; 3]));
        assert_eq!(out, [0.0; 2]);
        p.shared.ring.lock().unwrap().samples.extend([0.0, 0.0]);
        assert!(!p.mix(&mut out, 2, 1.0, [1.0; 3]));
        assert_eq!(out, [1.0, -1.0]);
        let shared = p.shared.clone();
        drop(p);
        assert!(shared.cancelled.load(Ordering::Acquire));
    }
    #[test]
    #[ignore = "requires HORIZON_TEST_AUDIO pointing at a synthetic 3+ second Ogg Vorbis file"]
    fn vorbis_worker_is_bounded_and_cancellable() {
        let path = std::path::PathBuf::from(
            std::env::var_os("HORIZON_TEST_AUDIO").expect("synthetic fixture"),
        );
        let playback = Playback::new(2);
        let shared = playback.shared.clone();
        let worker_shared = shared.clone();
        let (send, recv) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            decode_pass(&path, &worker_shared, |rate, channels| {
                send.send((rate, channels)).unwrap();
            })
        });
        assert_eq!(
            recv.recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            (48000, 2)
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
        {
            let r = shared.ring.lock().unwrap();
            assert_eq!(r.samples.len(), FRAMES * 2);
            assert!(r.samples.iter().any(|s| s.abs() > 0.01));
        }
        assert!(
            !worker.is_finished(),
            "decoder must wait when the bounded ring is full"
        );
        drop(playback);
        assert!(
            !worker.join().unwrap(),
            "dropping a voice cancels the file worker"
        );
    }
}
