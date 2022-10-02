use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::thread::JoinHandle;
use std::time::Duration;

pub trait ServiceHandle<T>: Sized {
    type Err: Display;
    fn send(self, t: T) -> Result<Self, (T, Self::Err)>;
}

pub struct TransService<T, S: ServiceHandle<T>> {
    backing: S,
    data: PhantomData<std::mem::ManuallyDrop<T>>,
}

pub fn transform<T, S: ServiceHandle<T>>(s: S) -> TransService<T, S> {
    TransService {
        backing: s,
        data: Default::default()
    }
}


impl <F: TryFrom<T>, T: From<F>, S: ServiceHandle<T>> ServiceHandle<F> for TransService<T, S> where <F as TryFrom<T>>::Error: Debug {
    type Err = S::Err;

    fn send(self, t: F) -> Result<Self, (F, Self::Err)> {
        self.backing.send(T::from(t))
            .map_err(|(t, err)| {
                let f = F::try_from(t).expect("sender did not return the value that was send");
                (f, err)
            })
            .map(|backing| Self {
                backing,
                data: Default::default()
            })
    }
}

pub struct SenderDiedError;
impl Display for SenderDiedError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "The sender channel died")
    }
}

impl <T: 'static + Send> ServiceHandle<T> for std::sync::mpsc::Sender<T> {
    type Err = SenderDiedError;

    fn send(self, t: T) -> Result<Self, (T, Self::Err)> {
        std::sync::mpsc::Sender::send(&self, t).map_err(|err| (err.0, SenderDiedError)).map(|_| self)
    }
}

pub fn join_with_timeout<T: Send + 'static>(join_handle: JoinHandle<T>, timeout: Duration) -> Option<Result<T, Box<dyn core::any::Any + Send + 'static>>> {
    if join_handle.is_finished() {
        Some(join_handle.join())
    } else {
        let channel = std::sync::mpsc::sync_channel(0);
        let _ = std::thread::Builder::new().spawn(move ||{
            channel.0.send(join_handle.join())
        }).ok()?;
        channel.1.recv_timeout(timeout).ok()
    }
}