#![feature(generic_associated_types)]
#![allow(stable_features)]

use std::time::Duration;

pub mod gif_recorder;

pub trait BufConsumer {
    type Err;
    type Buf<'a>;
    fn write_buf<'b>(&mut self, buf: Self::Buf<'b>, delay: Duration) -> Result<(), Self::Err>;
}