use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::marker::PhantomData;
use std::ops::ControlFlow;
use std::thread::JoinHandle;
use std::time::Duration;
use async_std::channel::TrySendError;
use async_trait::async_trait;
use async_std::channel::{Sender as ChannelSender, Receiver as ChannelReceiver};

#[async_trait]
pub trait ServiceHandle<T>: Sized {
    type Err: Display;
    async fn send(self, t: T) -> Result<Self, (T, Self::Err)>;
    fn try_send(self, t: T) -> Result<(Self, Option<T>), (T, Self::Err)>;
}

pub struct TransService<T, S: ServiceHandle<T>> {
    backing: S,
    data: PhantomData<std::mem::ManuallyDrop<T>>,
}

pub fn transform<T, S: ServiceHandle<T>>(s: S) -> TransService<T, S> {
    TransService {
        backing: s,
        data: Default::default(),
    }
}


#[async_trait]
impl<F: TryFrom<T> + Send + 'static, T: From<F> + Send, S: ServiceHandle<T> + Send> ServiceHandle<F> for TransService<T, S> where <F as TryFrom<T>>::Error: Debug {
    type Err = S::Err;

    async fn send(self, t: F) -> Result<Self, (F, Self::Err)> {
        self.backing.send(T::from(t))
            .await
            .map_err(|(t, err)| {
                let f = F::try_from(t).expect("sender did not return the value that was send");
                (f, err)
            })
            .map(|backing| Self {
                backing,
                data: Default::default(),
            })
    }

    fn try_send(self, t: F) -> Result<(Self, Option<F>), (F, Self::Err)> {
        self.backing.try_send(T::from(t))
            .map_err(|(t, err)| {
                let f = F::try_from(t).expect("sender did not return the value that was send");
                (f, err)
            })
            .map(|(backing, m)| {
                let new = Self {
                    backing,
                    data: Default::default(),
                };
                let m = m.map(|t| F::try_from(t).expect("sender did not return the value that was send"));
                (new, m)
            })
    }
}

pub struct SenderDiedError;

impl Display for SenderDiedError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "The sender channel died")
    }
}

#[async_trait]
impl<T: 'static + Send> ServiceHandle<T> for async_std::channel::Sender<T> {
    type Err = SenderDiedError;

    async fn send(self, t: T) -> Result<Self, (T, Self::Err)> {
        async_std::channel::Sender::send(&self, t)
            .await
            .map_err(|err| (err.0, SenderDiedError)).map(|_| self)
    }

    fn try_send(self, t: T) -> Result<(Self, Option<T>), (T, Self::Err)> {
        let result = async_std::channel::Sender::try_send(&self, t);
        let err = match result {
            Ok(()) => return Ok((self, None)),
            Err(err) => err
        };
        match err {
            TrySendError::Full(t) => Ok((self, Some(t))),
            TrySendError::Closed(t) => Err((t, SenderDiedError))
        }
    }
}

