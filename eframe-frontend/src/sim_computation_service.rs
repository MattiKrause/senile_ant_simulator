use std::fmt::{Display, Formatter};
use ant_sim::ant_sim::AntSimulator;
use async_std::channel::{Sender as ChannelSender, Receiver as ChannelReceiver};
use crate::AntSimFrame;
use crate::service_handle::{SenderDiedError, ServiceHandle};

use async_trait::async_trait;

pub struct SimComputeMessage(pub Box<AntSimulator<AntSimFrame>>, pub Box<AntSimulator<AntSimFrame>>);
pub struct SimComputationFinished(pub Box<AntSimulator<AntSimFrame>>, pub Box<AntSimulator<AntSimFrame>>);
pub struct SimStepComputationService {
    task_q: ChannelSender<SimComputeMessage>
}

enum WorkerError<S: ServiceHandle<SimComputationFinished>> {
    QueueDied, SenderFailed(S::Err)
}

impl <S: ServiceHandle<SimComputationFinished>> Display for WorkerError<S> where S::Err: Display{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerError::QueueDied => write!(f, "job queue died"),
            WorkerError::SenderFailed(err) => write!(f, "sender died: {err}")
        }
    }
}

impl SimStepComputationService {
    pub fn new<S>(service_handle: S) -> Result<Self, String> where S: 'static + Send + ServiceHandle<SimComputationFinished>, S::Err: 'static + Send + Display {
        let task_q = async_std::channel::unbounded();
        let task = async {
            let err = Self::task_worker(task_q.1, service_handle).await;
            match err {
                Ok(()) => log::debug!(target: "SimStepComputationService", "SimStepComputationService finished without error"),
                Err(err) => log::debug!(target: "SimStepComputatiobService", "SimStepComputationService finished failed: {err}")
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        async_std::task::spawn(task);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(task);
        let result= Self {
            task_q: task_q.0,
        };
        Ok(result)
    }

    async fn task_worker<S>(rec: ChannelReceiver<SimComputeMessage>, mut send_to: S) -> Result<(), WorkerError<S>> where S: 'static + Send + ServiceHandle<SimComputationFinished>, S::Err: 'static + Send + Display {
        loop {
            let mut job = rec.recv().await.map_err(|_| WorkerError::QueueDied)?;
            job.0.update(job.1.as_mut());
            send_to = send_to.send(SimComputationFinished(job.0, job.1)).await
                .map_err(|(_, err)| {
                    WorkerError::SenderFailed(err)
                })?;
        }
    }
}

#[async_trait]
impl ServiceHandle<SimComputeMessage> for SimStepComputationService {
    type Err = SenderDiedError;

    async fn send(mut self, t: SimComputeMessage) -> Result<Self, (SimComputeMessage, Self::Err)> {
        let send_err = match ServiceHandle::send(self.task_q, t).await {
            Ok(send) => {
                self.task_q = send;
                return Ok(self)
            },
            Err(err) => err,
        };
        Err((send_err.0, SenderDiedError))
    }

    fn try_send(mut self, t: SimComputeMessage) -> Result<(Self, Option<SimComputeMessage>), (SimComputeMessage, Self::Err)> {
        let send_err = match ServiceHandle::try_send(self.task_q, t) {
            Ok((sender, m)) => {
                self.task_q = sender;
                return Ok((self, m))
            },
            Err(err) => err,
        };
        Err((send_err.0, SenderDiedError))
    }
}