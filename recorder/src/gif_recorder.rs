use std::fs::File;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use gif::{DisposalMethod, EncodingError, Frame};
use crate::RgbaConsumer;

pub struct GIFRecorder {
    writer: gif::Encoder<File>,
    width: u16, height: u16,
}
#[derive(Debug)]
pub enum NewGifRecorderError {
    FileAlreadyExists, FileErr(std::io::Error), FormatErr
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
    pub fn new_frame(&mut self, frame: &mut [u8], delay: Duration) {
        let mut frame = gif::Frame::from_rgba(self.width, self.height, frame);
        frame.delay = (delay.as_millis() / 10) as u16;
        frame.dispose = DisposalMethod::Keep;
        self.writer.write_frame(&frame).unwrap();
    }
}

impl RgbaConsumer  for GIFRecorder {
    fn write_buf(&mut self, buf: &mut [u8], delay: Duration) {
        self.new_frame(buf, delay);
    }
}