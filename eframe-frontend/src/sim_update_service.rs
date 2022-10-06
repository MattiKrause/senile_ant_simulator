use std::fmt::{Display, Formatter};
use std::time::{Duration};
use ant_sim::ant_sim::AntSimulator;
use crate::{AntSimFrame};
use async_std::future::{timeout};
use egui::{Color32, ColorImage};
use ant_sim::ant_sim_frame::AntSim;
use crate::channel_actor::*;
use crate::service_handle::*;
use crate::sim_computation_service::{SimComputationService, SimComputeMessage};
use crate::time_polyfill::*;

pub enum SimUpdaterMessage {
    SetDelay(Duration),
    Pause(bool),
    ImmediateNextFrame,
    NewSim(Box<AntSimulator<AntSimFrame>>),
    RequestCurrentState
}

pub enum SimUpdateServiceMessage {
    NewFrame(egui::ImageData),
    CurrentState(Box<AntSimulator<AntSimFrame>>)
}

pub type SimUpdateService = ChannelActor<SimUpdaterMessage>;


pub enum SimUpdateError<SE: 'static + Send + Display> {
    QueueDied,
    SenderError(SE),
    InternalError(String),
}

impl<SE: 'static + Send + Display> SimUpdateError<SE> {
    fn comp_service_died() -> Self {
        Self::InternalError(String::from("Computation Service died"))
    }
}

impl<SE: 'static + Send + Display> Display for SimUpdateError<SE> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SimUpdateError::QueueDied => write!(f, "task queue died"),
            SimUpdateError::SenderError(err) => write!(f, "sender error: {err}"),
            SimUpdateError::InternalError(err) => write!(f, "internal error: {err}")
        }
    }
}


impl SimUpdateService {
    pub fn new<S>(send_to: S, c: (Duration, Box<AntSimulator<AntSimFrame>>)) -> Result<Self, String>
        where S: 'static + Send + ServiceHandle<SimUpdateServiceMessage>,
              S::Err: 'static + Send + Display,
    {
        let actor = ChannelActor::new_actor::<_, _, _, SimUpdateError<S::Err>, _, _>("SimUpdateService", send_to, move |rec, mut send_to, this| {
            let mut compute_channel = async_std::channel::unbounded();
            let mut  compute = SimComputationService::new(compute_channel.0);
            let mut timer = match Timer::new() {
                Ok(t) => t,
                Err(err) => return ServiceCreateResult::Err(format!("failed to query time: {err}"))
            };

            let task = async move {
                let (mut delay, sim) = c;
                let mut paused = false;
                let mut ignore_updates = 0u32;
                let mut next_scheduled_update = timer.now();
                let mut save_requested = false;
                compute = compute.send(SimComputeMessage(sim.clone(), sim))
                    .await
                    .map_err(|_| SimUpdateError::comp_service_died())?;
                loop {
                    let use_delay = if paused {
                        Duration::MAX
                    } else {
                        timer.saturating_duration_till(&next_scheduled_update)
                    };
                    let mut received = timeout(use_delay, rec.recv()).await;
                    if let Ok(message) = received {
                        let message = message.map_err(|_| SimUpdateError::QueueDied)?;
                        match message {
                            SimUpdaterMessage::SetDelay(new_delay) => {
                                next_scheduled_update = Self::new_scheduled_time(&timer, next_scheduled_update, new_delay, delay);
                                delay = new_delay
                            }
                            SimUpdaterMessage::Pause(new_paused) => paused = new_paused,
                            SimUpdaterMessage::ImmediateNextFrame => next_scheduled_update = timer.now(),
                            SimUpdaterMessage::NewSim(sim) => {
                                compute = compute.send(SimComputeMessage(sim.clone(), sim))
                                    .await
                                    .map_err(|err| SimUpdateError::comp_service_died())?;
                                next_scheduled_update = timer.now();
                                ignore_updates += 1;
                            }
                            SimUpdaterMessage::RequestCurrentState => {
                                save_requested = true;
                            }
                        }
                        continue;
                    }
                    let update = loop {
                        let mut update = compute_channel.1.recv().await.map_err(|_| SimUpdateError::comp_service_died())?;
                        if ignore_updates > 0 {
                            ignore_updates -= 1;
                            continue;
                        } else {
                            break update;
                        }
                    };

                    let image = Self::sim_to_image(update.0.as_ref());
                    next_scheduled_update = timer.now().checked_add(delay).unwrap_or(next_scheduled_update);
                    if save_requested {
                        save_requested = false;
                        send_to = send_to.send(SimUpdateServiceMessage::CurrentState(update.0.clone()))
                            .await
                            .map_err(|(_, err)| SimUpdateError::SenderError(err))?;
                    }
                    send_to = send_to.send(SimUpdateServiceMessage::NewFrame(image))
                        .await
                        .map_err(|(_, err)| SimUpdateError::SenderError(err))?;
                    compute = compute.send(SimComputeMessage(update.1, update.0))
                        .await
                        .map_err(|_| SimUpdateError::comp_service_died())?;
                }
            };
            ServiceCreateResult::Ok(task)
        });
        actor
    }

    fn new_scheduled_time(timer: &Timer, scheduled_time: Time, new_delay: Duration, old_delay: Duration) -> Time {
        if timer.now().before(&scheduled_time) {
            if new_delay > old_delay {
                scheduled_time.checked_add(new_delay - old_delay).unwrap_or(scheduled_time)
            } else {
                scheduled_time.checked_sub(old_delay - new_delay).unwrap_or(scheduled_time)
            }
        } else {
            scheduled_time
        }
    }

    pub fn sim_to_image<A: AntSim>(sim: &AntSimulator<A>) -> egui::ImageData {
        let mut pixels = vec![Color32::BLACK; sim.sim.cell_count()];
        rgba_adapter::draw_to_buf(sim, ImageRgba(&mut pixels));
        let dim = [sim.sim.width(), sim.sim.height()];
        ColorImage { size: dim, pixels }.into()
    }
}


struct ImageRgba<'a>(&'a mut [Color32]);

impl<'a> rgba_adapter::SetRgb for ImageRgba<'a> {
    #[inline(always)]
    fn len(&self) -> usize {
        self.0.len()
    }

    #[inline(always)]
    fn set_rgb(&mut self, index: usize, pix: [u8; 3]) {
        self.0[index] = Color32::from_rgb(pix[0], pix[1], pix[2]);
    }
}