#![feature(generic_associated_types)]
#![warn(clippy::all, rust_2018_idioms)]
#![allow(stable_features)]
#![feature(duration_checked_float)]
#![feature(let_else)]

mod app;
mod load_file_service;
mod service_handle;
mod app_services;
mod sim_computation_service;
mod sim_update_service;
/// time polyfill, since std::time::Instant does not work on wasm32 and wasm_timer
/// appears to be unmaintained.
mod time_polyfill;
mod channel_actor;
mod app_event_handling;

use ant_sim::ant_sim_frame_impl::AntSimVecImpl;
pub use app::AppState;

pub type AntSimFrame = AntSimVecImpl;