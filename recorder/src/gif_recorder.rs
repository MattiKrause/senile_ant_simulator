use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Duration;
use gif::{EncodingError, Frame};
use crate::{BufConsumer, RgbaBufRef};

pub struct GIFRecorder {
    writer: gif::Encoder<File>,
    width: u16,
    height: u16,
    idx_buffer: Vec<u8>,
}

#[derive(Debug)]
pub enum NewGifRecorderError {
    FileAlreadyExists,
    FileErr(std::io::Error),
    FormatErr,
}

#[derive(Debug)]
pub enum GifFrameError {
    IOError(io::Error),
    FormatErr,
}

const FOOD_RES: u8 = 25;
const P_RES: u8 = 18;
const F_ANT: [u8; 3] = [0xFF / 2, 0xFF, 0xFF / 2];

impl GIFRecorder {
    pub fn new(width: u16, height: u16, file: impl AsRef<Path>, allow_replace: bool) -> Result<Self, NewGifRecorderError> {
        let file = file.as_ref();
        if !allow_replace && file.exists() {
            return Err(NewGifRecorderError::FileAlreadyExists);
        }
        let file = File::options().create_new(!allow_replace).create(true).write(true).open(file).map_err(NewGifRecorderError::FileErr)?;
        let palette_vec = Self::palette_vec();
        let enc = gif::Encoder::new(file, width, height, &palette_vec.into_iter().flat_map(|b|b).collect::<Vec<_>>())
            .map_err(|err| match err {
                EncodingError::Format(_) => NewGifRecorderError::FormatErr,
                EncodingError::Io(err) => NewGifRecorderError::FileErr(err)
            })?;
        let rec = Self {
            writer: enc,
            width,
            height,
            idx_buffer: vec![0u8; height as usize * width as usize],
        };
        Ok(rec)
    }
    pub fn new_frame(&mut self, frame: impl Iterator<Item=[u8; 3]>, delay: Duration) -> Result<(), GifFrameError> {
        frame.map(|pix| Self::map_to_palette_vec(pix)).zip(self.idx_buffer.iter_mut())
            .for_each(|(i, buf)| *buf = i);
        let frame = Frame {
            width: self.width,
            height: self.height,
            delay: (delay.as_millis() / 10) as u16,
            buffer: Cow::Borrowed(&self.idx_buffer),
            ..Frame::default()
        };

        self.writer.write_frame(&frame).map_err(|err| match err {
            EncodingError::Format(_) => GifFrameError::FormatErr,
            EncodingError::Io(err) => GifFrameError::IOError(err),
        })
    }
    fn palette_vec() -> Vec<[u8; 3]> {
        let mut res = Vec::new();
        res.push([0, 0, 0]);
        res.push([0xFF, 0xFF, 0xFF]);
        res.push([0xAF, 0xAF, 0xAF]);
        res.push([0xFF, 0xFF, 0]);
        res.push(F_ANT);
        for i in 0..=(u8::MAX / FOOD_RES) {
            res.push([0, i * FOOD_RES, 0]);
        }
        for i in 0..=(u8::MAX / P_RES) {
            for j in 0..=(u8::MAX / P_RES) {
                res.push([i * P_RES, 0, j * P_RES]);
            }
        }
        res
    }
    fn map_to_palette_vec(pix: [u8; 3]) -> u8 {
        if pix == [0, 0, 0] {
            0
        } else if pix == [0xFF, 0xFF, 0xFF] {
            1
        } else if pix == [0xAF, 0xAF, 0xAF] {
            2
        } else if pix == [0xFF, 0xFF, 0] {
            3
        } else if pix[0] > 0 && pix[1] == 0xFF && pix[2] > 0  {
            4
        } else if pix[0] == 0 && pix[1] > 0 && pix[2] == 0 {
            5 + (pix[1] / FOOD_RES)
        } else {
            5 + (u8::MAX / FOOD_RES + 1) + (pix[0] / P_RES) * (u8::MAX / P_RES + 1) + (pix[2] / P_RES)
        }
    }
}

impl BufConsumer for GIFRecorder {
    type Err = GifFrameError;
    type Buf<'a> = RgbaBufRef<'a>;

    fn write_buf<'b>(&mut self, buf: RgbaBufRef<'b>, delay: Duration) -> Result<(), GifFrameError> {
        let as_rgb = buf.0.chunks_exact(4).map(|chunk| {
            let mut pix = [0u8; 3];
            pix.copy_from_slice(&chunk[0..3]);
            pix
        });
        self.new_frame(as_rgb, delay)
    }
}

impl Display for GifFrameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GifFrameError::IOError(err) => write!(f, "failed to write to target file: {err}"),
            GifFrameError::FormatErr => write!(f, "invalid gif encoding")
        }
    }
}