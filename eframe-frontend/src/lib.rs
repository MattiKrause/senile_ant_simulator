#![feature(generic_associated_types)]
#![warn(clippy::all, rust_2018_idioms)]
#![allow(stable_features)]
#![feature(duration_checked_float)]

mod app;
mod load_file_service;
mod service_handle;
mod app_services;
mod sim_computation_service;
mod sim_update_service;
mod time_polyfill;
mod channel_actor;

use ant_sim::ant_sim_frame_impl::AntSimVecImpl;
pub use app::AppState;

pub type AntSimFrame = AntSimVecImpl;