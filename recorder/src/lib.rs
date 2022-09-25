use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread;
use std::thread::Thread;
use std::time::Duration;

pub mod gif_recorder;

pub trait RgbaConsumer {
    fn write_buf(&mut self, buf: &mut [u8], delay: Duration);
}