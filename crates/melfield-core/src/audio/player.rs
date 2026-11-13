//! Native playback (cpal) with streaming append and a 60 Hz spectrum feed for
//! the dot field. The cpal stream lives on its own thread (streams are !Send
//! on some platforms); everything else talks to shared atomics.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use serde::Serialize;

use super::dsp::{resample, Pcm};
use super::spectrum::{Analyzer, BANDS};
use crate::error::{ErrorCode, MelError, MelResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerFrame {
    pub id: String,
    pub playing: bool,
    pub position_s: f64,
    pub duration_s: f64,
    /// True while more audio may still arrive (streaming generation).
    pub streaming: bool,
    pub bands: Vec<f32>,
}

pub type FrameFn = Arc<dyn Fn(PlayerFrame) + Send + Sync>;

struct Shared {
    samples: Mutex<Vec<f32>>,
    playhead: AtomicUsize,
    playing: AtomicBool,
    input_open: AtomicBool,
    id: Mutex<String>,
    rate: u32,
    channels: usize,
}

pub struct Player {
    shared: Arc<Shared>,
    _keepalive: mpsc::Sender<()>,
}

fn no_output() -> MelError {
    MelError::new(
        ErrorCode::NoAudioOutput,
        "no speakers found — plug in headphones or speakers and try again",
    )
}

impl Player {
    pub fn new(on_frame: FrameFn) -> MelResult<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<MelResult<(u32, usize, Arc<Shared>)>>();
        let (keep_tx, keep_rx) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("melfield-audio-out".into())
            .spawn(move || {
                let built = build_stream();
                match built {
                    Ok((stream, shared)) => {
                        let _ = ready_tx.send(Ok((shared.rate, shared.channels, shared.clone())));
                        let _ = stream.play();
                        // Park until the Player is dropped.
                        let _ = keep_rx.recv();
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(MelError::internal)?;
        let (_, _, shared) = ready_rx.recv().map_err(|_| no_output())??;
        spawn_ticker(shared.clone(), on_frame);
        Ok(Self {
            shared,
            _keepalive: keep_tx,
        })
    }

    pub fn device_rate(&self) -> u32 {
        self.shared.rate
    }

    /// Start a new (possibly streaming) clip. Replaces whatever was playing.
    pub fn begin(&self, id: &str, streaming: bool) {
        self.shared.playing.store(false, Ordering::SeqCst);
        self.shared.samples.lock().clear();
        self.shared.playhead.store(0, Ordering::SeqCst);
        *self.shared.id.lock() = id.to_string();
        self.shared.input_open.store(streaming, Ordering::SeqCst);
        self.shared.playing.store(true, Ordering::SeqCst);
    }

    /// Append audio (any rate; resampled to the device rate).
    pub fn append(&self, pcm: &Pcm) -> MelResult<()> {
        let at_rate = resample(pcm, self.shared.rate)?;
        self.shared
            .samples
            .lock()
            .extend_from_slice(&at_rate.samples);
        Ok(())
    }

    pub fn end_stream(&self) {
        self.shared.input_open.store(false, Ordering::SeqCst);
    }

    pub fn play(&self, id: &str, pcm: &Pcm) -> MelResult<()> {
        self.begin(id, false);
        self.append(pcm)
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        let len = self.shared.samples.lock().len();
        if self.shared.playhead.load(Ordering::SeqCst) >= len
            && !self.shared.input_open.load(Ordering::SeqCst)
        {
            self.shared.playhead.store(0, Ordering::SeqCst);
        }
        self.shared.playing.store(true, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.shared.playing.store(false, Ordering::SeqCst);
        self.shared.input_open.store(false, Ordering::SeqCst);
        self.shared.playhead.store(0, Ordering::SeqCst);
    }

    pub fn seek(&self, seconds: f64) {
        let len = self.shared.samples.lock().len();
        let pos = ((seconds.max(0.0) * self.shared.rate as f64) as usize).min(len);
        self.shared.playhead.store(pos, Ordering::SeqCst);
    }

    pub fn current_id(&self) -> String {
        self.shared.id.lock().clone()
    }
}

fn build_stream() -> MelResult<(cpal::Stream, Arc<Shared>)> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or_else(no_output)?;
    let supported = device
        .default_output_config()
        .map_err(|e| no_output().with_detail(e))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let shared = Arc::new(Shared {
        samples: Mutex::new(Vec::new()),
        playhead: AtomicUsize::new(0),
        playing: AtomicBool::new(false),
        input_open: AtomicBool::new(false),
        id: Mutex::new(String::new()),
        rate: config.sample_rate,
        channels: config.channels as usize,
    });
    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, shared.clone()),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, shared.clone()),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, shared.clone()),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config, shared.clone()),
        other => Err(no_output().with_detail(format!("unsupported sample format {other:?}"))),
    }?;
    Ok((stream, shared))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: Arc<Shared>,
) -> MelResult<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = shared.channels.max(1);
    device
        .build_output_stream(
            *config,
            move |out: &mut [T], _| {
                let playing = shared.playing.load(Ordering::Relaxed);
                // try_lock: never block the realtime thread; output silence instead.
                let guard = if playing {
                    shared.samples.try_lock()
                } else {
                    None
                };
                let mut head = shared.playhead.load(Ordering::Relaxed);
                for frame in out.chunks_mut(channels) {
                    let s = match &guard {
                        Some(buf) if head < buf.len() => {
                            head += 1;
                            buf[head - 1]
                        }
                        _ => 0.0,
                    };
                    for o in frame.iter_mut() {
                        *o = T::from_sample(s);
                    }
                }
                if guard.is_some() {
                    shared.playhead.store(head, Ordering::Relaxed);
                }
            },
            |e| tracing::warn!(error = %e, "audio output error"),
            None,
        )
        .map_err(|e| no_output().with_detail(e))
}

fn spawn_ticker(shared: Arc<Shared>, on_frame: FrameFn) {
    std::thread::Builder::new()
        .name("melfield-spectrum".into())
        .spawn(move || {
            let mut analyzer = Analyzer::new(shared.rate);
            let mut was_active = false;
            loop {
                std::thread::sleep(Duration::from_millis(16));
                let playing = shared.playing.load(Ordering::Relaxed);
                let streaming = shared.input_open.load(Ordering::Relaxed);
                let head = shared.playhead.load(Ordering::Relaxed);
                let (len, bands) = {
                    let buf = shared.samples.lock();
                    let bands = if playing {
                        analyzer.frame(&buf, head)
                    } else {
                        [0.0; BANDS]
                    };
                    (buf.len(), bands)
                };
                let finished = playing && !streaming && head >= len && len > 0;
                if finished {
                    shared.playing.store(false, Ordering::Relaxed);
                }
                let active = playing && !finished;
                if !active && !was_active {
                    continue; // idle: emit nothing
                }
                if !active {
                    analyzer.reset();
                }
                was_active = active;
                on_frame(PlayerFrame {
                    id: shared.id.lock().clone(),
                    playing: active,
                    position_s: head as f64 / shared.rate as f64,
                    duration_s: len as f64 / shared.rate as f64,
                    streaming,
                    bands: if active {
                        bands.to_vec()
                    } else {
                        vec![0.0; BANDS]
                    },
                });
            }
        })
        .expect("spawn spectrum thread");
}
