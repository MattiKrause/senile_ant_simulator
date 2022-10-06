use std::fmt::{Display};
use ant_sim::ant_sim::AntSimulator;
use crate::AntSimFrame;
use crate::service_handle::{ServiceHandle};

use crate::channel_actor::{ChannelActor, WorkerError};

pub struct SimComputeMessage(pub Box<AntSimulator<AntSimFrame>>, pub Box<AntSimulator<AntSimFrame>>);

pub struct SimComputationFinished(pub Box<AntSimulator<AntSimFrame>>, pub Box<AntSimulator<AntSimFrame>>);

pub type SimComputationService = ChannelActor<SimComputeMessage>;

impl SimComputationService {
    pub fn new<S>(service_handle: S) -> Self
        where
            S: 'static + Send + ServiceHandle<SimComputationFinished>,
            S::Err: 'static + Send + Display
    {
        Self::new_actor::<_, _,_, WorkerError<SimComputationFinished, S>, _, _>("SimComputationService", service_handle, |rec, mut send_to, _| async move {
            loop {
                let mut job = rec.recv().await.map_err(|_| WorkerError::QueueDied)?;
                job.0.update(job.1.as_mut());
                send_to = send_to.send(SimComputationFinished(job.0, job.1)).await
                    .map_err(|(_, err)| {
                        WorkerError::SenderFailed(err)
                    })?;
            }
        })
    }
}