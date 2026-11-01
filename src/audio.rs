//! In-process soundtrack decoding. No system codecs, subprocesses or audio device.
use crate::model::Result;
use std::{fs::File, io::ErrorKind, path::Path};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_MP3, CODEC_TYPE_NULL, DecoderOptions},
    errors::Error,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{Limit, MetadataOptions},
    probe::Hint,
};

pub const EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "ogg", "oga", "m4a", "aac", "aif", "aiff",
];
const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;
const FORMAT_HELP: &str = "Choose MP3, WAV, FLAC, Ogg Vorbis, M4A (AAC-LC/ALAC), AAC-LC or AIFF. Protected music and HE-AAC are not supported.";

#[derive(Debug)]
pub struct Audio {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}
impl Audio {
    pub fn read(path: &Path) -> Result<Self> {
        Self::read_with_check(path, || Ok(()))
    }

    pub fn read_with_check(path: &Path, check: impl FnMut() -> Result<()>) -> Result<Self> {
        Self::decode(path, check, MAX_INPUT_BYTES, MAX_DECODED_BYTES)
    }

    fn decode(
        path: &Path,
        mut check: impl FnMut() -> Result<()>,
        input_limit: u64,
        decoded_limit: usize,
    ) -> Result<Self> {
        check()?;
        let file = File::open(path).map_err(|e| format!("Cannot open music: {e}"))?;
        if file.metadata().map_err(|e| e.to_string())?.len() > input_limit {
            return Err("Music files must be at most 128 MiB. Choose a smaller file.".into());
        }
        let source = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(&extension.to_ascii_lowercase());
        }
        let options = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };
        let metadata = MetadataOptions {
            limit_metadata_bytes: Limit::Maximum(1024 * 1024),
            limit_visual_bytes: Limit::Maximum(1024 * 1024),
        };
        let mut format = symphonia::default::get_probe()
            .format(&hint, source, &options, &metadata)
            .map_err(|e| format!("Cannot read this music file: {e}. {FORMAT_HELP}"))?
            .format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| format!("No audio track found. {FORMAT_HELP}"))?;
        let track_id = track.id;
        let codec = track.codec_params.codec;
        let expected_frames = track.codec_params.n_frames;
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions { verify: true })
            .map_err(|e| format!("Cannot decode this music: {e}. {FORMAT_HELP}"))?;
        let mut audio = Self {
            rate: 0,
            channels: 0,
            samples: Vec::new(),
        };
        let mut buffer: Option<SampleBuffer<i16>> = None;
        let mut decoded_end = 0u64;
        loop {
            check()?;
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(format!("Cannot read music; the file may be damaged: {e}")),
            };
            if packet.track_id() != track_id {
                continue;
            }
            let decoded = decoder.decode(&packet).map_err(|e| {
                format!("Cannot decode music; the file may be damaged or unsupported: {e}")
            })?;
            // Container duration and packet timestamps share a time base. MP4's
            // duration need not be expressed in decoded PCM sample frames.
            decoded_end = decoded_end.max(packet.ts().saturating_add(packet.dur()));
            let spec = *decoded.spec();
            let channels = spec.channels.count();
            if !(1..=2).contains(&channels) || !(8000..=96000).contains(&spec.rate) {
                return Err("Music must be mono or stereo, at 8–96 kHz. Convert surround sound to stereo first.".into());
            }
            if audio.rate == 0 {
                audio.rate = spec.rate;
                audio.channels = channels as u16;
            } else if audio.rate != spec.rate || audio.channels as usize != channels {
                return Err("This music changes sample rate or channel count. Convert it to one continuous track first.".into());
            }
            let count = decoded
                .frames()
                .checked_mul(channels)
                .ok_or("Invalid music length")?;
            // Vorbis priming packets can legitimately decode to zero frames.
            // SampleBuffer's interleaved copy assumes a nonempty channel slice.
            if count == 0 {
                continue;
            }
            let total = audio
                .samples
                .len()
                .checked_add(count)
                .ok_or("Invalid music length")?;
            if total > decoded_limit / 2 {
                return Err(
                    "Music exceeds the 256 MiB decoded audio limit. Choose a shorter track.".into(),
                );
            }
            if buffer.as_ref().is_none_or(|b| b.capacity() < count) {
                buffer = Some(SampleBuffer::new(decoded.frames() as u64, spec));
            }
            let buffer = buffer.as_mut().unwrap();
            buffer.copy_interleaved_ref(decoded);
            if audio.samples.capacity() < total {
                let capacity = audio
                    .samples
                    .capacity()
                    .saturating_mul(2)
                    .max(total)
                    .min(decoded_limit / 2);
                audio
                    .samples
                    .try_reserve_exact(capacity - audio.samples.len())
                    .map_err(|_| "Not enough memory to decode this music")?;
            }
            audio.samples.extend_from_slice(buffer.samples());
        }
        check()?;
        if audio.samples.is_empty() {
            return Err("This music contains no decodable audio.".into());
        }
        if decoder.finalize().verify_ok == Some(false) {
            return Err(
                "Music integrity check failed. Choose an undamaged copy of the file.".into(),
            );
        }
        // Untagged MP3 duration is a bitrate estimate and can overestimate VBR.
        if codec != CODEC_TYPE_MP3 && expected_frames.is_some_and(|frames| decoded_end < frames) {
            return Err(
                "Music ended before its declared duration. Choose an undamaged copy of the file."
                    .into(),
            );
        }
        Ok(audio)
    }
}

