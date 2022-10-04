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

pub enum WorkerError<M, S: ServiceHandle<M>> {
    QueueDied,
    SenderFailed(S::Err),
}

impl<M, S: ServiceHandle<M>> Display for WorkerError<M, S> where S::Err: Display {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerError::QueueDied => write!(f, "job queue died"),
            WorkerError::SenderFailed(err) => write!(f, "sender died: {err}")
        }
    }
}

pub struct ChannelActor<M: 'static + Send> {
    pub task_q: ChannelSender<M>,
}

pub trait ChannelActorFUNResult {
    type Res<M: 'static + Send>;
    type Fut;

    fn res_from<M: 'static + Send>(a: ChannelActor<M>) -> Self::Res<M>;
    fn to_fun<M: 'static + Send>(self) -> Result<Self::Fut, Self::Res<M>>;
}

impl<FC: 'static + Send, F: Future<Output=FC>> ChannelActorFUNResult for F {
    type Res<M: 'static + Send> = ChannelActor<M>;
    type Fut = F;

    fn res_from<M: 'static + Send>(a: ChannelActor<M>) -> Self::Res<M> {
        a
    }

    fn to_fun<M: 'static + Send>(self) -> Result<Self::Fut, Self::Res<M>> {
        Ok(self)
    }
}

/// Work-around for implementing ChannelActorFUNResult for Result, otherwise conflicting implementation error
pub enum ServiceCreateResult<O, E> {
    Ok(O),
    Err(E),
}

impl<O, E> From<Result<O, E>> for ServiceCreateResult<O, E> {
    fn from(r: Result<O, E>) -> Self {
        match r {
            Ok(v) => ServiceCreateResult::Ok(v),
            Err(v) => ServiceCreateResult::Err(v),
        }
    }
}

#[macro_export]
macro_rules! service_err {
    ($result: expr) => {
        match $result {
            ServiceCreateResult::Ok(v) => v,
            ServiceCreateResult::Err(e) => return ServiceCreateResult::Err(e),
        }
    };
}

impl<E, FC: 'static + Send, F: Future<Output=FC>> ChannelActorFUNResult for ServiceCreateResult<F, E> {
    type Res<M: 'static + Send> = Result<ChannelActor<M>, E>;
    type Fut = F;

    fn res_from<M: 'static + Send>(a: ChannelActor<M>) -> Self::Res<M> {
        Ok(a)
    }

    fn to_fun<M: 'static + Send>(self) -> Result<Self::Fut, Self::Res<M>> {
        match self {
            ServiceCreateResult::Ok(v) => Ok(v),
            ServiceCreateResult::Err(e) => Err(Err(e))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub trait ConditionalSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl <T: Send> ConditionalSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait ConditionalSend {}
#[cfg(target_arch = "wasm32")]
impl <T> ConditionalSend for T {}

impl<M: 'static + Send> ChannelActor<M> {
    pub fn new_actor<S, SM, FU, FU_ERR, FUN, FUN_ERR>(name: &'static str, send_to: S, f: FUN) -> FUN_ERR::Res<M>
        where S: 'static + Send + ServiceHandle<SM>,
              SM: 'static + Send,
              S::Err: 'static + Send + Display,
              FU: 'static + ConditionalSend + Future<Output=Result<(), FU_ERR>>,
              FU_ERR: 'static + Display,
              FUN: 'static + FnOnce(ChannelReceiver<M>, S, ChannelSender<M>) -> FUN_ERR,
              FUN_ERR: ChannelActorFUNResult<Fut = FU>
    {
        let task_q = async_std::channel::unbounded();
        let task_send = task_q.0.clone();
        let task = match f(task_q.1, send_to, task_send).to_fun() {
            Ok(f) => f,
            Err(err) => return err,
        };
        let task = async move {
            let err = task.await;
            match err {
                Ok(()) => log::debug!(target: name, "SimStepComputationService finished without error"),
                Err(err) => log::debug!(target: name, "SimStepComputationService finished failed: {err}")
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        async_std::task::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
        let result = Self {
            task_q: task_q.0,
        };
        FUN_ERR::res_from(result)
    }
}


#[async_trait]
impl<M: 'static + Send> ServiceHandle<M> for ChannelActor<M> {
    type Err = SenderDiedError;

    async fn send(mut self, t: M) -> Result<Self, (M, Self::Err)> {
        let send_err = match ServiceHandle::send(self.task_q, t).await {
            Ok(send) => {
                self.task_q = send;
                return Ok(self);
            }
            Err(err) => err,
        };
        Err((send_err.0, SenderDiedError))
    }

    fn try_send(mut self, t: M) -> Result<(Self, Option<M>), (M, Self::Err)> {
        let send_err = match ServiceHandle::try_send(self.task_q, t) {
            Ok((sender, m)) => {
                self.task_q = sender;
                return Ok((self, m));
            }
            Err(err) => err,
        };
        Err((send_err.0, SenderDiedError))
    }
}