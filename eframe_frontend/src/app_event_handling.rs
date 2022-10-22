use std::fmt::Write;
use std::mem::replace;
use std::str::FromStr;
use egui::{TextureFilter, TextureHandle};
use rand::{Rng, SeedableRng};
use ant_sim::ant_sim::AntSimulator;
use ant_sim::ant_sim_ant::{Ant, AntState};
use ant_sim::ant_sim_frame::{AntPosition, AntSim, AntSimCell};
use ant_sim::ant_sim_frame_impl::NewAntSimVecImplError;
use crate::{AntSimFrame, AppState};
use crate::app::{AppEvents, BrushMaterial, BrushType, GameState, GameStateEdit, POINTS_R1};
use crate::load_file_service::LoadFileMessages;
use crate::service_handle::{ServiceHandle};
use crate::sim_update_service::{SimUpdaterMessage, SimUpdateService};

pub fn handle_events(state: &mut AppState, _ctx: &egui::Context) {
    macro_rules! resume_if_present {
            ($service: expr) => {
                if let Some(service) = replace(&mut $service, None) {
                    service
                } else {
                    continue;
                }
            };
        }
    macro_rules! resume_if_condition {
            ($cond: expr) => {
                if !$cond {
                    continue;
                }
            };
        }
    let mut event_query = state.mailbox.try_recv();
    while let Ok(event) = event_query {
        log::debug!(target: "App", "{event:?}");
        event_query = state.mailbox.try_recv();
        match event {
            AppEvents::ReplaceSim(ant_sim) => {
                log::debug!(target: "App", "Received new simulation instance");
                match ant_sim {
                    Ok(res) => {
                        repaint(res.as_ref(), &mut state.game_image);
                        state.game_state = GameState::Edit(Box::new(GameStateEdit::new(res)));
                        if let Some(update) = replace(&mut state.services.update, None) {
                            if let Ok(service) = update.try_send(SimUpdaterMessage::Pause(true)) {
                                state.services.update = Some(service.0);
                            } else {
                                panic!("services down!")
                            }
                        }
                    }
                    Err(err) => {
                        state.error_stack.push(format!("Failed to load save: {err}"));
                    }
                }
            }
            AppEvents::NewStateImage(image) => {
                log::debug!("test");
                state.game_image.set(image, TextureFilter::Nearest);
                _ctx.request_repaint();
            }
            AppEvents::SetPreferredSearchPath(path) => {
                state.preferred_path = Some(path);
            }
            AppEvents::CurrentVersion(sim) => {
                log::debug!(target: "App", "received new version");
                #[cfg(not(target_arch = "wasm32"))]
                if state.save_requested {
                    state.save_requested = false;
                    let file_service = resume_if_present!(state.services.load_file);
                    let mut prompt_builder = rfd::AsyncFileDialog::new()
                        .set_file_name("ant_sim_save.txt")
                        .set_title("save simulation state");
                    if let Some(path) = state.preferred_path.as_ref().and_then(|path| path.parent()) {
                        prompt_builder = prompt_builder.set_directory(path);
                    }
                    let prompt = prompt_builder.save_file();
                    match file_service.try_send(LoadFileMessages::SaveStateMessage(Box::pin(prompt), sim)) {
                        Ok((service, _)) => {
                            state.services.load_file = Some(service);
                        }
                        Err(_) => {
                            log::warn!("File services down!");
                        }
                    };
                }
                #[cfg(target_arch = "wasm32")]
                if state.save_requested {
                    state.save_requested = false;
                    let file_service = resume_if_present!(state.services.load_file);
                    match file_service.try_send(LoadFileMessages::DownloadStateMessage(sim)) {
                        Ok((service, _)) => {
                            state.services.load_file = Some(service);
                        }
                        Err(_) => {
                            log::warn!("File services down!");
                        }
                    };
                }
            }
            AppEvents::Error(err) => {
                state.error_stack.push(err);
            }
            AppEvents::RequestPause => {
                resume_if_condition!(matches!(state.game_state, GameState::Launched));
                let update_service = resume_if_present!(state.services.update);
                state.game_speed.paused = !state.game_speed.paused;
                log::debug!(target: "App", "pause state: {}", state.game_speed.paused);
                match update_service.try_send(SimUpdaterMessage::Pause(state.game_speed.paused)) {
                    Ok((service, _)) => {
                        state.services.update = Some(service);
                    }
                    Err(_) => {}
                }
            }
            AppEvents::DelayRequest(new_delay) => {
                state.game_speed.delay = new_delay;
                let update_service = resume_if_present!(state.services.update);
                let send_result = update_service.try_send(SimUpdaterMessage::SetDelay(new_delay));
                match send_result {
                    Ok((actor, _)) => {
                        state.services.update = Some(actor);
                    }
                    Err(_) => {}
                }
            }
            AppEvents::RequestLoadGame => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(service) = replace(&mut state.services.load_file, None) {
                    let mut prompt_builder = rfd::AsyncFileDialog::new().set_title("Load save state");
                    if let Some(path) = state.preferred_path.as_ref().and_then(|path| path.parent()) {
                        prompt_builder = prompt_builder.set_directory(path);
                    }
                    let prompt = prompt_builder.pick_file();
                    match service.try_send(LoadFileMessages::LoadFileMessage(Box::pin(prompt))) {
                        Ok(ready) => {
                            state.services.load_file = Some(ready.0);
                        }
                        Err(_) => {
                            log::warn!(target:"App", "LoadFileService failed")
                        }
                    }
                }
            }
            AppEvents::RequestSaveGame => {
                state.save_requested = true;
                match &state.game_state {
                    GameState::Launched => {
                        let update_service = resume_if_present!(state.services.update);
                        match update_service.try_send(SimUpdaterMessage::RequestCurrentState) {
                            Ok((c, _)) => {
                                state.services.update = Some(c);
                            }
                            Err(_) => {
                                panic!("update service down");
                            }
                        }
                    }
                    GameState::Edit(edit) => {
                        state.send_me(AppEvents::CurrentVersion(edit.sim.clone()));
                    }
                }
            }
            AppEvents::RequestLaunch => {
                let edit_state = if matches!(state.game_state, GameState::Edit(_)) {
                    match replace(&mut state.game_state, GameState::Launched) {
                        GameState::Edit(e) => e,
                        _ => unreachable!(),
                    }
                } else {
                    continue;
                };
                let update_service = replace(&mut state.services.update, None)
                    .and_then(|service| service.try_send(SimUpdaterMessage::NewSim(edit_state.sim)).ok())
                    .and_then(|(service, _)| service.try_send(SimUpdaterMessage::Pause(false)).ok())
                    .expect("update service down")
                    .0;
                state.services.update = Some(update_service);
            }
            AppEvents::RequestSetBoardWidth => {
                let GameState::Edit(ref mut edit) = state.game_state else { continue; };
                let width_text = edit.width_text_buffer.trim();
                let width_num = match usize::from_str(width_text) {
                    Ok(num) => num,
                    Err(_) => {
                        edit.width_text_buffer = edit.sim.sim.width().to_string();
                        continue;
                    }
                };
                let new_board = AntSimFrame::new(width_num, edit.sim.sim.height());
                let mut new_board = match new_board {
                    Ok(board) => board,
                    Err(err) => {
                        let err_str = match err {
                            NewAntSimVecImplError::DimensionZero =>
                                "The new board contains no pixels",
                            NewAntSimVecImplError::DimensionTooLarge | NewAntSimVecImplError::OutOfMemory =>
                                "The new board's dimensions are too large"
                        };
                        edit.width_text_buffer = edit.sim.sim.width().to_string();
                        state.error_stack.push(err_str.to_string());
                        continue;
                    }
                };
                translate_sim(&edit.sim.sim, &mut new_board);
                edit.sim.ants = edit.sim.ants.iter()
                    .map(|ant| clamp_ant_pos(ant, &edit.sim.sim, &new_board))
                    .collect();
                edit.sim.sim = new_board;
                repaint(edit.sim.as_ref(), &mut state.game_image);
            }
            AppEvents::RequestSetBoardHeight => {
                let GameState::Edit(ref mut edit) = state.game_state else { continue; };
                let height_text = edit.height_text_buffer.trim();
                let height_num = match usize::from_str(height_text) {
                    Ok(num) => num,
                    Err(_) => {
                        edit.height_text_buffer = edit.sim.sim.height().to_string();
                        continue;
                    }
                };
                let new_board = AntSimFrame::new(edit.sim.sim.width(), height_num);
                let mut new_board = match new_board {
                    Ok(board) => board,
                    Err(err) => {
                        let err_str = match err {
                            NewAntSimVecImplError::DimensionZero =>
                                "The new board contains no pixels",
                            NewAntSimVecImplError::DimensionTooLarge | NewAntSimVecImplError::OutOfMemory =>
                                "The new board's dimensions are too large"
                        };
                        edit.height_text_buffer = edit.sim.sim.height().to_string();
                        state.error_stack.push(err_str.to_string());
                        continue;
                    }
                };
                translate_sim(&edit.sim.sim, &mut new_board);
                edit.sim.ants = edit.sim.ants.iter()
                    .map(|ant| clamp_ant_pos(ant, &edit.sim.sim, &new_board))
                    .collect();
                edit.sim.sim = new_board;
                repaint(edit.sim.as_ref(), &mut state.game_image);
            }
            AppEvents::RequestSetSeed => {
                let GameState::Edit(ref mut edit) = state.game_state else { continue; };
                let seed_text = edit.seed_text_buffer.trim();
                if seed_text.len() > 19 {
                    state.error_stack.push(String::from("The seed can only be at most 19 digits long!"));
                    edit.seed_text_buffer = edit.sim.seed.to_string();
                    continue;
                }
                match u64::from_str(seed_text) {
                    Ok(seed) => {
                        edit.sim.seed = seed;
                    }
                    Err(_) => {
                        state.error_stack.push(String::from("The seed must consist of 1-19 digits"));
                        edit.seed_text_buffer = edit.sim.seed.to_string();
                        continue;
                    }
                };
            }
            AppEvents::PaintStroke { from, to } => {
                let GameState::Edit(ref mut edit) = state.game_state else { continue; };
                let BrushMaterial::Cell(ref cell) = edit.brush_material else { continue };
                paint_stroke(from, to, cell.clone(), &edit.brush_form, &mut edit.sim.sim);
                repaint(edit.sim.as_ref(), &mut state.game_image);
            }
            AppEvents::SetBrushType(b) => {
                let GameState::Edit(ref mut edit) = state.game_state else { continue; };
                let new_brush = match b {
                    BrushType::Circle(c) => {
                        Brush::new_circle(c)
                    }
                };
                edit.brush_form = new_brush;
            }
            AppEvents::SetBrushMaterial(cell) => {
                let GameState::Edit(ref mut edit) = state.game_state else { continue; };
                edit.brush_material = cell;
            }
            AppEvents::ImmediateNextFrame => {
                resume_if_condition!(matches!(state.game_state, GameState::Launched));
                let frame = resume_if_present!(state.services.update);
                match frame.try_send(SimUpdaterMessage::ImmediateNextFrame) {
                    Ok((service, _)) => {
                        state.services.update = Some(service)
                    }
                    Err(_) => {}
                }
            }
            AppEvents::BoardClick(click) => {
                let GameState::Edit(ref mut edit) = state.game_state else {
                    continue;
                };
                let pos = click.map(|c| c as usize);
                let pos = AntPosition {
                    x: pos[0],
                    y: pos[1]
                };
                let Some(pos) = edit.sim.sim.encode(pos) else { continue; };
                match edit.brush_material {
                    BrushMaterial::AntSpawn => {
                        let mut seed = [0u8; 32];
                        let copy_value = edit.sim.seed + edit.sim.ants.len() as u64;
                        seed.chunks_mut(8).for_each(|chunk| chunk.copy_from_slice(&edit.sim.seed.to_le_bytes()));
                        let eweight = rand::prelude::StdRng::from_seed(seed).gen_range(0.55..0.65);
                        let ant = Ant::new(pos.clone(), pos, eweight, AntState::Foraging);
                        edit.sim.ants.push(ant);
                    }
                    BrushMaterial::AntKill => {
                        let ant = edit.sim.ants.iter().map(Ant::position)
                            .enumerate()
                            .filter(|ant_pos| ant_pos.1 == &pos)
                            .last();
                        if let Some((i, _)) = ant {
                            edit.sim.ants.remove(i);
                        }
                    }
                    _ => continue,
                };
                repaint(&edit.sim, &mut state.game_image);

            }
            AppEvents::RequestSetPointsRadius => {
                let GameState::Edit(ref mut edit) = state.game_state else { continue };
                let r = edit.points_radius_buf;
                let res = r.is_finite().then_some(r).ok_or_else(|| "The points radius is not final");
                let r= match res {
                    Ok(r) => r,
                    Err(err) => {
                        state.error_stack.push(err.to_string());
                        continue;
                    }
                };
                edit.sim.config.distance_points = Box::new(POINTS_R1.map(|p| (p.0 *r, p.1 * r)));
            }
        }
    }
    if let Err(async_std::channel::TryRecvError::Closed) = event_query {
        panic!("services down!");
    }
}

