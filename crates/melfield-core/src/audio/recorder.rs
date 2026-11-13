//! Native microphone capture (cpal). Avoids webview getUserMedia quirks on
//! Linux/macOS. Emits a live level for the record orb.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;

use super::dsp::Pcm;
use crate::error::{ErrorCode, MelError, MelResult};

pub const MAX_SECONDS: f32 = 60.0;

pub type LevelFn = Arc<dyn Fn(f32, f64) + Send + Sync>;

fn no_mic() -> MelError {
    MelError::new(
        ErrorCode::NoMicrophone,
        "couldn't hear a microphone — check it's plugged in and that melfield is allowed to use it",
    )
}

pub struct Recording {
    samples: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    stop: mpsc::Sender<()>,
    done: Arc<AtomicBool>,
}

impl Recording {
    /// Start recording from the default input. `on_level(level 0..1, seconds)` ~30 Hz.
    pub fn start(on_level: LevelFn) -> MelResult<Self> {
        let samples = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<MelResult<u32>>();
        let s2 = samples.clone();
        let d2 = done.clone();
        std::thread::Builder::new()
            .name("melfield-audio-in".into())
            .spawn(move || {
                let built = (|| -> MelResult<(cpal::Stream, u32)> {
                    let device = cpal::default_host()
                        .default_input_device()
                        .ok_or_else(no_mic)?;
                    let supported = device
                        .default_input_config()
                        .map_err(|e| no_mic().with_detail(e))?;
                    let format = supported.sample_format();
                    let config: cpal::StreamConfig = supported.into();
                    let rate = config.sample_rate;
                    let stream = match format {
                        cpal::SampleFormat::F32 => input::<f32>(&device, &config, s2.clone()),
                        cpal::SampleFormat::I16 => input::<i16>(&device, &config, s2.clone()),
                        cpal::SampleFormat::U16 => input::<u16>(&device, &config, s2.clone()),
                        cpal::SampleFormat::I32 => input::<i32>(&device, &config, s2.clone()),
                        other => Err(no_mic().with_detail(format!("{other:?}"))),
                    }?;
                    stream.play().map_err(|e| no_mic().with_detail(e))?;
                    Ok((stream, rate))
                })();
                match built {
                    Ok((stream, rate)) => {
                        let _ = ready_tx.send(Ok(rate));
                        let started = Instant::now();
                        loop {
                            match stop_rx.recv_timeout(Duration::from_millis(33)) {
                                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                            }
                            let secs = started.elapsed().as_secs_f64();
                            let level = {
                                let buf = s2.lock();
                                let tail = &buf[buf.len().saturating_sub(rate as usize / 20)..];
                                if tail.is_empty() {
                                    0.0
                                } else {
                                    (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32)
                                        .sqrt()
                                }
                            };
                            on_level((level * 4.0).min(1.0), secs);
                            if secs as f32 >= MAX_SECONDS {
                                break;
                            }
                        }
                        drop(stream);
                        d2.store(true, Ordering::SeqCst);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(MelError::internal)?;
        let rate = ready_rx.recv().map_err(|_| no_mic())??;
        Ok(Self {
            samples,
            rate,
            stop: stop_tx,
            done,
        })
    }

    pub fn stop(self) -> Pcm {
        let _ = self.stop.send(());
        for _ in 0..50 {
            if self.done.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let samples = std::mem::take(&mut *self.samples.lock());
        Pcm::new(samples, self.rate)
    }
}

fn input<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    out: Arc<Mutex<Vec<f32>>>,
) -> MelResult<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    let cap = (config.sample_rate as f32 * (MAX_SECONDS + 1.0)) as usize;
    device
        .build_input_stream(
            *config,
            move |data: &[T], _| {
                let mut buf = out.lock();
                if buf.len() >= cap {
                    return;
                }
                for frame in data.chunks(channels) {
                    let s: f32 = frame
                        .iter()
                        .map(|x| <f32 as cpal::FromSample<T>>::from_sample_(*x))
                        .sum::<f32>()
                        / channels as f32;
                    buf.push(s);
                }
            },
            |e| tracing::warn!(error = %e, "audio input error"),
            None,
        )
        .map_err(|e| no_mic().with_detail(e))
}
