//! Small, seekable AVI 1.0 writer: MJPEG video and optional interleaved PCM16 audio.
use crate::model::Result;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

pub struct Audio {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}
impl Audio {
    pub fn read(path: &Path) -> Result<Self> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        if len > 128 * 1024 * 1024 {
            return Err("WAV music must be smaller than 128 MiB".into());
        }
        let mut bytes = Vec::with_capacity(len as usize);
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err("Select a PCM 16-bit WAV audio file".into());
        }
        let mut format = None;
        let mut data = None;
        let mut at = 12usize;
        while at + 8 <= bytes.len() {
            let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            let end = at.checked_add(8 + size).ok_or("Invalid WAV size")?;
            if end > bytes.len() {
                return Err("Truncated WAV file".into());
            }
            if &bytes[at..at + 4] == b"fmt " && size >= 16 {
                let s = &bytes[at + 8..end];
                let code = u16::from_le_bytes(s[0..2].try_into().unwrap());
                let channels = u16::from_le_bytes(s[2..4].try_into().unwrap());
                let rate = u32::from_le_bytes(s[4..8].try_into().unwrap());
                let align = u16::from_le_bytes(s[12..14].try_into().unwrap());
                let bits = u16::from_le_bytes(s[14..16].try_into().unwrap());
                if code != 1
                    || bits != 16
                    || !(1..=2).contains(&channels)
                    || !(8000..=96000).contains(&rate)
                    || align != channels * 2
                {
                    return Err(
                        "Music requires uncompressed PCM 16-bit mono/stereo WAV (8–96 kHz)".into(),
                    );
                }
                format = Some((rate, channels));
            } else if &bytes[at..at + 4] == b"data" {
                data = Some(at + 8..end);
            }
            at = end + (size & 1);
        }
        let (rate, channels) = format.ok_or("Missing WAV format")?;
        let data = &bytes[data.ok_or("Missing WAV samples")?];
        if data.is_empty() || data.len() % (channels as usize * 2) != 0 {
            return Err("Empty or misaligned WAV samples".into());
        }
        let samples = data
            .chunks_exact(2)
            .map(|s| i16::from_le_bytes([s[0], s[1]]))
            .collect();
        Ok(Self {
            rate,
            channels,
            samples,
        })
    }
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

