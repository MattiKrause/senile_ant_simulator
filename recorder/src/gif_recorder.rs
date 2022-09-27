use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Duration;
use gif::{DisposalMethod, EncodingError};
use crate::{BufConsumer, RgbaBufRef};

pub struct GIFRecorder {
    writer: gif::Encoder<File>,
    width: u16, height: u16,
}
#[derive(Debug)]
pub enum NewGifRecorderError {
    FileAlreadyExists, FileErr(std::io::Error), FormatErr
}

#[derive(Debug)]
pub enum GifFrameError {
    IOError(io::Error), FormatErr
}

impl GIFRecorder {
    pub fn new(width: u16, height: u16, file: impl AsRef<Path>, allow_replace: bool) -> Result<Self, NewGifRecorderError> {
        let file = file.as_ref();
        if !allow_replace && file.exists(){
            return Err(NewGifRecorderError::FileAlreadyExists);
        }
        let file = File::options().create_new(!allow_replace).create(true).write(true).open(file).map_err(NewGifRecorderError::FileErr)?;
        let enc = gif::Encoder::new(file, width, height, &[])
            .map_err(|err| match err {
                EncodingError::Format(_) => NewGifRecorderError::FormatErr,
                EncodingError::Io(err) => NewGifRecorderError::FileErr(err)
            })?;
        let rec = Self {
            writer: enc,
            width,
            height
        };
        Ok(rec)
    }
    pub fn new_frame(&mut self, mut frame: RgbaBufRef, delay: Duration) -> Result<(), GifFrameError> {
        let mut frame = gif::Frame::from_rgba_speed(self.width, self.height, &mut  frame.0, 1);
        frame.delay = (delay.as_millis() / 10) as u16;
        frame.dispose = DisposalMethod::Keep;
        self.writer.write_frame(&frame).map_err(|err| match err {
            EncodingError::Format(_) => GifFrameError::FormatErr,
            EncodingError::Io(err) => GifFrameError::IOError(err),
        })
    }
}

impl BufConsumer  for GIFRecorder {
    type Err = GifFrameError;
    type Buf<'a> = RgbaBufRef<'a>;

    fn write_buf<'b>(&mut self, buf: RgbaBufRef<'b>, delay: Duration) -> Result<(), GifFrameError> {
        self.new_frame(buf, delay)
    }
}

impl Display for GifFrameError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GifFrameError::IOError(err) =>  write!(f, "failed to write to target file: {err}"),
            GifFrameError::FormatErr => write!(f, "invalid gif encoding")
        }
    }
}