fn with_points_on_line(from: [f32; 2], to: [f32; 2], mut with: impl FnMut(AntPosition)) {
    let from = from.map(|c| c as usize);
    let to = to.map(|c| c as usize);
    let (dx, ix) = if from[0] <= to[0] {
        (to[0] - from[0], 1)
    } else {
        (from[0] - to[0], usize::MAX)
    };
    let dx = dx as isize;
    let (dy, iy) = if from[1] <= to[1] {
        (to[1] - from[1], 1)
    } else {
        (from[1] - to[1], usize::MAX)
    };
    let dy = -(dy as isize);
    let mut current = AntPosition { x: from[0] as usize, y: from[1] as usize};
    let mut error = dx + dy;
    loop {
        with(current);
        let break_cond = current.x == to[0];
        let e2 = error * 2;
        let e2_larger_dy = e2 >= dy;
        error = error.wrapping_add(dy * (e2_larger_dy as isize));
        current.x = current.x.wrapping_add(ix * (e2_larger_dy as usize));
        if break_cond && e2_larger_dy { break; }
        let e2_smaller_dx = e2 <= dx;
        error = error.wrapping_add(dx * (e2_smaller_dx as isize));
        if (current.y == to[1]) & (break_cond | e2_smaller_dx) { break; }
        current.y = current.y.wrapping_add(iy * (e2_smaller_dx as usize));
    }
}

