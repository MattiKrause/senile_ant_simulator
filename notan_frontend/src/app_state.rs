use std::sync::mpsc::{Receiver, SyncSender};
use std::time::{Duration, Instant};
use notan::draw::{Draw, Font};
use notan::prelude::*;
use ant_sim::ant_sim::AntSimulator;
use ant_sim::ant_sim_frame_impl::AntSimVecImpl;
use rgba_adapter::RgbaBoxBuf;

#[derive(AppState)]
pub struct State {
    pub resources: Resources,
    pub edit_state: EditState,
}

pub struct Resources {
    pub default_font: Font,
}

pub enum EditState {
    CorruptedState,
    ErrorState(EditStateError),
    Edit(EditStateEdit),
    Started(EditStateStarted),
}

pub struct AntSimTexture {
    pub texture: Texture,
    pub buf: RgbaBoxBuf,
    pub dirty: bool,
}

pub struct EditStateError {
    pub back_state: Box<EditState>,
    pub error: String,
    pub draw: Option<Draw>,
}

pub struct EditStateEdit {
    pub save_state: AntSimulator<AntSimFrameImpl>,
    pub back_texture: AntSimTexture,
    pub draw: Option<Draw>,
}

pub struct EditStateStarted {
    pub save_state: GameState,
    pub back_texture: AntSimTexture,
    pub delay: Duration,
    pub last_updated: Instant,
    pub draw: Option<Draw>,
    pub paused: bool,
}

pub type AntSimFrameImpl = AntSimVecImpl;

pub struct GameState {
    pub sim1: AntSimulator<AntSimFrameImpl>,
    pub sim2: AntSimulator<AntSimFrameImpl>,
}