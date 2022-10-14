

use std::fmt::{Display};
use std::path::{PathBuf as SyncPathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    pin::Pin,
    future::Future
};

use async_std::channel::{Receiver as ChannelReceiver};
use ant_sim::ant_sim::AntSimulator;
use ant_sim::ant_sim_frame::AntSim;
use crate::AntSimFrame;
use crate::service_handle::{ServiceHandle};
use ant_sim_save::save_io::{DecodeSaveError, EncodeSaveError};
use crate::channel_actor::{ChannelActor, WorkerError};

pub enum LoadFileMessages {
    DroppedFileMessage(DroppedFileMessage),
    #[cfg(not(target_arch = "wasm32"))]
    LoadFileMessage(Pin<Box<dyn 'static + Send + Future<Output = Option<rfd::FileHandle>>>>),
    #[cfg(not(target_arch = "wasm32"))]
    SaveStateMessage(Pin<Box<dyn 'static + Send + Future<Output = Option<rfd::FileHandle>>>>, Box<AntSimulator<AntSimFrame>>),
    #[cfg(target_arch = "wasm32")]
    DownloadStateMessage(Box<AntSimulator<AntSimFrame>>),
}

#[cfg(not(target_arch = "wasm32"))]
pub struct DroppedFileMessage {
    pub path_buf: SyncPathBuf,
}

#[cfg(target_arch = "wasm32")]
pub struct DroppedFileMessage {
    pub bytes: std::sync::Arc<[u8]>,
}

pub struct FileParsingError(pub String);

pub enum LoadFileResponse{
    LoadedFile(Result<AntSimulator<crate::AntSimFrame>, FileParsingError>),
    UpdatePreferredPath(SyncPathBuf),
    SaveError(String)
}

impl LoadFileResponse {
    fn save_error(err: String) -> Self {
        Self::SaveError(format!("failed to save file: {err}"))
    }
}

pub type LoadFileService = ChannelActor<LoadFileMessages>;

impl LoadFileService {
    pub fn new<S>(service_handle: S) -> Self where S: 'static + Send + ServiceHandle<LoadFileResponse>, S::Err: 'static + Send + Display {
        Self::new_actor("LoadFileService", service_handle, |rec, send_to, _| Self::task_worker(rec, send_to))
    }

    async fn task_worker<S>(rec: ChannelReceiver<LoadFileMessages>, mut send_to: S) -> Result<(), WorkerError<LoadFileResponse, S>>
        where
            S: 'static + Send + ServiceHandle<LoadFileResponse>,
            S::Err: 'static + Send + Display
    {
        loop {
            let job = rec.recv().await.map_err(|_| WorkerError::QueueDied)?;
            match job {
                LoadFileMessages::DroppedFileMessage(f) => {
                    let result = Self::handle_dropped_file(f).await.map_err(FileParsingError);
                    let send_message = LoadFileResponse::LoadedFile(result);
                    send_to = send_to.send(send_message).await
                        .map_err(|(_, err)| WorkerError::SenderFailed(err))?;
                }
                #[cfg(not(target_arch = "wasm32"))]
                LoadFileMessages::LoadFileMessage(fut) => {
                    let dialog = Self::load_file_dialog(fut).await;
                    let (buf, sim_res) = if let Some(res) = dialog {
                        res
                    } else {
                        continue
                    };
                    let sim_res = LoadFileResponse::LoadedFile(sim_res.map_err(FileParsingError));
                    send_to = send_to.send(sim_res).await
                        .map_err(|(_, err)| WorkerError::SenderFailed(err))?;
                    send_to = send_to.send(LoadFileResponse::UpdatePreferredPath(buf)).await
                        .map_err(|(_, err)| WorkerError::SenderFailed(err))?;
                }
                #[cfg(not(target_arch = "wasm32"))]
                LoadFileMessages::SaveStateMessage(fut, sim) => {
                    let result = Self::save_file_dialog(fut, sim.as_ref()).await;
                    let (file, err) = if let Some(result) = result {
                        result
                    }  else {
                        continue
                    };
                    if let Err(err) = err {
                        send_to = send_to.send(LoadFileResponse::SaveError(format!("failed to save to file: {err}"))).await
                            .map_err(|(_, err)| WorkerError::SenderFailed(err))?;
                    }
                    send_to = send_to.send(LoadFileResponse::UpdatePreferredPath(file.into())).await
                        .map_err(|(_, err)| WorkerError::SenderFailed(err))?
                }
                #[cfg(target_arch = "wasm32")]
                LoadFileMessages::DownloadStateMessage(sim) => {
                    if let Err(err) = Self::download_state(sim.as_ref()) {
                        send_to = send_to.send(LoadFileResponse::save_error(err)).await
                            .map_err(|(_, err)| WorkerError::SenderFailed(err))?;
                    };
                }
            };


        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn handle_dropped_file(message: DroppedFileMessage) -> Result<AntSimulator<AntSimFrame>, String> {
        let file_name = message.path_buf.file_name().and_then(|str| str.to_str()).unwrap_or("").to_owned();
        let path_buf = async_std::path::PathBuf::from(message.path_buf);
        let bytes = async_std::fs::read(&path_buf)
            .await
            .map_err(|err| format!("Failed to read file {}: {err}", file_name))?;
        let sim =ant_sim_save::save_io::decode_save(&mut bytes.as_slice(),  try_construct_frame)
            .map_err(|err| match err {
                DecodeSaveError::FailedToRead(err) => format!("Failed to read file {}: {err}", file_name),
                DecodeSaveError::InvalidFormat(err) => format!("invalid save file format: {err}"),
                DecodeSaveError::InvalidData(err) => format!("invalid data in file {}: {err}", file_name)
            })?;
        Ok(sim)
    }
    #[cfg(target_arch = "wasm32")]
    async fn handle_dropped_file(message: DroppedFileMessage) -> Result<AntSimulator<AntSimFrame>, String> {
        let mut bytes = message.bytes.as_ref();
        ant_sim_save::save_io::decode_save(&mut bytes, try_construct_frame).map_err(|err| match err {
            DecodeSaveError::FailedToRead(err) => format!("Failed to read the dropped file: {err}"),
            DecodeSaveError::InvalidFormat(err) => format!("The dropped file has an invalid format: {err}"),
            DecodeSaveError::InvalidData(err) => format!("The dropped file contains invalid data: {err}")
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    async fn load_file_dialog(file: Pin<Box<dyn 'static + Send + Future<Output = Option<rfd::FileHandle>>>>) -> Option<(SyncPathBuf, Result<AntSimulator<AntSimFrame>, String>)>{
        let file = file.await?;
        Some((file.path().to_path_buf(), Self::handle_dropped_file(DroppedFileMessage { path_buf: file.path().to_path_buf() }).await))
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn save_file_dialog(file: Pin<Box<dyn 'static + Send + Future<Output = Option<rfd::FileHandle>>>>, sim: &AntSimulator<AntSimFrame>) -> Option<(SyncPathBuf, Result<(), String>)> {
        let file = file.await?;
        let file_path = async_std::path::PathBuf::from(file.path().to_path_buf());
        let file = async_std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&file_path);
        let result = Self::save_to_file(file, sim).await;
        Some((file_path.into(), result))
    }
    #[cfg(not(target_arch = "wasm32"))]
    async fn save_to_file(file: impl Future<Output = std::io::Result<async_std::fs::File>>, sim: &AntSimulator<AntSimFrame>) -> Result<(), String> {
        use async_std::io::WriteExt;
        let mut repr = Vec::new();
        ant_sim_save::save_io::encode_save(&mut repr, &sim).map_err(|err| match err {
            EncodeSaveError::FailedToWrite(err) => format!("failed to write to buffer: {err}"),
            EncodeSaveError::InvalidData => format!("simulation data is invalid"),
        })?;
        let mut file = file.await.map_err(|err| format!("failed to open file: {err}"))?;
        dbg!(repr.len());
        file.write_all(&repr).await.map_err(|err| format!("failed to write to file: {err}"))?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    fn download_state<A: AntSim>(sim: &AntSimulator<A>)  -> Result<(), String> {
        let mut repr = Vec::new();
        ant_sim_save::save_io::encode_save(&mut repr, sim).map_err(|err| match err {
            EncodeSaveError::FailedToWrite(w) => format!("failed to write to buf: {w}"),
            EncodeSaveError::InvalidData => format!("current game state is invalid")
        })?;
        Self::download_file(&repr, "save_state.txt")
    }

    #[cfg(target_arch = "wasm32")]
    fn download_file(file: &[u8], name: &str) -> Result<(), String> {
        use eframe::wasm_bindgen::{JsValue, JsCast};
        let window = web_sys::window().ok_or_else(|| String::from("not in a window context"))?;
        let document = window.document().ok_or_else(|| String::from("no associated document"))?;
        let blob = gloo_file::Blob::new(file);
        let url = gloo_file::ObjectUrl::from(blob);
        let element = document.create_element("a").map_err(|_| format!("failed to create download element", ))?;

        let element = element.dyn_into::<web_sys::HtmlElement>().map_err(|_| String::from("unknown element type"))?;
        element.set_attribute("hidden", "")
            .and_then(|_| element.set_attribute("download", name))
            .and_then(|_| element.set_attribute("href", &url))
            .map_err(|_| format!("failed to set attributes on download"))?;
        element.click();
        Ok(())
    }
}
fn try_construct_frame(d: ant_sim_save::Dimensions) -> Result<AntSimFrame, ()> {
    let width = d.width.try_into().map_err(|_| ())?;
    let height = d.height.try_into().map_err(|_| ())?;
    AntSimFrame::new(width, height).map_err(|_| ())
}