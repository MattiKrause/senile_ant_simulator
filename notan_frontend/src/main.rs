mod app_state;

use std::borrow::Cow;
use std::io::{Error, ErrorKind};
use std::mem::{replace, swap};
use std::time::{Duration, Instant};
use notan::app::GfxRenderer;
use notan::draw::*;
use notan::prelude::*;
use ant_sim::ant_sim::{AntSimConfig, AntSimulator, AntVisualRangeBuffer};
use ant_sim::ant_sim_frame::{AntPosition, AntSim, AntSimCell};
use ant_sim_save::save_subsystem::ReadSaveFileError;
use rgba_adapter::{ColorBuffer, RgbaBoxBuf};
use crate::app_state::*;

#[notan_main]
fn main() {
    notan::init_with(setup)
        .add_config(DrawConfig)
        .draw(draw)
        .event(event_handler)
        .update(update)
        .build()
        .unwrap();
    println!("Hello, world!");
}

static DEFAULT_FONT: &[u8] = include_bytes!("../assets/Roboto-Regular.ttf");
const DEFAULT_DELAY: Duration = Duration::from_millis(200);

fn setup(assets: &mut Assets, gfx: &mut Graphics) -> State {
    let (save_state, err) = match load_or_default_save_state() {
        Ok(ok) => (ok, None),
        Err(err) => (default_save_state(), Some(err)),
    };
    let back_texture = create_ant_texture(gfx, &save_state).expect("infallible texture creation");
    let mut edit_state = EditState::Edit(EditStateEdit { save_state, back_texture, draw: None,});
    edit_state = match err {
        None => edit_state,
        Some(error) => EditState::ErrorState(EditStateError { back_state: Box::new(edit_state), error, draw: None }),
    };
    let default_font = gfx.create_font(DEFAULT_FONT).expect("catastrophic failure: unable to load font");
    let resources = Resources { default_font };
    State {
        resources,
        edit_state,
    }
}

fn load_or_default_save_state() -> Result<AntSimulator<AntSimFrameImpl>, String> {
    let save = ant_sim_save::save_subsystem::SaveFileClass::read_save_from("ant_sim_saves/ant_sim_test_state.txt", |d| {
        let width = d.width.try_into().map_err(|_| ())?;
        let height = d.height.try_into().map_err(|_| ())?;
        AntSimFrameImpl::new(width, height).map_err(|_| ())
    });
    match save {
        Ok(sim) =>
            Ok(sim),
        Err(ReadSaveFileError::FileDoesNotExist | ReadSaveFileError::PathNotFile | ReadSaveFileError::FailedToRead(_)) =>
            Ok(default_save_state()),
        Err(ReadSaveFileError::InvalidData(err) | ReadSaveFileError::InvalidFormat(err)) => {
            Err(format!("default save state corrupted: {err}"))
        }
    }
}

#[inline(never)]
fn default_save_state() -> AntSimulator<AntSimFrameImpl> {
    const WIDTH: usize = 255;
    const HEIGHT: usize = 255;
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
    let mut sim = AntSimFrameImpl::new(255, 255).expect("infallible default creation");
    let home_pos = sim.encode(AntPosition { x: WIDTH / 2, y: HEIGHT / 2 }).expect("infallible default home");
    sim.set_cell(&home_pos, AntSimCell::Home);
    let ants = Vec::new();
    AntSimulator {
        sim,
        ants,
        seed: 42,
        config: AntSimConfig {
            distance_points: Box::new(POINTS1),
            food_haul_amount: 20,
            pheromone_decay_amount: 300,
            seed_step: 1,
            visual_range: AntVisualRangeBuffer::new(3),
        },
    }
}

fn event_handler(state: &mut State, event: Event) {
    match event {
        Event::KeyDown { key } => handle_char(state, key),
        _ => {}
    }
}

fn handle_char(state: &mut State, c: KeyCode) {
    match &mut state.edit_state {
        EditState::ErrorState(EditStateError { back_state, ..}) => {
            if c == KeyCode::Return {
                state.edit_state = replace(back_state.as_mut(), EditState::CorruptedState);
            }
        }
        EditState::Edit(EditStateEdit { save_state, back_texture, draw, .. }) => {
            match c {
                KeyCode::S => {
                    state.edit_state = EditState::Started(EditStateStarted {
                        save_state: GameState {
                            sim1: save_state.clone(),
                            sim2: save_state.clone(),
                        },
                        back_texture: AntSimTexture {
                            texture: back_texture.texture.clone(),
                            buf: replace(&mut back_texture.buf, RgbaBoxBuf::from_pixels(0)),
                            dirty: false
                        },
                        delay: DEFAULT_DELAY,
                        last_updated: Instant::now(),
                        draw: None,
                        paused: false
                    })
                }
                _ => {}
            }
        }
        EditState::Started(state) => {
            match c {
                KeyCode::Right => update_game_state(state),
                KeyCode::P => {
                    state.paused = !state.paused;
                    state.draw = None;
                },
                c => {
                    if let Some(delay) = handle_delay_set(c) {
                        state.delay = delay;
                        state.draw = None;
                    }
                }
            };
        }
        EditState::CorruptedState => unreachable!()
    }
}

fn handle_delay_set(c: KeyCode) -> Option<Duration> {
    let delay = match c {
        KeyCode::Key1 => 10,
        KeyCode::Key2 => 20,
        KeyCode::Key3 => 50,
        KeyCode::Key4 => 100,
        KeyCode::Key5 => 200,
        KeyCode::Key6 => 500,
        KeyCode::Key7 => 750,
        KeyCode::Key8 => 1000,
        KeyCode::Key9 => 2000,
        KeyCode::Key0 => 0,
        _ => return None,
    };
    return Some(Duration::from_millis(delay))
}

