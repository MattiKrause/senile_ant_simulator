mod write_service;

use std::mem::swap;
use std::path::PathBuf;
use std::time::Duration;
use std::io::Write;
use clap::Parser;
use clap::builder::ValueHint;
use console::Term;
use ant_sim::ant_sim::AntSimulator;
use ant_sim::ant_sim_ant::AntState;
use ant_sim::ant_sim_frame::{AntPosition, AntSim, AntSimCell};
use ant_sim::ant_sim_frame_impl::AntSimVecImpl;
use ant_sim_save::save_subsystem::{ReadSaveFileError, SaveFileClass};
use recorder::gif_recorder::{GIFRecorder, NewGifRecorderError};
use recorder::{ColorBuffer, RgbaBoxBuf, SetRgb};
use crate::write_service::RgbaWriteService;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct RecorderArgs {
    /// The save file of which the replay is recorded
    #[clap(short = 's', long = "save_file", value_parser, value_hint=ValueHint::FilePath)]
    save_file_name: PathBuf,
    /// The gif file to which the replay is saved
    #[clap(long = "gif", value_parser, value_hint=ValueHint::FilePath)]
    gif_name: PathBuf,
    /// The delay between frames in milliseconds
    #[clap(short = 'd', long = "delay",  default_value_t = 20)]
    frame_delay: u32,
    /// The length of the replay in seconds
    #[clap(long = "time_limit")]
    time_limit: Option<u32>
}

struct SimulatorContext<A: AntSim> {
    sim1: Box<AntSimulator<A>>,
    sim2: Box<AntSimulator<A>>
}

fn main() -> Result<(), String> {
    let res: RecorderArgs = RecorderArgs::parse();
    recording_task(res, &mut Term::stdout())
}

pub fn recording_task(args: RecorderArgs, output: &mut Term) -> Result<(), String> {
    let save_file = parse_save_file(args.save_file_name)?;
    let recorder = create_gif_recorder_for(save_file.sim.width(), save_file.sim.height(), args.gif_name)?;

    let delay = Duration::from_millis(args.frame_delay.into());
    let time_limit = args.time_limit.map(|secs| Duration::from_secs(secs.into())).unwrap_or(Duration::MAX);

    let time_limit_of_str = args.time_limit.map(|t| format!("/{t}")).unwrap_or(String::new());
    let buf_size = save_file.sim.width() * save_file.sim.height();
    let mut gif_service = RgbaWriteService::<RgbaBoxBuf, _>::new(recorder, 5, buf_size, delay);
    let mut buf = RgbaBoxBuf::from_pixels(buf_size);
    let mut context = SimulatorContext {
        sim1: Box::new(save_file.clone()),
        sim2: Box::new(save_file)
    };
    let mut time = Duration::ZERO;
    let _ = writeln!(output, "secs: {}{}", 0, time_limit_of_str);
    while time < time_limit {
        context.sim1.update(&mut context.sim2);
        draw_to_buf(&context.sim1, &mut buf.buf_ref());
        gif_service = gif_service.queue_frame(&buf.buf_ref()).map_err(|err| format!("gif worker died: {err}"))?;
        swap(&mut context.sim1, &mut context.sim2);

        let secs = time.as_secs();
        time += delay;
        if time.as_secs() > secs {
            let _ = output.clear_last_lines(1);
            let _ = writeln!(output, "secs: {}{}", time.as_secs(), time_limit_of_str);
        }
    }
    let _ = writeln!(output, "finished writing the recording task");
    Ok(())
}

fn parse_save_file(file: PathBuf) -> Result<AntSimulator<AntSimVecImpl>, String> {
    let result = SaveFileClass::read_save_from(&file, |d| {
        let height = d.height.try_into().map_err(|_|())?;
        let width = d.width.try_into().map_err(|_|())?;
        AntSimVecImpl::new(width, height).map_err(|_|())
    });

    result.map_err(|err| match err {
        ReadSaveFileError::FileDoesNotExist => format!("The given save file does not exist"),
        ReadSaveFileError::PathNotFile => format!("The given path is not a sve file"),
        ReadSaveFileError::FailedToRead(err) => format!("failed to read save file: {err}"),
        ReadSaveFileError::InvalidFormat(err) => format!("corrupted save file:{err}"),
        ReadSaveFileError::InvalidData(err) => format!("corrupted save data: {err}"),
    })
}

fn create_gif_recorder_for(width: impl TryInto<u16>, height: impl TryInto<u16>, path: PathBuf) -> Result<GIFRecorder, String> {
    if let Some(parent) = path.parent() {
        std::fs::DirBuilder::new().recursive(true)
            .create(parent)
            .map_err(|err| format!("failed to create parent directories: {err}"))?;
    }
    {
        let width = width.try_into().map_err(|_| format!("unsupported board width for gif recorder"))?;
        let height = height.try_into().map_err(|_| format!("unsupported board height for gif recorder"))?;
        let recorder = GIFRecorder::new(width, height, &path, true);
        recorder.map_err(|err| match err {
                NewGifRecorderError::FileAlreadyExists => format!("The recorded replay already exists"),
                NewGifRecorderError::FileErr(err) => format!("Failed to write to the requested file: {err}"),
                NewGifRecorderError::FormatErr => format!("internal err :("),
            })
    }
}

fn draw_to_buf<A: AntSim>(sim: &AntSimulator<A>, frame: &mut impl SetRgb) {
    fn set_pixel(width: usize, pos: AntPosition, val: [u8; 3], into: &mut impl SetRgb) {
        into.set_rgb(pos.y * width + pos.x, val);
    }
    assert_eq!(sim.sim.width() * sim.sim.height(), frame.len());
    for cell in sim.sim.cells() {
        let (cell, pos): (AntSimCell, A::Position) = cell;
        let pos = sim.sim.decode(&pos);
        let color = match cell {
            AntSimCell::Path { pheromone_food, pheromone_home } => {
                [(pheromone_food.get() / 256u16) as u8, 0, (pheromone_home.get() / 256u16) as u8]
            }
            AntSimCell::Blocker => {
                [0xAF, 0xAF, 0xAF]
            }
            AntSimCell::Home => {
                [0xFF, 0xFF, 0x00]
            }
            AntSimCell::Food { amount } => {
                [0, (amount / 256u16) as u8, 0]
            }
        };
        set_pixel(sim.sim.width(), pos, color, frame);
    }
    for ant in &sim.ants {
        let pos = sim.sim.decode(ant.position());
        let color = match ant.state(){
            AntState::Foraging => [0xFF, 0xFF, 0xFF],
            AntState::Hauling { amount }=> {
                let amount  = (*amount / 256u16) as u8 * (u8::MAX / 2);
                [0xFF - amount, 0xFF, 0xFF - amount]
            }
        };
        set_pixel(sim.sim.width(), pos, color, frame);
    }
}
