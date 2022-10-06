use std::fmt::{Display, Formatter};
use std::future::Future;
use crate::service_handle::{SenderDiedError, ServiceHandle};
use async_std::channel::{Sender as ChannelSender, Receiver as ChannelReceiver};
use async_trait::async_trait;

pub struct ChannelActor<M: 'static + Send> {
    pub task_q: ChannelSender<M>,
}

#[cfg(not(target_arch = "wasm32"))]
pub trait ConditionalSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl <T: Send> ConditionalSend for T {}
#[cfg(target_arch = "wasm32")]
pub trait ConditionalSend {}
#[cfg(target_arch = "wasm32")]
impl <T> ConditionalSend for T {}

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

pub trait ChannelActorFUNResult {
    type Res<M: 'static + Send>;
    type Fut;

    fn res_from<M: 'static + Send>(a: ChannelActor<M>) -> Self::Res<M>;
    fn to_fun<M: 'static + Send>(self) -> Result<Self::Fut, Self::Res<M>>;
}

/// Work-around for implementing ChannelActorFUNResult for Result, otherwise conflicting implementation error
pub enum ServiceCreateResult<O, E> {
    Ok(O),
    Err(E),
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


impl<M: 'static + Send> ChannelActor<M> {
    pub fn new_actor<S, SM, FU, FuErr, FUN, FunErr>(name: &'static str, send_to: S, f: FUN) -> FunErr::Res<M>
        where S: 'static + Send + ServiceHandle<SM>,
              SM: 'static + Send,
              S::Err: 'static + Send + Display,
              FU: 'static + ConditionalSend + Future<Output=Result<(), FuErr>>,
              FuErr: 'static + Display,
              FUN: 'static + FnOnce(ChannelReceiver<M>, S, ChannelSender<M>) -> FunErr,
              FunErr: ChannelActorFUNResult<Fut = FU>
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
        FunErr::res_from(result)
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