use std::fmt::{Debug, Formatter};
use std::time::Duration;
use crate::load_file_service::{LoadFileResponse, FileParsingError, LoadFileService};
use crate::service_handle::{ServiceHandle};
use async_std::channel::{Sender as ChannelSender};
use ant_sim::ant_sim::AntSimulator;
use crate::AntSimFrame;
use crate::app::AppEvents;
use crate::sim_update_service::{SimUpdateService, SimUpdateServiceMessage};
use async_trait::async_trait;

pub struct Services {
    pub mailbox_in: ChannelSender<AppEvents>,
    pub load_file: Option<LoadFileService>,
    pub update: Option<SimUpdateService>
}

struct AppFacet<S: ServiceHandle<AppEvents>> {
    backing: S,
    ctx: egui::Context
}

#[async_trait]
impl<F: TryFrom<AppEvents> + Send + 'static, S: ServiceHandle<AppEvents> + Send> ServiceHandle<F> for AppFacet<S>
    where <F as TryFrom<AppEvents>>::Error: Debug, AppEvents: From<F> {
    type Err = S::Err;

    async fn send(self, t: F) -> Result<Self, (F, Self::Err)> {
        match self.backing.send(AppEvents::from(t)).await {
            Ok(backing) => {
                self.ctx.request_repaint();
                let new = Self {
                    backing,
                    ..self
                };
                Ok(new)
            }
            Err((t, err)) => {
                let f = F::try_from(t).expect("sender did not return the value that was send");
                Err((f, err))
            }
        }
    }

    fn try_send(self, t: F) -> Result<(Self, Option<F>), (F, Self::Err)> {
        match self.backing.try_send(AppEvents::from(t)) {
            Ok((backing, m)) => {
                self.ctx.request_repaint();
                let new = Self {
                    backing,
                    ..self
                };
                let m = m.map(|t| F::try_from(t).expect("sender did not return the value that was send"));
                Ok((new, m))
            }
            Err((t, err)) => {
                let f = F::try_from(t).expect("sender did not return the value that was send");
                Err((f, err))
            }
        }
    }
}

impl Debug for AppEvents {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        macro_rules! str_event {
            ($e: ident) => {
                write!(f, stringify!(AppEvents::$e))
            };
        }
        match self {
            AppEvents::ReplaceSim(_) => str_event!(ReplaceSim),
            AppEvents::NewStateImage(_) => str_event!(NewStateImage),
            AppEvents::SetPreferredSearchPath(_) => str_event!(SetPreferredSearchPath),
            AppEvents::CurrentVersion(_) => str_event!(CurrentVersion),
            AppEvents::Error(err) => write!(f, "AppEvent::Error({err})"),
            AppEvents::RequestPause => str_event!(RequestPause),
            AppEvents::DelayRequest(_) => str_event!(DelayRequest),
            AppEvents::RequestLoadGame => str_event!(RequestLoadGame),
            AppEvents::RequestSaveGame => str_event!(RequestSaveGame),
            AppEvents::RequestLaunch => str_event!(RequestLaunch),
            AppEvents::RequestSetBoardWidth => str_event!(RequestSetBoardWidth),
            AppEvents::RequestSetBoardHeight => str_event!(RequestSetBoarHeight),
            AppEvents::RequestSetSeed => str_event!(RequestSetSeed),
            AppEvents::PaintStroke { .. } => str_event!(PaintStroke),
            AppEvents::SetBrushType(_) => str_event!(SetBrushType),
            AppEvents::SetBrushMaterial(_) => str_event!(SetBrushMaterial),
            AppEvents::ImmediateNextFrame => str_event!(ImmediateNextFrame),
            AppEvents::BoardClick(_) => str_event!(BoardClick),
            AppEvents::RequestSetPointsRadius => str_event!(RequestSetPointsRadius)
        }
    }
}

impl From<LoadFileResponse> for AppEvents {
    fn from(e: LoadFileResponse) -> Self {
        match e {
            LoadFileResponse::LoadedFile(file) => {
                Self::ReplaceSim(file.map(Box::new).map_err(|err| err.0))
            }
            LoadFileResponse::UpdatePreferredPath(path) => {
                Self::SetPreferredSearchPath(path)
            }
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

pub fn load_file_service(mailbox: ChannelSender<AppEvents>, ctx: egui::Context) -> Option<LoadFileService> {
    let trans_service = AppFacet {
        backing: mailbox,
        ctx
    };
    let service = LoadFileService::new(trans_service);
    Some(service)
}

pub fn update_service(mailbox: ChannelSender<AppEvents>, delay: Duration, sim: AntSimulator<AntSimFrame>, initial_pause: bool, ctx: egui::Context) -> Option<SimUpdateService> {
    let trans_service = AppFacet {
        backing: mailbox,
        ctx
    };
    let service = SimUpdateService::new(trans_service, initial_pause, (delay, Box::new(sim)));
    match service {
        Ok(s) => Some(s),
        Err(err) => {
            log::warn!("failed to create update service: {err}");
            None
        }
    }
}