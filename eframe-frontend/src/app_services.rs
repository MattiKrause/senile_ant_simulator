use std::fmt::{Debug, Formatter};
use crate::load_file_service::{FileParsingCompleted, FileParsingError, LoadFileService};
use crate::service_handle::{transform, TransService};
use std::sync::mpsc::{Sender as ChannelSender, Sender};
use crate::app::AppEvents;

pub struct Services {
    pub mailbox_in: ChannelSender<AppEvents>,
    pub load_file: Option<LoadFileService<TransService<AppEvents, ChannelSender<AppEvents>>>>,
}

impl Debug for AppEvents {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AppEvents::ReplaceSim(_) => write!(f, "AppEvent::ReplaceSim"),
            AppEvents::NewStateImage(_) => write!(f, "AppEvent::NewStateImage"),
        }
    }
}

impl From<FileParsingCompleted> for AppEvents {
    fn from(e: FileParsingCompleted) -> Self {
        Self::ReplaceSim(e.0.map(Box::new).map_err(|err| err.0))
    }
}

impl TryFrom<AppEvents> for FileParsingCompleted {
    type Error = AppEvents;

    fn try_from(value: AppEvents) -> Result<Self, Self::Error> {
        match value {
            AppEvents::ReplaceSim(s) =>
                Ok(FileParsingCompleted(s.map(|b| *b).map_err(FileParsingError))),
            value =>
                Err(value)
        }
    }
}

pub fn load_file_service(mailbox: ChannelSender<AppEvents>) -> Option<LoadFileService<TransService<AppEvents, ChannelSender<AppEvents>>>> {
    let trans_service = transform(mailbox);
    let service = LoadFileService::new(trans_service);
    match service {
        Ok(s) => Some(s),
        Err(err) => {
            log::debug!(target: "LoadFileService", "cannot create service: {err}");
            None
        }
    }
}