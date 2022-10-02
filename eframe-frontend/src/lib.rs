#![warn(clippy::all, rust_2018_idioms)]

mod app;
mod load_file_service;
mod service_handle;
mod app_services;

use ant_sim::ant_sim_frame_impl::AntSimVecImpl;
pub use app::AppState;

pub type AntSimFrame = AntSimVecImpl;