use std::ops::{Add, DerefMut};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
use winit::dpi::{LogicalSize};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{EventLoop};
use winit::window::WindowBuilder;
use crate::ant_sim::{AntSimConfig, AntSimulator, AntVisualRangeBuffer, neighbors};

use crate::ant_sim_ant::{Ant, AntState, simple_hash};
use crate::ant_sim_frame::{AntPosition, AntSim, AntSimCell};
use crate::ant_sim_frame_impl::AntSimVecImpl;

mod ant_sim_frame;
mod ant_sim_ant;
mod ant_sim_frame_impl;
mod ant_sim;

const WIDTH: u32 = 255;
const HEIGHT: u32 = 255;
const HAUL_AMOUNT: u8 = 20;
const DECAY_RATE: u8 = 2;
const DEFAULT_FRAME_LEN: Duration = Duration::from_millis(200);
const SEED: u64 = 42;
const VISUAL_RANGE: u8 = 3;
static POINTS: &'static [(f64, f64); 8] = &POINTS3;

static POINTS3: [(f64, f64); 8] = [
    (3.0, 0.0),
    (2.0121320343559643, 2.1213203435596424),
    (0.0, 3.0),
    (-2.1213203435596424, 2.121320343559643),
    (-3.0, 0.0),
    (-2.121320343559643, -2.1213203435596424),
    (0.0, -3.0),
    (2.121320343559642, -2.121320343559643),
];

static POINTS1: [(f64, f64); 8] = [
    (1.0, 0.0),
    (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (0.0, 1.0),
    (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (-1.0, 0.0),
    (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (-0.0, -1.0),
    (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
];

fn main() {
    let event_loop = EventLoop::new();
    let window = {
        let size = LogicalSize::new(WIDTH as f64, HEIGHT as f64);
        let scaled_size = LogicalSize::new(WIDTH as f64, HEIGHT as f64);
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
        PixelsBuilder::new(WIDTH, HEIGHT, texture)
            .build()
            .unwrap()
    };
    let mut sim = AntSimVecImpl::new(WIDTH as usize, HEIGHT as usize).unwrap();
    let ants = vec![
        Ant::new_default(sim.encode(AntPosition { x: 125, y: 125 }).unwrap(), 0.55); 1
    ];
    sim.set_cell(&sim.encode(AntPosition { x: 125, y: 125 }).unwrap(), AntSimCell::Home);
    sim.set_cell(&sim.encode(AntPosition { x: 90, y: 125 }).unwrap(), AntSimCell::Food { amount: u8::MAX });
    sim.set_cell(&sim.encode(AntPosition { x: 110, y: 125 }).unwrap(), AntSimCell::Food { amount: u8::MAX });
    let sim_config = AntSimConfig {
        distance_points: Box::new(POINTS.clone()),
        food_haul_amount: HAUL_AMOUNT,
        pheromone_decay_rate: DECAY_RATE,
        seed_step: 17,
        visual_range: AntVisualRangeBuffer::new(VISUAL_RANGE as usize)
    };
    let sim = AntSimulator {
        sim,
        ants,
        seed: SEED,
        decay_step: 0,
        config: sim_config
    };
    main_loop(event_loop, screen, sim);
}

fn main_loop(event_loop: EventLoop<()>, mut screen: Pixels, state: AntSimulator<AntSimVecImpl>) {
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
    event_loop.run(move |a, b, c| {
        if last_loop.elapsed() > threshold {
            if let Ok(state) = state.try_lock() {
                last_loop = Instant::now();
                draw_state(&state.1, &mut screen);
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

fn pixel_of_pos(frame: &mut [u8], pos: AntPosition) -> &mut [u8] {
    let AntPosition { x, y } = pos;
    let pix = y * WIDTH as usize + x;
    pixel(frame, pix)
}

fn render_hash(screen: &mut Pixels) {
    let frame = screen.get_frame();
    frame
        .chunks_exact_mut(4)
        .enumerate()
        .map(|(i, p)| (simple_hash((i / u8::MAX as usize) as u64, (i % u8::MAX as usize) as u64), p))
        .map(|(i, p)| (i as u8, p))
        .for_each(|(h, p)| {
            p.copy_from_slice(&[h, h, h, 0xFF]);
        });
    screen.render().unwrap();
}

fn draw_state<A: AntSim>(sim: &AntSimulator<A>, on: &mut Pixels) {
    let frame = on.get_frame();
    for cell in sim.sim.cells() {
        let (cell, pos): (AntSimCell, A::Position) = cell;
        let pos = sim.sim.decode(&pos);
        let pixel = pixel_of_pos(frame, pos);
        let color = match cell {
            AntSimCell::Path { pheromone_food, pheromone_home } => {
                [pheromone_food.get(), 0, pheromone_home.get(), 0xFF]
            }
            AntSimCell::Blocker => {
                [0xAF, 0xAF, 0xAF, 0xFF]
            }
            AntSimCell::Home => {
                [0xFF, 0xFF, 0x0F, 0xFF]
            }
            AntSimCell::Food { amount } => {
                [0, amount, 0, 0xFF]
            }
        };
        pixel.copy_from_slice(&color);
    }
    for ant in &sim.ants {
        let pos = sim.sim.decode(ant.position());
        let color = match ant.state(){
            AntState::Foraging => [0xFF, 0xFF, 0xFF, 0xFF],
            AntState::Hauling { amount }=> {
                let amount  = *amount * ((u8::MAX / 2) / sim.config.food_haul_amount);
                [0xFF - amount, 0xFF, 0xFF - amount, 0xFF]
            }
        };
        pixel_of_pos(frame, pos).copy_from_slice(&color);
    }
    on.render().unwrap();
}
