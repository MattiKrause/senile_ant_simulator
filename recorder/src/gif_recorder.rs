use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Duration;
use gif::{EncodingError, Frame};
use crate::{BufConsumer, RgbaBufRef};
use crate::gif_pix_mapping::PIX_MAP;

pub struct GIFRecorder {
    writer: gif::Encoder<File>,
    width: u16,
    height: u16,
    palette: HashMap<[u8; 3], u8>,
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
        let palette = Self::create_palette();
        let mut palette_vec = palette.iter().collect::<Vec<_>>();
        palette_vec.sort_unstable_by_key(|(_, i)| **i);
        let palette_vec = palette_vec.into_iter().flat_map(|(p, _)| *p).collect::<Vec<_>>();
        let enc = gif::Encoder::new(file, width, height, &palette_vec)
            .map_err(|err| match err {
                EncodingError::Format(_) => NewGifRecorderError::FormatErr,
                EncodingError::Io(err) => NewGifRecorderError::FileErr(err)
            })?;
        let rec = Self {
            writer: enc,
            width,
            height,
            palette,
            idx_buffer: vec![0u8; height as usize * width as usize],
        };
        Ok(rec)
    }
    pub fn new_frame(&mut self, frame: impl Iterator<Item=[u8; 3]>, delay: Duration) -> Result<(), GifFrameError> {
        frame.map(|pix| Self::map_to_palette(pix).map_err(|req|(pix, req))).zip(self.idx_buffer.iter_mut())
            .try_for_each(|(i, buf)| i.map(|i| {*buf = i; ()}))
            .unwrap();
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
    fn create_palette() -> HashMap<[u8; 3], u8> {
        let mut res = HashMap::with_capacity(256);
        macro_rules! insert {
            (($r: expr, $g: expr, $b: expr)) => {res.insert([$r, $g, $b], res.len() as u8)};
        }
        insert!((0, 0, 0));
        insert!((0xFF, 0xFF, 0xFF));
        insert!((0xAF, 0xAF, 0xAF));
        insert!((0xFF, 0xFF, 0));
        res.insert(F_ANT, res.len() as u8);
        for i in 0..=(u8::MAX / FOOD_RES) {
            insert!((0, i * FOOD_RES, 0));
        }
        for i in 0..=(u8::MAX / P_RES) {
            for j in 0..=(u8::MAX / P_RES) {
                insert!((i * P_RES, 0, j * P_RES));
            }
        }
        assert!(res.len() <= 256);
        res
    }
    fn map_to_palette(pix: [u8; 3]) -> Result<u8, [u8; 3]> {
        if let Some(i) = PIX_MAP.get(&pix) {
            return Ok(*i);
        }
        if pix[0] > 0 && pix[1] == 0xFF && pix[2] > 0 {
            let f_ant = [0xFF / 2, 0xFF, 0xFF / 2];
            return PIX_MAP.get(&f_ant).copied().ok_or(f_ant);
        }
        if pix[0] == 0 && pix[1] > 0 && pix[2] == 0 {
            let f_pix = [0, (pix[1] / FOOD_RES) * FOOD_RES, 0];
            return PIX_MAP.get(&f_pix).copied().ok_or(f_pix);
        }
        let adj_pix = [(pix[0] / P_RES) * P_RES, 0, (pix[2] / P_RES) * P_RES];
        PIX_MAP.get(&adj_pix).copied().ok_or(adj_pix)
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