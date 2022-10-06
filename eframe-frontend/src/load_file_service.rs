

use std::fmt::{Display};
use std::path::{PathBuf as SyncPathBuf};
use async_std::path::PathBuf as AsyncPathBuf;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::{
    pin::Pin,
    future::Future
};

use async_std::channel::{Receiver as ChannelReceiver, Sender as ChannelSender, SendError};
use std::thread::JoinHandle;
use std::time::Duration;
use ant_sim::ant_sim::AntSimulator;
use crate::AntSimFrame;
use crate::service_handle::{SenderDiedError, ServiceHandle};
use async_trait::async_trait;
use ant_sim_save::AntSimData;
use ant_sim_save::save_io::{DecodeSaveError, EncodeSaveError};
use crate::channel_actor::{ChannelActor, WorkerError};

pub enum LoadFileMessages {
    DroppedFileMessage(DroppedFileMessage),
    #[cfg(not(target_arch = "wasm32"))]
    LoadFileMessage(Pin<Box<dyn 'static + Send + Future<Output = Option<rfd::FileHandle>>>>),
    #[cfg(not(target_arch = "wasm32"))]
    SaveStateMessage(Pin<Box<dyn 'static + Send + Future<Output = Option<rfd::FileHandle>>>>, Box<AntSimulator<AntSimFrame>>)
}

#[cfg(not(target_arch = "wasm32"))]
pub struct DroppedFileMessage {
    pub path_buf: SyncPathBuf,
}

#[cfg(target_arch = "wasm32")]
pub struct DroppedFileMessage {
    pub bytes: Arc<[u8]>,
}

pub struct FileParsingError(pub String);

pub enum LoadFileResponse{
    LoadedFile(Result<AntSimulator<crate::AntSimFrame>, FileParsingError>),
    UpdatePreferredPath(SyncPathBuf),
    #[cfg(not(target_arch = "wasm32"))]
    SaveError(String)
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
            };


        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn handle_dropped_file(message: DroppedFileMessage) -> Result<AntSimulator<AntSimFrame>, String> {
        use ant_sim_save::save_subsystem::ReadSaveFileError;
        let file_name = message.path_buf.file_name().and_then(|str| str.to_str()).unwrap_or("").to_owned();
        let path_buf = AsyncPathBuf::from(message.path_buf);
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
        use ant_sim_save::save_io::DecodeSaveError;
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
        let file_path = AsyncPathBuf::from(file.path().to_path_buf());
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
        file.write_all(&repr);
        Ok(())
    }
}
fn try_construct_frame(d: ant_sim_save::Dimensions) -> Result<AntSimFrame, ()> {
    let width = d.width.try_into().map_err(|_| ())?;
    let height = d.height.try_into().map_err(|_| ())?;
    AntSimFrame::new(width, height).map_err(|_| ())
}