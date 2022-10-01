use std::fs::{DirEntry, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use ant_sim::ant_sim::AntSimulator;
use ant_sim::ant_sim_frame::AntSim;
use crate::{Dimensions};
use crate::save_io::{decode_save, DecodeSaveError, encode_save, EncodeSaveError};

pub struct SaveFileClass {
    path: PathBuf,
    path_buf: PathBuf,
}
#[derive(Debug)]
pub enum CreateSaveFileClassError {
    PathNotDictionary, FailedToCreateParentDir(io::Error)
}
#[derive(Debug)]
pub enum WriteSaveFileError {
    PathNotFile, FileExists, FailedToWriteFile(io::Error), InvalidData
}
#[derive(Debug)]
pub enum ReadSaveFileError {
    PathNotFile, FileDoesNotExist, FailedToRead(io::Error), InvalidFormat(String), InvalidData(String)
}
#[derive(Debug)]
pub enum NewestSaveError {
    IOErr(io::Error),
    NoSave,
    OperationNotSupported
}

impl SaveFileClass {
    pub fn new(path: impl AsRef<Path>) ->  Result<Self, CreateSaveFileClassError> {
        let path = path.as_ref();

        if path.exists() && !path.is_dir() {
            return Err(CreateSaveFileClassError::PathNotDictionary)
        }
        std::fs::DirBuilder::new().recursive(true)
            .create(path)
            .map_err(CreateSaveFileClassError::FailedToCreateParentDir)?;
        let path = path.to_path_buf();
        let save_class = Self {
            path_buf: path.clone(),
            path,
        };
        Ok(save_class)
    }
    fn extend_path_buf(&mut self,  by: impl AsRef<Path>) {
        self.path_buf.clear();
        self.path_buf.push(&self.path);
        self.path_buf.push(by.as_ref());
    }
    pub fn write_new_save<A: AntSim>(&mut self, name: impl AsRef<Path>, sim: &AntSimulator<A>, allow_override: bool) -> Result<(), WriteSaveFileError> {
        let name = name.as_ref();
        self.extend_path_buf(name);
        if self.path_buf.exists() {
            if !name.is_file() {
                return Err(WriteSaveFileError::PathNotFile)
            }
            if !allow_override {
                return Err(WriteSaveFileError::FileExists);
            }
        }

        let mut file = File::options().create(true).write(true).read(false)
            .open(&self.path_buf)
            .map_err(WriteSaveFileError::FailedToWriteFile)?;
        encode_save(&mut file, sim).map_err(|err| match err {
            EncodeSaveError::FailedToWrite(err) => WriteSaveFileError::FailedToWriteFile(err),
            EncodeSaveError::InvalidData => WriteSaveFileError::InvalidData
        })
    }
    pub fn read_save<A: AntSim>(&mut self, name: impl AsRef<Path>, get_sim: impl FnOnce(Dimensions) -> Result<A, ()>) -> Result<AntSimulator<A>, ReadSaveFileError> {
        let name = name.as_ref();
        self.extend_path_buf(name);
        Self::read_save_from(&self.path_buf, get_sim)
    }
    pub fn read_save_from<A:AntSim>(path_buf: impl AsRef<Path>, get_sim: impl FnOnce(Dimensions) -> Result<A, ()>)-> Result<AntSimulator<A>, ReadSaveFileError>  {
        let path_buf = path_buf.as_ref();
        if !path_buf.exists() {
            return Err(ReadSaveFileError::FileDoesNotExist);
        }
        let mut file = File::options().read(true)
            .open(path_buf)
            .map_err(ReadSaveFileError::FailedToRead)?;
        decode_save(&mut file, get_sim).map_err(|err| match err {
            DecodeSaveError::InvalidFormat(err) => ReadSaveFileError::InvalidFormat(err),
            DecodeSaveError::InvalidData(err) => ReadSaveFileError::InvalidData(err),
            DecodeSaveError::FailedToRead(err) => ReadSaveFileError::FailedToRead(err),
        })
    }

    pub fn all_files(&mut self) -> io::Result<impl Iterator<Item = DirEntry>> {
        Ok(std::fs::read_dir(&self.path)?.filter_map(Result::ok))
    }
    pub fn newest_save(&mut self,) -> Result<PathBuf, NewestSaveError> {
        let files = self.all_files().map_err(NewestSaveError::IOErr)?;
        files
            .map(|entry| entry.path())
            .filter_map(|entry| std::fs::metadata(&entry).map(|md| (entry, md)).ok())
            .filter(|(_, md)| md.is_file())
            .map(|(entry, md)| md.modified().or_else(|_| md.created()).map(|t| (entry, t)))
            .collect::<Result<Vec<(PathBuf, SystemTime)>, _>>().map_err(|_| NewestSaveError::OperationNotSupported)?
            .into_iter()
            .max_by_key(|(_, t)| t.elapsed().ok().unwrap_or(Duration::ZERO))
            .map(|(entry, _)| entry)
            .ok_or(NewestSaveError::NoSave)
    }
}
