use std::io::{Read, Write};
use ant_sim::ant_sim::AntSimulator;
use ant_sim::ant_sim_frame::AntSim;
use crate::{AntSimData, Dimensions};

#[derive(Debug)]
pub enum DecodeSaveError {
    InvalidFormat(String), InvalidData(String), FailedToRead(std::io::Error)
}
#[derive(Debug)]
pub enum EncodeSaveError {
    FailedToWrite(std::io::Error), InvalidData
}

pub fn decode_save<A: AntSim>(r: &mut impl Read, get_sim: impl FnOnce(Dimensions) -> Result<A, ()>) -> Result<AntSimulator<A>, DecodeSaveError> {
    let data: AntSimData = serde_json::from_reader(r).map_err(|err| {
        if err.is_io() {
            DecodeSaveError::FailedToRead(err.into())
        } else {
            DecodeSaveError::InvalidFormat(format!("invalid data format at L{}:C{}: {}", err.line(), err.column(), err))
        }
    })?;
    data.try_into_board(get_sim).map_err(|err| DecodeSaveError::InvalidData(err))
}

pub fn encode_save<A: AntSim>(w: &mut impl Write, sim: &AntSimulator<A>) -> Result<(), EncodeSaveError> {
    let repr = AntSimData::from_state_sim(sim).map_err(|_| EncodeSaveError::InvalidData)?;
    serde_json::to_writer(w, &repr).map_err(|err| {
        if err.is_io() {
            EncodeSaveError::FailedToWrite(err.into())
        } else {
            EncodeSaveError::InvalidData
        }
    })
}