#[inline(never)]
fn paint_stroke(from: [f32; 2], to: [f32; 2], cell: AntSimCell, brush: &Brush, on: &mut AntSimFrame) {
    /*let from = egui::Vec2::from(from);
    let to = egui::Vec2::from(to);
    let step = (to - from).normalized();
    dbg!(from);
    let mut step_zero = step == egui::Vec2::new(0.0, 0.0);
    let mut current = from;
    //let mut result = Vec::with_capacity(((from - to).length().ceil() +  1.0) as usize);

    while (current - to).length() > ((current + step) - to).length() || step_zero {
        let pixel = current.floor();
        step_zero = false;
        current += step;
        let x = pixel.x as usize;
        let y = pixel.y as usize;
        let Some(pos) = on.encode(AntPosition { x, y }) else {
            continue;
        };
        on.set_cell(&pos, AntSimCell::Food { amount: u16::MAX  - 1 })
    }*/
    with_points_on_line(from, to, |current| {
        for pos in brush.apply_to_pos(current) {
            let Some(pos) = on.encode(pos) else { continue };
            on.set_cell(&pos, cell.clone());
        }
    });
}

#[inline(never)]
fn repaint(sim: &AntSimulator<AntSimFrame>, tex: &mut TextureHandle) {
    tex.set(SimUpdateService::sim_to_image(sim), TextureFilter::Nearest);
}

