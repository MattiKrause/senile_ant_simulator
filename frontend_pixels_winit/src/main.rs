use std::ops::{Add, DerefMut};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
use winit::dpi::{LogicalSize};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{EventLoop};
use winit::window::WindowBuilder;
use chrono::{DateTime, Local};

use ant_sim::ant_sim::{AntSimulator};

use ant_sim::ant_sim_ant::{AntState};
use ant_sim::ant_sim_frame::{AntPosition, AntSim, AntSimCell};
use ant_sim::ant_sim_frame_impl::AntSimVecImpl;
use ant_sim_save::save_subsystem::*;
use recorder::gif_recorder::GIFRecorder;
use recorder::RgbaBufRef;

const DEFAULT_FRAME_LEN: Duration = Duration::from_millis(1000);
static _POINTS3: [(f64, f64); 8] = [
    (3.0, 0.0),
    (2.0121320343559643, 2.1213203435596424),
    (0.0, 3.0),
    (-2.1213203435596424, 2.121320343559643),
    (-3.0, 0.0),
    (-2.121320343559643, -2.1213203435596424),
    (0.0, -3.0),
    (2.121320343559642, -2.121320343559643),
];

static _POINTS1: [(f64, f64); 8] = [
    (1.0, 0.0),
    (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (0.0, 1.0),
    (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (-1.0, 0.0),
    (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (-0.0, -1.0),
    (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
];

fn main() -> Result<(), String>{
    let mut save_class = SaveFileClass::new("ant_sim_saves/").unwrap();
    let save_name = String::from("ant_sim_test_state.txt");
    let sim = read_save(&mut save_class, &save_name)?;

    let event_loop = EventLoop::new();
    let window = {
        let width: f64 = sim.sim.width() as f64;
        let height: f64 = sim.sim.height() as f64;
        let size = LogicalSize::new(width, height);
        let scaled_size = LogicalSize::new(width, height);
        WindowBuilder::new()
            .with_resizable(true)
            .with_title("Ant Simulator 9000")
            .with_inner_size(scaled_size)
            .with_min_inner_size(size)
            .build(&event_loop)
            .unwrap()
    };
    let screen = {
        let win_size = window.inner_size();
        let texture = SurfaceTexture::new(win_size.width, win_size.height, &window);
        PixelsBuilder::new(sim.sim.width() as u32, sim.sim.height() as u32, texture)
            .build()
            .unwrap()
    };
    main_loop(event_loop, screen, sim, save_class);
    Ok(())
}

fn write_save<A: AntSim>(to_file: &mut SaveFileClass, name: &str, sim: &AntSimulator<A>) -> Result<(), String> {
    to_file.write_new_save(name, sim, true).map_err(|err| match err {
        WriteSaveFileError::PathNotFile => format!("path is not file"),
        WriteSaveFileError::FileExists => format!("the file already exists and cannot be overriden"),
        WriteSaveFileError::FailedToWriteFile(err) => format!("failed to write to file: {err}"),
        WriteSaveFileError::InvalidData => format!("invalid state data")
    })
}

fn write_auto_save<A: AntSim>(to_file: &mut SaveFileClass, base_name: &str, sim: &AntSimulator<A>) -> Result<(), String> {
    let time = DateTime::<Local>::from(SystemTime::now());
    let time_str = time.to_rfc3339();
    write_save(to_file, &format!("{base_name}-autosave-{time_str}.json"), sim)
}

fn read_save(from_class: &mut SaveFileClass, from_file: &str) -> Result<AntSimulator<AntSimVecImpl>, String> {
    let res = from_class.read_save(from_file, |d| {
        let width = d.width.try_into().map_err(|_|())?;
        let height = d.height.try_into().map_err(|_|())?;
        AntSimVecImpl::new(width, height)
    });
    res.map_err(|err| match err {
        ReadSaveFileError::PathNotFile => format!("given path is not a file"),
        ReadSaveFileError::FileDoesNotExist => format!("the given save file does not exist"),
        ReadSaveFileError::FailedToRead(err) => format!("Failed to read from file: {err}"),
        ReadSaveFileError::InvalidFormat(err) => err,
        ReadSaveFileError::InvalidData(err) => err
    })
}

fn main_loop(event_loop: EventLoop<()>, mut screen: Pixels, state: AntSimulator<AntSimVecImpl>, mut save_class: SaveFileClass) {
    let mut gif = GIFRecorder::new(state.sim.width() as u16, state.sim.height() as u16, "ant.gif", true).unwrap();
    let state = Mutex::new((Box::new(state.clone()), Box::new(state)));
    let state = &*Box::leak(Box::new(state));
    let threshold = DEFAULT_FRAME_LEN;
    let producer_patience = Duration::from_millis(10);
    let proxy = event_loop.create_proxy();
    let proceed = Condvar::new();
    let proceed = &*Box::leak(Box::new(proceed));
    let _handle = thread::spawn(move || {
        let proxy = proxy;
        let producer_patience = producer_patience;
        let mut state = state.lock().unwrap();
        loop {
            let (prev, new) = state.deref_mut();
            prev.update(new.deref_mut());
            std::mem::swap(prev, new);
            let (new_state, timeout) = proceed.wait_timeout(state, producer_patience).unwrap();
            state = if timeout.timed_out() {
                proxy.send_event(()).unwrap();
                proceed.wait(new_state).unwrap()
            } else {
                new_state
            };
        }
    });

    let mut last_loop = Instant::now();
    event_loop.run(move |a, _, c| {
        if last_loop.elapsed() > threshold {
            if let Ok(state) = state.try_lock() {
                last_loop = Instant::now();
                draw_state(&state.1, &mut screen);
                gif.new_frame(RgbaBufRef::try_from(screen.get_frame()).unwrap(), Duration::from_millis(20));
                write_auto_save(&mut save_class, "default-save", state.1.as_ref()).unwrap();
                drop(state);
                proceed.notify_all();
            } else {
                c.set_wait_until(Instant::now().add(Duration::from_millis(5)));
            }
        }
        if let Event::WindowEvent { window_id: _, event } = a {
            match event {
                WindowEvent::Resized(r) => {
                    screen.resize_surface(r.width, r.height);
                }
                WindowEvent::CloseRequested => {
                    c.set_exit();
                }
                _ => {}
            }
        }
    });
}

fn pixel(frame: &mut [u8], pix: usize) -> &mut [u8] {
    let pix = pix * 4;
    &mut frame[pix..(pix + 4)]
}

fn pixel_of_pos(width: usize, frame: &mut [u8], pos: AntPosition) -> &mut [u8] {
    let AntPosition { x, y } = pos;
    let pix = y * width + x;
    pixel(frame, pix)
}

fn draw_state<A: AntSim>(sim: &AntSimulator<A>, on: &mut Pixels) {
    let frame = on.get_frame();
    for cell in sim.sim.cells() {
        let (cell, pos): (AntSimCell, A::Position) = cell;
        let pos = sim.sim.decode(&pos);
        let pixel = pixel_of_pos(sim.sim.width(), frame, pos);
        let color = match cell {
            AntSimCell::Path { pheromone_food, pheromone_home } => {
                [(pheromone_food.get() / 256u16) as u8, 0, (pheromone_home.get() / 256u16) as u8, 0xFF]
            }
            AntSimCell::Blocker => {
                [0xAF, 0xAF, 0xAF, 0xFF]
            }
            AntSimCell::Home => {
                [0xFF, 0xFF, 0x00, 0xFF]
            }
            AntSimCell::Food { amount } => {
                [0, (amount / 256u16) as u8, 0, 0xFF]
            }
        };
        pixel.copy_from_slice(&color);
    }
    for ant in &sim.ants {
        let pos = sim.sim.decode(ant.position());
        let color = match ant.state(){
            AntState::Foraging => [0xFF, 0xFF, 0xFF, 0xFF],
            AntState::Hauling { amount }=> {
                let amount  = (*amount / 256u16) as u8 * (u8::MAX / 2);
                [0xFF - amount, 0xFF, 0xFF - amount, 0xFF]
            }
        };
        pixel_of_pos(sim.sim.width(), frame, pos).copy_from_slice(&color);
    }
    on.render().unwrap();
}
