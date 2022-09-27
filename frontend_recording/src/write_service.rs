use std::fmt::Display;
use std::sync::mpsc::{Receiver, sync_channel, SyncSender};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use recorder::{BufConsumer, ColorBuffer};

type FrameBuffer = Box<[u8]>;
enum BufWriterError<Err> {
    ChannelDeath,
    ConsumerErr(Err)
}
pub struct RgbaWriteService<B: ColorBuffer, C: for<'b> BufConsumer<Buf<'b> = B::Ref<'b>>>{
    join_handle: JoinHandle<Result<(), (C, BufWriterError<C::Err>)>>,
    buf_q: Receiver<B>,
    job_q: SyncSender<B>,
}

impl <B, C> RgbaWriteService<B, C> where B: ColorBuffer + Send + 'static, C: for <'b> BufConsumer<Buf<'b> = B::Ref<'b>> + Send+ 'static, C::Err: Display + Send + 'static {
    pub fn new(c: C, job_q: usize, buf_size: usize, use_delay: Duration) -> Self {
        let (buf_q_send, buf_q_rec) = sync_channel(job_q);
        let (job_q_send, job_q_rec) = sync_channel(job_q);
        for _ in 0..job_q {
            buf_q_send.send(B::from_pixels(buf_size)).unwrap();
        }
        let join_handle = thread::spawn(move  || {
            let mut c = c;
            let err = Self::consumer_work(
                || job_q_rec.recv().map_err(|_|()),
                |buf| buf_q_send.send(buf).map_err(|_|()),
                &mut c, use_delay
            );
            err.map_err(|err| (c, err))
        });
        Self {
            join_handle,
            buf_q: buf_q_rec,
            job_q: job_q_send
        }
    }

    pub fn queue_frame<'b>(self, frame: &B::Ref<'b>) -> Result<Self, String> {
        if self.join_handle.is_finished() {
            let err = self.join_handle.join()
                .map_err(|err| format!("worker failed unexpectedly: {err:?}"))?
                .map_err(|(_, err)| {
                    match err {
                        BufWriterError::ChannelDeath => format!("lost connection to the worker"),
                        BufWriterError::ConsumerErr(err) => format!("worker failed: {err}")
                    }
                })
                .expect_err("worker crashed with no error");
            return Err(err);
        }
        let result = self.buf_q.recv()
            .map_err(|_|())
            .map(|mut err| {
                err.copy_from_ref(frame);
                err
            })
            .and_then(|buffer| self.job_q.send(buffer).map_err(|_|()));
        match result {
            Ok(_) => Ok(self),
            Err(_) => Err(String::from("worker died without error"))
        }
    }


    fn consumer_work(job_q: impl Fn() -> Result<B, ()>, buf_q: impl Fn(B) -> Result<(), ()>, c: &mut C, delay: Duration,) -> Result<(), BufWriterError<C::Err>> {
        loop {
            let mut job = job_q().map_err(|_| BufWriterError::ChannelDeath)?;
            c.write_buf(job.buf_ref(), delay).map_err(BufWriterError::ConsumerErr)?;
            buf_q(job).map_err(|_| BufWriterError::ChannelDeath)?;
        }
    }
}