fn translate_sim(from: &AntSimFrame, into: &mut AntSimFrame) {
    for (cell, pos) in from.cells() {
        let Some(new_pos) = into.encode(from.decode(&pos)) else { continue; };
        into.set_cell(&new_pos, cell);
    }
}

fn clamp_ant_pos<A: AntSim>(ant: &Ant<A>, from: &A, sim: &A) -> Ant<A> {
    let mut ant_position = from.decode(&ant.position);
    let mut last_ant_position = from.decode(&ant.last_position);
    macro_rules! clamp_coord {
        ($coord: ident, $max: ident) => {
            if ant_position.$coord >= sim.$max() {
                let pos = sim.$max() - 1;
                if last_ant_position.$coord >= ant_position.$coord {
                    last_ant_position.$coord = pos;
                } else {
                    let diff = ant_position.$coord - last_ant_position.$coord;
                    last_ant_position.$coord = pos.saturating_sub(diff);
                }
                ant_position.$coord = pos;
            }
        };
    }
    clamp_coord!(x, width);
    clamp_coord!(y, height);
    let encoded_pos = sim.encode(ant_position)
        .expect("failed to safely encode ant position");
    let encoded_last_pos = sim.encode(last_ant_position)
        .expect("failed to safely encode ant position");
    Ant::new(encoded_pos, encoded_last_pos, ant.explore_weight, ant.state)
}

