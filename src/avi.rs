//! Small, seekable AVI 1.0 writer: MJPEG video and optional interleaved PCM16 audio.
use crate::model::Result;
use std::io::{Seek, SeekFrom, Write};

pub use crate::audio::Audio;

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
        for entry in bytes[idx + 8..].as_chunks::<16>().0 {
            let offset = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            assert_eq!(&bytes[movi + offset..movi + offset + 4], &entry[0..4]);
        }
        assert_eq!(
            audio.frame(0, 24, 2, 1.0, false).len() + audio.frame(1, 24, 2, 1.0, false).len(),
            (8000 * 2 / 24) * 2
        );
    }
}
