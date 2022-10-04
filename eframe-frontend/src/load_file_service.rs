use std::any::Any;
use std::fmt::{Display, Formatter, Pointer};
use std::path::PathBuf;
use std::sync::Arc;
use async_std::channel::{Receiver as ChannelReceiver, Sender as ChannelSender, SendError};
use std::thread::JoinHandle;
use std::time::Duration;
use ant_sim::ant_sim::AntSimulator;
use crate::AntSimFrame;
use crate::service_handle::{SenderDiedError, ServiceHandle};
use async_trait::async_trait;
use crate::channel_actor::{ChannelActor, WorkerError};

pub enum LoadFileMessages {
    DroppedFileMessage(DroppedFileMessage)
}

#[cfg(not(target_arch = "wasm32"))]
pub struct DroppedFileMessage {
    pub path_buf: PathBuf,
}

#[cfg(target_arch = "wasm32")]
pub struct DroppedFileMessage {
    pub bytes: Arc<[u8]>,
}

pub struct FileParsingError(pub String);

pub struct FileParsingCompleted(pub Result<AntSimulator<crate::AntSimFrame>, FileParsingError>);

pub type LoadFileService = ChannelActor<LoadFileMessages>;

impl LoadFileService {
    pub fn new<S>(service_handle: S) -> Self where S: 'static + Send + ServiceHandle<FileParsingCompleted>, S::Err: 'static + Send + Display {
        Self::new_actor("LoadFileService", service_handle, |rec, send_to, _| Self::task_worker(rec, send_to))
    }

    async fn task_worker<S>(rec: ChannelReceiver<LoadFileMessages>, mut send_to: S) -> Result<(), WorkerError<FileParsingCompleted, S>>
        where
            S: 'static + Send + ServiceHandle<FileParsingCompleted>,
            S::Err: 'static + Send + Display
    {
        loop {
            let job = rec.recv().await.map_err(|_| WorkerError::QueueDied)?;
            let res = match job {
                LoadFileMessages::DroppedFileMessage(f) => {
                    let result = Self::handle_dropped_file(f).map_err(FileParsingError);
                    FileParsingCompleted(result)
                }
            };

            send_to = send_to.send(res).await
                .map_err(|(_, err)| WorkerError::SenderFailed(err))?;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_dropped_file(message: DroppedFileMessage) -> Result<AntSimulator<AntSimFrame>, String> {
        use ant_sim_save::save_subsystem::ReadSaveFileError;
        let res = ant_sim_save::save_subsystem::SaveFileClass::read_save_from(&message.path_buf, try_construct_frame);
        todo!("async io");
        res.map_err(|err| match err {
            ReadSaveFileError::PathNotFile => String::from("Dropped object is not a file"),
            ReadSaveFileError::FileDoesNotExist => String::from("The dropped file cannot be accessed"),
            ReadSaveFileError::FailedToRead(err) => format!("Failed to read the dropped file: {err}"),
            ReadSaveFileError::InvalidFormat(err) => format!("The dropped file has an invalid format: {err}"),
            ReadSaveFileError::InvalidData(err) => format!("The dropped file contains invalid data: {err}")
        })
    }
    #[cfg(target_arch = "wasm32")]
    fn handle_dropped_file(message: DroppedFileMessage) -> Result<AntSimulator<AntSimFrame>, String> {
        use ant_sim_save::save_io::DecodeSaveError;
        let mut bytes = message.bytes.as_ref();
        ant_sim_save::save_io::decode_save(&mut bytes, try_construct_frame).map_err(|err| match err {
            DecodeSaveError::FailedToRead(err) => format!("Failed to read the dropped file: {err}"),
            DecodeSaveError::InvalidFormat(err) => format!("The dropped file has an invalid format: {err}"),
            DecodeSaveError::InvalidData(err) => format!("The dropped file contains invalid data: {err}")
        })
    }
}
fn try_construct_frame(d: ant_sim_save::Dimensions) -> Result<AntSimFrame, ()> {
    let width = d.width.try_into().map_err(|_| ())?;
    let height = d.height.try_into().map_err(|_| ())?;
    AntSimFrame::new(width, height).map_err(|_| ())
}