pub struct Brush {
    positions: Box<[[usize; 2]]>
}

impl Brush {
    pub fn new_circle(radius: usize) -> Self {
        fn circle_part(start_x: usize, start_y: usize, off_x: usize, off_y: usize, add_to: &mut Vec<[usize; 2]>) {
            let dir_x = [off_x, 0usize.wrapping_sub(off_x)];
            let dir_y = [off_y, 0usize.wrapping_sub(off_y)];
            for off in dir_x {
                let y = start_y.wrapping_add(off);
                let left = start_x - off_y;
                let right = start_x + off_y;
                add_to.extend((left..=right).map(|x| [x, y]));
            }
            for off in dir_y {
                let y = start_y.wrapping_add(off);
                let left = start_x - off_x;
                let right = start_y + off_x;
                add_to.extend((left..=right).map(|x| [x, y]));
            }
        }
        if radius == 0 {
            return Self {
                positions: Box::new([])
            }
        }
        if radius == 1 {
            return Self {
                positions: Box::new([[0; 2]])
            }
        }
        let radius = radius - 1;
        let mut x = 0;
        let mut y = radius;
        let mut d = 3 - 2*(radius as isize);
        let mut points = Vec::new();
        circle_part(radius, radius,  x, y, &mut points);
        while y >= x {
            x += 1;
            if d > 0 {
                y -= 1;
                d += 4 * (x.wrapping_sub(y)  as isize) + 10;
            } else {
                d += 4 * x as isize + 6;
            }
            circle_part(radius, radius, x, y,  &mut points);
        }
        points.iter_mut().for_each(|p| {
            p[0] = p[0].wrapping_sub(radius);
            p[1] = p[1].wrapping_sub(radius);
        });

        Self {
            positions: points.into_boxed_slice()
        }
    }
    fn apply_to_pos<'s>(&'s self, pos: AntPosition) -> impl Iterator<Item = AntPosition> + 's{
        self.positions.as_ref().iter().copied().map(move |[x, y]| AntPosition {
            x: pos.x.wrapping_add(x),
            y: pos.y.wrapping_add(y)
        })
    }
}