//! Encode WAV/FLAC, decode anything people drop on the Clone screen.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::dsp::Pcm;
use crate::error::{ErrorCode, MelError, MelResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Wav,
    Flac,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "wav" => Some(Self::Wav),
            "flac" => Some(Self::Flac),
            _ => None,
        }
    }
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

pub fn write_wav(path: &Path, pcm: &Pcm) -> MelResult<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: pcm.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let tmp = path.with_extension("wav.tmp");
    {
        let mut w = hound::WavWriter::create(&tmp, spec).map_err(MelError::internal)?;
        for &s in &pcm.samples {
            w.write_sample(to_i16(s)).map_err(MelError::internal)?;
        }
        w.finalize().map_err(MelError::internal)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn wav_bytes(pcm: &Pcm) -> MelResult<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: pcm.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec).map_err(MelError::internal)?;
        for &s in &pcm.samples {
            w.write_sample(to_i16(s)).map_err(MelError::internal)?;
        }
        w.finalize().map_err(MelError::internal)?;
    }
    Ok(cursor.into_inner())
}

pub fn flac_bytes(pcm: &Pcm) -> MelResult<Vec<u8>> {
    use flacenc::bitsink::ByteSink;
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let samples: Vec<i32> = pcm.samples.iter().map(|s| to_i16(*s) as i32).collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| MelError::internal(format!("{e:?}")))?;
    let source =
        flacenc::source::MemSource::from_samples(&samples, 1, 16, pcm.sample_rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| MelError::internal(format!("{e:?}")))?;
    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| MelError::internal(format!("{e:?}")))?;
    Ok(sink.as_slice().to_vec())
}

/// Save in the requested format (atomic write).
pub fn save(path: &Path, pcm: &Pcm, format: Format) -> MelResult<()> {
    match format {
        Format::Wav => write_wav(path, pcm),
        Format::Flac => {
            let bytes = flac_bytes(pcm)?;
            let tmp = path.with_extension("flac.tmp");
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(&tmp, path)?;
            Ok(())
        }
    }
}

pub fn read_wav(path: &Path) -> MelResult<Pcm> {
    decode_file(path)
}

fn bad_audio() -> MelError {
    MelError::new(
        ErrorCode::BadAudio,
        "couldn't read that file — try a wav, mp3, m4a or flac recording",
    )
}

/// Decode wav/mp3/flac/m4a/ogg into mono f32 PCM at its native rate.
pub fn decode_file(path: &Path) -> MelResult<Pcm> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).map_err(|e| bad_audio().with_detail(e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| bad_audio().with_detail(e))?;
    let mut format = probed.format;
    let track = format.default_track().ok_or_else(bad_audio)?.clone();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| bad_audio().with_detail(e))?;
    let rate = track.codec_params.sample_rate.ok_or_else(bad_audio)?;
    let mut mono = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => return Err(bad_audio().with_detail(e)),
        };
        if packet.track_id() != track.id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(bad_audio().with_detail(e)),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        for frame in buf.samples().chunks(channels) {
            mono.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }
    if mono.is_empty() {
        return Err(bad_audio());
    }
    Ok(Pcm::new(mono, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone() -> Pcm {
        Pcm::new(
            (0..24_000).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect(),
            24_000,
        )
    }

    #[test]
    fn wav_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        write_wav(&p, &tone()).unwrap();
        let back = decode_file(&p).unwrap();
        assert_eq!(back.sample_rate, 24_000);
        assert_eq!(back.samples.len(), 24_000);
        assert!((back.samples[100] - tone().samples[100]).abs() < 0.001);
    }

    #[test]
    fn flac_writes_valid_stream() {
        // Output verified against libsndfile (reads all 24 000 samples); here we
        // check the stream header and STREAMINFO sample rate.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.flac");
        save(&p, &tone(), Format::Flac).unwrap();
        let raw = std::fs::read(&p).unwrap();
        assert_eq!(&raw[..4], b"fLaC");
        let rate = ((raw[18] as u32) << 12) | ((raw[19] as u32) << 4) | ((raw[20] as u32) >> 4);
        assert_eq!(rate, 24_000);
        assert!(raw.len() < 24_000 * 2, "should compress");
    }

    #[test]
    fn garbage_is_a_human_error() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.mp3");
        std::fs::write(&p, b"not audio at all").unwrap();
        let e = decode_file(&p).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadAudio);
    }
}
