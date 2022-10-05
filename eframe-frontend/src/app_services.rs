use std::fmt::{Debug, Formatter};
use std::time::Duration;
use crate::load_file_service::{LoadFileResponse, FileParsingError, LoadFileService};
use crate::service_handle::{transform, TransService};
use async_std::channel::{Sender as ChannelSender, Sender};
use ant_sim::ant_sim::AntSimulator;
use crate::AntSimFrame;
use crate::app::AppEvents;
use crate::sim_update_service::{SimUpdateService, SimUpdateServiceMessage};

pub struct Services {
    pub mailbox_in: ChannelSender<AppEvents>,
    pub load_file: Option<LoadFileService>,
    pub update: Option<SimUpdateService>
}

impl Debug for AppEvents {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AppEvents::ReplaceSim(_) => write!(f, "AppEvent::ReplaceSim"),
            AppEvents::NewStateImage(_) => write!(f, "AppEvent::NewStateImage"),
            AppEvents::SetPreferredSearchPath(_) => write!(f,  "AppEvent: UpdatePreferredPath"),
            AppEvents::CurrentVersion(_) => write!(f, "AppEvent: CurrentVersion"),
            AppEvents::Error(err) => write!(f, "AppEvent: Error({err})")
        }
    }
}

impl From<LoadFileResponse> for AppEvents {
    fn from(e: LoadFileResponse) -> Self {
        match e {
            LoadFileResponse::LoadedFile(file) => {
                Self::ReplaceSim(file.map(Box::new).map_err(|err| err.0))
            }
            #[cfg(not(target_arch = "wasm32"))]
            LoadFileResponse::UpdatePreferredPath(path) => {
                Self::SetPreferredSearchPath(path)
            }
            #[cfg(not(target_arch = "wasm32"))]
            LoadFileResponse::SaveError(err) => AppEvents::Error(err)
        }
    }
}

impl TryFrom<AppEvents> for LoadFileResponse {
    type Error = AppEvents;

    fn try_from(value: AppEvents) -> Result<Self, Self::Error> {
        match value {
            AppEvents::ReplaceSim(s) =>
                Ok(LoadFileResponse::LoadedFile(s.map(|b| *b).map_err(FileParsingError))),
            #[cfg(not(target_arch = "wasm32"))]
            AppEvents::SetPreferredSearchPath(path) => {
                Ok(LoadFileResponse::UpdatePreferredPath(path))
            }
            #[cfg(not(target_arch = "wasm32"))]
            AppEvents::Error(err) if err.starts_with("failed to save")=> {
                Ok(LoadFileResponse::SaveError(err))
            }
            value =>
                Err(value)
        }
    }
}


impl From<SimUpdateServiceMessage> for AppEvents {
    fn from(message: SimUpdateServiceMessage) -> Self {
        match message {
            SimUpdateServiceMessage::NewFrame(sim) => Self::NewStateImage(sim),
            SimUpdateServiceMessage::CurrentState(sim) => Self::CurrentVersion(sim),
        }
    }
}

impl TryFrom<AppEvents> for SimUpdateServiceMessage {
    type Error = AppEvents;

    fn try_from(value: AppEvents) -> Result<Self, Self::Error> {
        match value {
            AppEvents::NewStateImage(image) => Ok(SimUpdateServiceMessage::NewFrame(image)),
            AppEvents::CurrentVersion(sim) => Ok(SimUpdateServiceMessage::CurrentState(sim)),
            state => Err(state)
        }
    }
}

pub fn load_file_service(mailbox: ChannelSender<AppEvents>) -> Option<LoadFileService> {
    let trans_service = transform(mailbox);
    let service = LoadFileService::new(trans_service);
    Some(service)
}

pub fn update_service(mailbox: ChannelSender<AppEvents>, delay: Duration, sim: AntSimulator<AntSimFrame>) -> Option<SimUpdateService> {
    let trans_service = transform(mailbox);
    let service = SimUpdateService::new(trans_service, (delay, Box::new(sim)));
    match service {
        Ok(s) => Some(s),
        Err(err) => {
            log::warn!("failed to create update service: {err}");
            None
        }
    }
}