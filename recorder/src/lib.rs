#![feature(generic_associated_types)]

use std::ops::DerefMut;
use std::time::Duration;

pub mod gif_recorder;

pub trait BufConsumer {
    type Err;
    type Buf<'a>;
    fn write_buf<'b>(&mut self, buf: Self::Buf<'b>, delay: Duration) -> Result<(), Self::Err>;
}

pub trait SetRgb {
    fn len(&self) -> usize;
    fn set_rgb(&mut self, index: usize, pix: [u8; 3]);
}

pub trait ColorBuffer {
    type Ref<'a> where Self: 'a;
    fn from_pixels(pixels: usize) -> Self;
    fn buf_ref<'r>(&'r  mut self) -> Self::Ref<'r>;
    fn copy_from_ref<'r>(&mut self, from: &Self::Ref<'r>);
}

#[repr(transparent)]
pub struct RgbaBoxBuf(Box<[u8]>);
#[repr(transparent)]
pub struct RgbaBufRef<'b>(&'b mut [u8]);

impl RgbaBoxBuf {
    pub fn buf_ref(&mut self) -> RgbaBufRef {
        RgbaBufRef(self.0.as_mut())
    }
}

impl TryFrom<Box<[u8]>> for RgbaBoxBuf {
    type Error = Box<[u8]>;

    fn try_from(mut value: Box<[u8]>) -> Result<Self, Self::Error> {
        if RgbaBufRef::try_from(value.as_mut()).is_ok() {
            Ok(RgbaBoxBuf(value))
        } else {
            Err(value)
        }
    }
}

impl ColorBuffer for RgbaBoxBuf {
    type Ref<'a> = RgbaBufRef<'a>;

    fn from_pixels(pixels: usize) -> Self {
        Self(vec![0;pixels * 4].into_boxed_slice())
    }

    fn buf_ref<'r>(&'r mut self) -> Self::Ref<'r> {
        RgbaBufRef(self.0.as_mut())
    }

    fn copy_from_ref<'r>(&mut self, from: &Self::Ref<'r>) {
        self.buf_ref().into_ref().copy_from_slice(from.0);
    }
}

impl <'b> TryFrom<&'b mut [u8]> for RgbaBufRef<'b> {
    type Error = ();

    fn try_from(r: &'b mut [u8]) -> Result<Self, Self::Error> {
        (r.len() % 4 == 0).then(|| Self(r)).ok_or(())
    }
}

impl <'b> RgbaBufRef<'b> {
    pub fn into_ref(self) -> &'b mut [u8] {  
        self.0
    }  
}

impl <'b> SetRgb for RgbaBufRef<'b> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn set_rgb(&mut self, index: usize, rgb: [u8; 3]) {
        let pix = self.0.chunks_exact_mut(4).skip(index).next().unwrap();
        pix[0..3].copy_from_slice(&rgb);
        pix[3] = 0xFF;
    }
}

#[repr(transparent)]
pub struct RgbBoxBuf(Box<[u8]>);
pub struct RgbBufRef<'b>(&'b mut [u8]);

impl <'b1> RgbBufRef<'b1> {
    pub fn to_rgba_full(&self, into: &mut RgbaBufRef, alpha: u8) {
        assert_eq!((into.0.len() / 4) * 3, self.0.len());
        self.0.chunks_exact(3).zip(into.0.chunks_exact_mut(4))
            .for_each(|(rgb, rgba)| {
                rgba[0..3].copy_from_slice(rgb);
                rgba[3] = alpha;
            })
    }
}

impl RgbBoxBuf {
    pub fn buf_ref(&mut self) -> RgbBufRef {
        RgbBufRef(self.0.as_mut())
    }
}

impl TryFrom<Box<[u8]>> for RgbBoxBuf {
    type Error = Box<[u8]>;

    fn try_from(mut value: Box<[u8]>) -> Result<Self, Self::Error> {
        if RgbBufRef::try_from(value.as_mut()).is_ok() {
            Ok(RgbBoxBuf(value))
        } else {
            Err(value)
        }
    }
}

impl ColorBuffer for RgbBoxBuf {
    type Ref<'a> where Self: 'a = RgbBufRef<'a>;

    fn from_pixels(pixels: usize) -> Self {
        Self(vec![0; pixels * 3].into_boxed_slice())
    }

    fn buf_ref<'r>(&'r mut self) -> Self::Ref<'r> {
        self.buf_ref()
    }

    fn copy_from_ref<'r>(&mut self, from: &Self::Ref<'r>) {
        self.buf_ref().0.copy_from_slice(from.0)
    }
}

impl <'b> TryFrom<&'b mut [u8]> for RgbBufRef<'b> {
    type Error = ();

    fn try_from(r: &'b mut [u8]) -> Result<Self, Self::Error> {
        (r.len() % 3 == 0).then(|| Self(r)).ok_or(())
    }
}

impl <'b> RgbBufRef<'b> {
    pub fn into_ref(self) -> &'b mut [u8] {
        self.0
    }
}

impl <'b> SetRgb for RgbBufRef<'b> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn set_rgb(&mut self, index: usize, rgb: [u8; 3]) {
        let pix = self.0.chunks_exact_mut(3).skip(index).next().unwrap();
        pix[0..3].copy_from_slice(&rgb);
    }
}