fn update(app: &mut App, state: &mut State) {
    match &mut state.edit_state {
        EditState::CorruptedState => unreachable!(),
        EditState::ErrorState(_) => {}
        EditState::Edit(_) => {}
        EditState::Started(s) => {
            if s.last_updated.elapsed() >= s.delay && (!s.paused){
                update_game_state(s);
                s.last_updated = Instant::now();
            }
        }
    }
}

fn update_game_state(s: &mut EditStateStarted) {
    let GameState { sim1, sim2 } = &mut s.save_state;
    sim1.update(sim2);
    swap(sim1, sim2);
    s.back_texture.dirty = true;
}

fn draw(gfx: &mut Graphics, plugins: &mut Plugins, state: &mut State) {
    fn draw_err_state(gfx: &mut Graphics, draw: &mut Draw, resources: &Resources, state: &mut EditStateError) {
        match state.back_state.as_mut() {
            EditState::ErrorState(s) =>
                draw_err_state(gfx, draw, resources, s),
            EditState::Edit(s) =>
                draw_edit_state(gfx, draw, s),
            EditState::Started(s) =>
                draw_game_state(gfx, draw,resources, s),
            EditState::CorruptedState => unreachable!()
        }
        err_popup(draw, &resources.default_font, &state.error);
    }
    fn draw_edit_state(gfx: &mut Graphics, draw: &mut Draw, state: &mut EditStateEdit) {
        fit_ant_sim_texture(draw, &mut state.back_texture.texture);
    }
    fn draw_game_state(gfx: &mut Graphics, draw: &mut Draw, resources: &Resources, state: &mut EditStateStarted) {
        let width = draw.width();
        let height = draw.height();
        fit_ant_sim_texture(draw, &mut state.back_texture.texture);
        let show_text = game_speed_text(state);
        draw.text(&resources.default_font, show_text.as_ref())
            .size(10.0)
            .color(Color::WHITE)
            .position(width * 0.90, height * 0.1);
    }
    match &mut state.edit_state {
        EditState::ErrorState(s) => {
            if let Some(ref d) = s.draw {
                gfx.render(d);
                return;
            }
            let mut draw = gfx.create_draw();
            draw.rect((0.0, 0.0),draw.size()).color(Color::BLACK);
            draw_err_state(gfx, &mut draw, &state.resources, s);
            gfx.render(&draw);
            s.draw = Some(draw);
            return;
        }
        EditState::Edit(s) => {
            if let Some(ref d) = s.draw {
                gfx.render(d);
                return;
            }
            let mut draw = gfx.create_draw();
            draw.clear(Color::BLACK);
            draw_edit_state(gfx, &mut draw, s);
            gfx.render(&draw);
            s.draw = Some(draw);
            s.draw.as_ref().unwrap();
        }
        EditState::Started(s) => {
            if s.back_texture.dirty {
                update_ant_texture(&mut s.back_texture, &s.save_state.sim1, gfx);
            }
            if let Some(ref d) = s.draw {
                gfx.render(d);
                return;
            }
            let mut draw = gfx.create_draw();
            draw.clear(Color::BLACK);
            draw_game_state(gfx, &mut draw, &state.resources, s);
            gfx.render(&draw);
            s.draw = Some(draw);
            return;
        }
        EditState::CorruptedState => unreachable!(),
    }
}

fn game_speed_text(state: &EditStateStarted) -> Cow<'static, str> {
    if state.paused {
        Cow::Borrowed("Paused")
    }  else if state.delay == Duration::ZERO {
        Cow::Borrowed("Fastest")
    } else {
        Cow::Owned(format!("{}s", state.delay.as_secs_f64()))
    }

}

fn update_ant_texture<A: AntSim>(target: &mut AntSimTexture, sim: &AntSimulator<A>, gfx: &mut Graphics) {
    target.dirty = false;
    use rgba_adapter::SetRgb;
    assert_eq!(target.buf.buf_ref().len(), sim.sim.width() * sim.sim.height());
    rgba_adapter::draw_to_buf(sim, target.buf.buf_ref());
    gfx.update_texture(&mut target.texture).with_data(target.buf.buf_ref().into_ref()).update().unwrap();
}

fn create_ant_texture<A: AntSim>(gfx: &mut Graphics, sim: &AntSimulator<A>) -> Result<AntSimTexture, String> {
    let size = sim.sim.width() * sim.sim.height();
    let width = sim.sim.width().try_into().map_err(|_| String::from("width too large"))?;
    let height = sim.sim.height().try_into().map_err(|_| String::from("height too large"))?;
    let mut buf = RgbaBoxBuf::from_pixels(size);
    rgba_adapter::draw_to_buf(sim, buf.buf_ref());
    gfx.create_texture()
        .from_bytes(buf.buf_ref().into_ref(), width, height)
        .build()
        .map(|texture| AntSimTexture { texture, buf, dirty: false })
}

fn fit_ant_sim_texture(draw: &mut Draw, texture: &Texture) {
    let s = if draw.width() < draw.height() { draw.width() } else { draw.height() };
    draw.image(texture).size(s, s);
}

fn err_popup(gfx: &mut Draw, font: &Font, err: &str) {
    let (width, height) = gfx.size();
    gfx.rect((width * 0.25, height * 0.3), (width * 0.5, height * 0.3))
        .color(Color::RED.with_alpha(0.5))
        .blend_mode(BlendMode::OVER)
        .fill();
    gfx.text(font, err)
        .color(Color::BLACK)
        .position(width * 0.3, height * 0.35);
    gfx.text(&font, "Press enter to continue")
        .color(Color::BLACK)
        .position(width * 0.3, height * 0.4);
}