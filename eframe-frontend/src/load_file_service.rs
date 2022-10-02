use std::any::Any;
use std::fmt::{Display, Formatter, Pointer};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver as ChannelReceiver, Sender as ChannelSender, SendError};
use std::thread::JoinHandle;
use std::time::Duration;
use ant_sim::ant_sim::AntSimulator;
use crate::AntSimFrame;
use crate::service_handle::{join_with_timeout, ServiceHandle};

pub enum LoadFileMessages {
    DroppedFileMessage(DroppedFileMessage)
}

#[cfg(not(target_arch = "wasm32"))]
pub struct DroppedFileMessage {
    pub path_buf: PathBuf
}

#[cfg(target_arch = "wasm32")]
pub struct DroppedFileMessage {
    pub bytes: Arc<[u8]>
}

pub struct FileParsingError(pub String);

pub struct FileParsingCompleted(pub Result<AntSimulator<crate::AntSimFrame>, FileParsingError>);

pub struct LoadFileService<S: ServiceHandle<FileParsingCompleted>> {
    task_q: ChannelSender<LoadFileMessages>,
    worker_handle: JoinHandle<Result<(), WorkerError<S>>>
}

enum WorkerError<S: ServiceHandle<FileParsingCompleted>> {
    QueueDied, SenderFailed(S::Err)
}

static WORKER_ID: &str = "file-loading-service";

impl <S: 'static + Send + ServiceHandle<FileParsingCompleted>> LoadFileService<S> where S::Err: 'static + Send + Display {
    pub fn new(service_handle: S) -> Result<Self, String> {
        let task_q = std::sync::mpsc::channel();
        let worker_handle = std::thread::Builder::new()
            .name(WORKER_ID.to_owned())
            .spawn(move || {
                Self::task_worker(task_q.1, service_handle)
            })
            .map_err(|err| format!("Failed to create worker {WORKER_ID}: {err}"))?;
        let result= Self {
            task_q: task_q.0,
            worker_handle
        };
        Ok(result)
    }

    fn task_worker(rec: ChannelReceiver<LoadFileMessages>, mut send_to: S) -> Result<(), WorkerError<S>> {
        loop {
            let job = rec.recv().map_err(|_| WorkerError::QueueDied)?;
            let res = match job {
                LoadFileMessages::DroppedFileMessage(f) => {
                    let result = Self::handle_dropped_file(f).map_err(FileParsingError);
                    FileParsingCompleted(result)
                },
            };

            send_to = send_to.send(res)
                .map_err(|(_, err)| WorkerError::SenderFailed(err))?;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn handle_dropped_file(message: DroppedFileMessage) -> Result<AntSimulator<AntSimFrame>, String> {
        use ant_sim_save::save_subsystem::ReadSaveFileError;
        let res = ant_sim_save::save_subsystem::SaveFileClass::read_save_from(&message.path_buf, try_construct_frame);
        res.map_err(|err| match err {
            ReadSaveFileError::PathNotFile => String::from ("Dropped object is not a file"),
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
    fn recover_err(handle: JoinHandle<Result<(), WorkerError<S>>>) -> LoadFileError<S::Err> {
        let recovered_err = join_with_timeout(handle, Duration::from_millis(10));
        let thread_err = match recovered_err {
            None => return LoadFileError::IrregularError(format!("Failed to recover error")),
            Some(err) => err
        };
        let actual_error = match thread_err {
            Err(_) => return LoadFileError::IrregularError(format!("worker died under suspicious circumstances")),
            Ok(err) => err,
        };
        match actual_error {
            Ok(()) => LoadFileError::IrregularError(format!("worker stopped working")),
            Err(w_err) => match w_err {
                WorkerError::QueueDied => LoadFileError::IrregularError(format!("work queue died, even though service handle is still alive")),
                WorkerError::SenderFailed(err) => LoadFileError::SenderDied(err),
            }
        }
    }
}

pub enum LoadFileError<SE> {
    SenderDied(SE), IrregularError(String)
}

impl <SE: Display> Display for LoadFileError<SE> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadFileError::SenderDied(err) => write!(f, "service handle died: {err}"),
            LoadFileError::IrregularError(err) => write!(f, "service failed in an unexpected manner: {err}")
        }
    }
}

impl <S: 'static + Send + ServiceHandle<FileParsingCompleted>> ServiceHandle<LoadFileMessages> for LoadFileService<S> where S::Err: 'static + Send + Display {
    type Err = LoadFileError<S::Err>;

    fn send(mut self, t: LoadFileMessages) -> Result<Self, (LoadFileMessages, Self::Err)> {
        let send_err = match ServiceHandle::send(self.task_q, t) {
            Ok(send) => {
                self.task_q = send;
                return Ok(self)
            },
            Err(err) => err,
        };
        let worker_err = Self::recover_err(self.worker_handle);
        Err((send_err.0, worker_err))
    }
}

fn try_construct_frame(d: ant_sim_save::Dimensions) -> Result<AntSimFrame, ()> {
    let width = d.width.try_into().map_err(|_|())?;
    let height = d.height.try_into().map_err(|_|())?;
    AntSimFrame::new(width, height).map_err(|_|())
}