pub struct Avi<W: Write + Seek> {
    w: W,
    movi: u64,
    riff_size: u64,
    index: Vec<([u8; 4], u32, u32)>,
    frames: u32,
}
fn io<T>(r: std::io::Result<T>) -> Result<T> {
    r.map_err(|e| e.to_string())
}
fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
impl<W: Write + Seek> Avi<W> {
    fn pos(&mut self) -> Result<u64> {
        io(self.w.stream_position())
    }
    fn raw(&mut self, b: &[u8]) -> Result<()> {
        io(self.w.write_all(b))
    }
    fn chunk(&mut self, id: &[u8; 4], data: &[u8]) -> Result<()> {
        self.raw(id)?;
        self.raw(&(data.len() as u32).to_le_bytes())?;
        self.raw(data)?;
        if data.len() % 2 == 1 {
            self.raw(&[0])?;
        }
        Ok(())
    }
    fn list(&mut self, id: &[u8; 4]) -> Result<u64> {
        self.raw(b"LIST")?;
        let p = self.pos()?;
        self.raw(&0u32.to_le_bytes())?;
        self.raw(id)?;
        Ok(p)
    }
    fn finish_list(&mut self, pos: u64) -> Result<()> {
        let end = self.pos()?;
        io(self.w.seek(SeekFrom::Start(pos)))?;
        self.raw(&((end - pos - 4) as u32).to_le_bytes())?;
        io(self.w.seek(SeekFrom::Start(end)))?;
        Ok(())
    }
    pub fn new(
        w: W,
        width: u32,
        height: u32,
        fps: u32,
        frames: u32,
        audio: Option<&Audio>,
    ) -> Result<Self> {
        let mut a = Self {
            w,
            movi: 0,
            riff_size: 4,
            index: vec![],
            frames: 0,
        };
        a.raw(b"RIFF")?;
        a.raw(&0u32.to_le_bytes())?;
        a.raw(b"AVI ")?;
        let hdrl = a.list(b"hdrl")?;
        a.chunk(
            b"avih",
            &u32s(&[
                1_000_000 / fps,
                0,
                0,
                0x10,
                frames,
                0,
                if audio.is_some() { 2 } else { 1 },
                width * height * 3,
                width,
                height,
                0,
                0,
                0,
                0,
            ]),
        )?;
        let video = a.list(b"strl")?;
        let mut strh = Vec::from(*b"vidsMJPG");
        strh.extend(u32s(&[
            0,
            0,
            0,
            1,
            fps,
            0,
            frames,
            width * height * 3,
            u32::MAX,
            0,
        ]));
        for v in [0u16, 0, width as u16, height as u16] {
            strh.extend(v.to_le_bytes());
        }
        a.chunk(b"strh", &strh)?;
        let mut strf = u32s(&[40, width, height]);
        strf.extend([1, 0, 24, 0]);
        strf.extend(b"MJPG");
        strf.extend(u32s(&[width * height * 3, 0, 0, 0, 0]));
        a.chunk(b"strf", &strf)?;
        a.finish_list(video)?;
        if let Some(audio) = audio {
            let list = a.list(b"strl")?;
            let align = u32::from(audio.channels) * 2;
            let samples = (u64::from(frames) * u64::from(audio.rate) / u64::from(fps)) as u32;
            let mut h = Vec::from(*b"auds\0\0\0\0");
            h.extend(u32s(&[
                0,
                0,
                0,
                align,
                audio.rate * align,
                0,
                samples,
                audio.rate * align / fps + align,
                u32::MAX,
                align,
            ]));
            h.extend([0u8; 8]);
            a.chunk(b"strh", &h)?;
            let mut f = vec![];
            f.extend(1u16.to_le_bytes());
            f.extend(audio.channels.to_le_bytes());
            f.extend(u32s(&[audio.rate, audio.rate * align]));
            f.extend((align as u16).to_le_bytes());
            f.extend(16u16.to_le_bytes());
            a.chunk(b"strf", &f)?;
            a.finish_list(list)?;
        }
        a.finish_list(hdrl)?;
        a.movi = a.list(b"movi")?;
        Ok(a)
    }
    pub fn frame(&mut self, jpeg: &[u8], audio: Option<&[u8]>) -> Result<()> {
        for (id, data) in std::iter::once((*b"00dc", jpeg)).chain(audio.map(|b| (*b"01wb", b))) {
            if self.pos()? + data.len() as u64 + (self.index.len() as u64 + 2) * 16 > 2_000_000_000
            {
                return Err("AVI reached the 2 GB safety limit; use a shorter project, lower resolution, or MP4".into());
            }
            let offset = (self.pos()? - (self.movi + 4)) as u32;
            self.chunk(&id, data)?;
            self.index.push((id, offset, data.len() as u32));
        }
        self.frames += 1;
        Ok(())
    }
    pub fn finish(mut self) -> Result<W> {
        self.finish_list(self.movi)?;
        let mut index = Vec::with_capacity(self.index.len() * 16);
        for (id, offset, len) in &self.index {
            index.extend(id);
            index.extend(u32s(&[0x10, *offset, *len]));
        }
        self.chunk(b"idx1", &index)?;
        self.finish_list(self.riff_size)?;
        io(self.w.flush())?;
        Ok(self.w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_soundtrack_fades_before_silence_and_loop_fills() {
        let audio = Audio {
            rate: 8000,
            channels: 1,
            samples: vec![1000; 8000],
        };
        let ending = audio.frame(23, 24, 48, 1.0, false);
        assert_eq!(&ending[ending.len() - 2..], &[0, 0]);
        assert!(audio.frame(24, 24, 48, 1.0, false).iter().all(|b| *b == 0));
        assert!(audio.frame(24, 24, 48, 1.0, true).iter().any(|b| *b != 0));
    }
    #[test]
    fn riff_sizes_indexes_and_pcm_are_consistent() {
        let audio = Audio {
            rate: 8000,
            channels: 1,
            samples: vec![1000; 8000],
        };
        let mut avi = Avi::new(
            std::io::Cursor::new(Vec::new()),
            16,
            16,
            24,
            2,
            Some(&audio),
        )
        .unwrap();
        for n in 0..2 {
            avi.frame(&[1, 2, 3], Some(&audio.frame(n, 24, 2, 1.0, false)))
                .unwrap();
        }
        let bytes = avi.finish().unwrap().into_inner();
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
            bytes.len() - 8
        );
        let movi = bytes.windows(4).position(|v| v == b"movi").unwrap();
        let idx = bytes.windows(4).position(|v| v == b"idx1").unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[idx + 4..idx + 8].try_into().unwrap()),
            64
        );
        for entry in bytes[idx + 8..].chunks_exact(16) {
            let offset = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            assert_eq!(&bytes[movi + offset..movi + offset + 4], &entry[0..4]);
        }
        assert_eq!(
            audio.frame(0, 24, 2, 1.0, false).len() + audio.frame(1, 24, 2, 1.0, false).len(),
            (8000 * 2 / 24) * 2
        );
    }
}