impl Audio {
    pub fn frame(&self, n: u32, fps: u32, video_frames: u32, volume: f32, repeat: bool) -> Vec<u8> {
        let first = u64::from(n) * u64::from(self.rate) / u64::from(fps);
        let end = u64::from(n + 1) * u64::from(self.rate) / u64::from(fps);
        let total = u64::from(video_frames) * u64::from(self.rate) / u64::from(fps);
        let source_frames = self.samples.len() / self.channels as usize;
        let audible_end = if repeat {
            total
        } else {
            total.min(source_frames as u64)
        };
        let mut bytes = Vec::with_capacity((end - first) as usize * self.channels as usize * 2);
        for f in first..end {
            let fade = ((f as f32 / (self.rate as f32 * 0.5))
                .min(audible_end.saturating_sub(f) as f32 / (self.rate as f32 * 0.5)))
            .clamp(0.0, 1.0);
            for c in 0..self.channels as usize {
                let sample = if repeat || f < (source_frames as u64) {
                    self.samples[(f as usize % source_frames) * self.channels as usize + c]
                } else {
                    0
                };
                bytes.extend_from_slice(
                    &((sample as f32 * volume * fade).round() as i16).to_le_bytes(),
                );
            }
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio")
            .join(name)
    }

    #[test]
    fn decodes_real_music_formats_with_duration_and_stereo_preserved() {
        let pcm = Audio::read(&fixture("tone.wav")).unwrap();
        for name in [
            "tone.mp3",
            "tone.flac",
            "tone.ogg",
            "tone.m4a",
            "tone.aac",
            "tone.aiff",
        ] {
            let audio = Audio::read(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!((audio.rate, audio.channels), (44100, 2), "{name}");
            let seconds = audio.samples.len() as f64 / (audio.rate as f64 * 2.0);
            assert!((seconds - 1.0).abs() < 0.15, "{name}: {seconds}s");
            for (channel, frequency, amplitude) in [(0, 440.0, 12000.0), (1, 660.0, 9000.0)] {
                // Magnitude at the original tone frequency tolerates encoder delay while
                // checking channel order, non-silence and sample conversion independently.
                let samples: Vec<_> = audio
                    .samples
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| s[channel] as f64)
                    .collect();
                let n = samples.len() as f64;
                let phase = std::f64::consts::TAU * frequency / audio.rate as f64;
                let sin: f64 = samples
                    .iter()
                    .enumerate()
                    .map(|(i, s)| s * (phase * i as f64).sin())
                    .sum();
                let cos: f64 = samples
                    .iter()
                    .enumerate()
                    .map(|(i, s)| s * (phase * i as f64).cos())
                    .sum();
                let measured = 2.0 * sin.hypot(cos) / n;
                assert!(
                    (measured / amplitude - 1.0).abs() < 0.25,
                    "{name}, channel {channel}: {measured}"
                );
            }
            if name.ends_with("flac") || name.ends_with("aiff") {
                assert!(audio.samples.len() <= pcm.samples.len(), "{name}");
                let max_error = audio
                    .samples
                    .iter()
                    .zip(&pcm.samples)
                    .map(|(&a, &b)| (a as i32 - b as i32).abs())
                    .max()
                    .unwrap();
                // VLC's encoder pipeline converts through float; allow one LSB.
                assert!(max_error <= 1, "{name}: maximum PCM error {max_error}");
            }
        }
    }

    #[test]
    fn audio_limits_and_cancellation_are_checked_during_decoding() {
        let path = fixture("tone.mp3");
        assert!(
            Audio::decode(&path, || Ok(()), 100, MAX_DECODED_BYTES)
                .unwrap_err()
                .contains("128 MiB")
        );
        assert!(
            Audio::decode(&path, || Ok(()), MAX_INPUT_BYTES, 1000)
                .unwrap_err()
                .contains("256 MiB")
        );
        let mut checks = 0;
        let error = Audio::read_with_check(&path, || {
            checks += 1;
            if checks == 5 {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error, "cancelled");
        assert_eq!(checks, 5);
    }

    #[test]
    fn rejects_empty_unsupported_and_truncated_music() {
        let dir = crate::fixture_dir();
        let path = dir.path().join("broken.mp3");
        for bytes in [b"".as_slice(), b"this is not audio"] {
            std::fs::write(&path, bytes).unwrap();
            assert!(Audio::read(&path).is_err());
        }
        let path = dir.path().join("truncated.wav");
        let mut bytes = std::fs::read(fixture("tone.wav")).unwrap();
        bytes.truncate(bytes.len() / 2);
        std::fs::write(&path, bytes).unwrap();
        assert!(Audio::read(&path).is_err());
    }

    fn write_wav(path: &Path, code: u16, bits: u16, channels: u16, rate: u32, samples: &[u8]) {
        let align = channels * bits / 8;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((36 + samples.len() as u32).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(code.to_le_bytes());
        bytes.extend(channels.to_le_bytes());
        bytes.extend(rate.to_le_bytes());
        bytes.extend((rate * align as u32).to_le_bytes());
        bytes.extend(align.to_le_bytes());
        bytes.extend(bits.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend((samples.len() as u32).to_le_bytes());
        bytes.extend(samples);
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn converts_high_resolution_wav_and_rejects_surround_or_excessive_rate() {
        let dir = crate::fixture_dir();
        let path = dir.path().join("music.wav");
        let samples: Vec<u8> = [-8388608i32, -4194304, 0, 4194304, 8388352]
            .iter()
            .flat_map(|s| s.to_le_bytes()[..3].to_vec())
            .collect();
        write_wav(&path, 1, 24, 1, 48000, &samples);
        let audio = Audio::read(&path).unwrap();
        assert_eq!((audio.rate, audio.channels), (48000, 1));
        assert_eq!(audio.samples, [-32768, -16384, 0, 16384, 32767]);
        let samples: Vec<u8> = [-1.0f32, -0.5, 0.0, 0.5, 1.0]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        write_wav(&path, 3, 32, 1, 48000, &samples);
        assert_eq!(
            Audio::read(&path).unwrap().samples,
            [-32768, -16384, 0, 16384, 32767]
        );
        for (channels, rate) in [(6, 48000), (1, 192000)] {
            write_wav(
                &path,
                1,
                16,
                channels,
                rate,
                &vec![0; channels as usize * 100],
            );
            assert!(Audio::read(&path).unwrap_err().contains("mono or stereo"));
        }
    }
}
