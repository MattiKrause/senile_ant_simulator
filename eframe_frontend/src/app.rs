use std::mem::replace;
use std::path::PathBuf;
use std::time::Duration;
use async_std::channel::{Receiver as ChannelReceiver, Sender as ChannelSender};
use eframe::emath::Align;
use eframe::epaint::textures::TextureFilter;
use egui::*;
use ant_sim::ant_sim::{AntSimConfig, AntSimulator, AntVisualRangeBuffer};
use ant_sim::ant_sim_frame::{AntSim, AntSimCell, NonMaxU16};
use ant_sim::ant_sim_frame_impl::{AntSimVecImpl};
use crate::app_event_handling::{Brush, handle_events};
use crate::app_services::{load_file_service, Services, update_service};
use crate::load_file_service::{DroppedFileMessage, LoadFileMessages};
use crate::service_handle::{ServiceHandle};
use crate::sim_update_service::{SimUpdateService};

type AntSimFrame = AntSimVecImpl;

pub enum AppEvents {
    ReplaceSim(Result<Box<AntSimulator<AntSimFrame>>, String>),
    NewStateImage(ImageData),
    SetPreferredSearchPath(PathBuf),
    CurrentVersion(Box<AntSimulator<AntSimFrame>>),
    Error(String),
    RequestPause,
    DelayRequest(Duration),
    RequestLoadGame,
    RequestSaveGame,
    RequestLaunch,
    RequestSetBoardWidth,
    RequestSetBoardHeight,
    RequestSetSeed,
    PaintStroke {
        from: [f32; 2],
        to: [f32; 2],
    },
    SetBrushType(BrushType),
    SetBrushMaterial(BrushMaterial),
    ImmediateNextFrame,
    BoardClick([f32; 2]),
    RequestSetPointsRadius
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum BrushType {
    Circle(usize)
}
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum BrushMaterial {
    Cell(AntSimCell),
    AntSpawn,
    AntKill
}
/// We derive Deserialize/Serialize so we can persist app state on shutdown.
pub struct AppState {
    pub game_image: TextureHandle,
    pub mailbox: ChannelReceiver<AppEvents>,
    pub error_stack: Vec<String>,
    pub save_requested: bool,
    pub preferred_path: Option<PathBuf>,
    pub game_state: GameState,
    pub input_locked: bool,
    pub game_speed: GameSpeed,
    // Example stuff:
    pub label: String,

    // this how you opt-out of serialization of a member
    pub value: f32,
    pub services: Services,
}

pub enum GameState {
    Launched,
    Edit(Box<GameStateEdit>),
}

pub struct GameStateEdit {
    pub sim: Box<AntSimulator<AntSimFrame>>,
    pub show_side_panel: bool,
    pub brush_form: Brush,
    pub brush_material: BrushMaterial,
    pub width_text_buffer: String,
    pub height_text_buffer: String,
    pub seed_text_buffer: String,
    pub points_radius_buf: f64,
    pub brush_circle_radius: usize,
}

impl GameStateEdit {
    pub fn new(sim: Box<AntSimulator<AntSimFrame>>) -> Self {
        Self {
            show_side_panel: false,
            brush_form: Brush::new_circle(1),
            brush_material: BrushMaterial::Cell(AntSimCell::Path { pheromone_food: NonMaxU16::new(0), pheromone_home: NonMaxU16::new(0) }),
            width_text_buffer: sim.sim.width().to_string(),
            height_text_buffer: sim.sim.height().to_string(),
            seed_text_buffer: sim.seed.to_string(),
            points_radius_buf: try_classify_points_radius_from(&sim.config.distance_points).unwrap_or(f64::NAN),
            sim,
            brush_circle_radius: 1,
        }
    }
}

pub struct GameSpeed {
    pub paused: bool,
    pub delay: Duration,
}

impl AppState {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {

        // This is also where you can customized the look at feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        /*if let Some(instance) = cc.storage.and_then(|storage| eframe::get_value(storage, eframe::APP_KEY)) {
            return instance;
        }*/
        Self::create_new(cc)
    }

    fn create_new(cc: &eframe::CreationContext<'_>) -> Self {
        let ant_sim = default_ant_sim();
        let colored_image = SimUpdateService::sim_to_image(&ant_sim);
        let texture = cc.egui_ctx.load_texture("ant_sim background", colored_image, TextureFilter::Nearest);
        let mailbox = async_std::channel::unbounded();
        let services = Services {
            load_file: load_file_service(mailbox.0.clone(), cc.egui_ctx.clone()),
            update: update_service(mailbox.0.clone(), Duration::from_millis(200), default_ant_sim(), true, cc.egui_ctx.clone()),
            mailbox_in: mailbox.0,
        };
        AppState {
            game_image: texture,
            mailbox: mailbox.1,
            error_stack: Vec::new(),
            save_requested: false,
            preferred_path: None,
            game_state: GameState::Edit(Box::new(GameStateEdit::new(Box::new(ant_sim)))),
            input_locked: false,
            game_speed: GameSpeed { paused: false, delay: Duration::from_millis(200) },
            label: "lbl".to_string(),
            value: 42.0,
            services,
        }
    }

    pub fn send_me(&self, event: AppEvents) {
        let _ = ChannelSender::try_send(&self.services.mailbox_in, event);
    }

    fn handle_dropped_file(&mut self, files: &[DroppedFile]) {
        if files.len() > 0 {
            log::debug!(target: "App", "files dropped: {:?}", files.iter().map(|f|&f.name).collect::<Vec<_>>())
        }
        if files.len() > 1 {
            self.error_stack.push(String::from("please drop only one file at once"));
            return;
        }
        let file = if let Some(file) = files.first() {
            file
        } else {
            return;
        };
        log::debug!(target: "App", "file {} was dropped", file.name);
        let service = if let Some(service) = replace(&mut self.services.load_file, None) {
            service
        } else {
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
            let message = file.path.clone().map(|path_buf| DroppedFileMessage { path_buf });
        #[cfg(target_arch = "wasm32")]
            let message = file.bytes.clone().map(|bytes| DroppedFileMessage { bytes });
        if let Some(m) = message {
            let send_res = service.try_send(LoadFileMessages::DroppedFileMessage(m));
            match send_res {
                Ok(res) => {
                    self.services.load_file = Some(res.0);
                }
                Err(_) => {
                }
            }
        } else {
            log::warn!(target: "LoadFileService", "failed to handle file");
        }
    }


    fn handle_input(&mut self, ctx: &egui::Context) {
        let input = ctx.input();
        self.handle_dropped_file(&input.raw.dropped_files);
        #[cfg(not(target_arch = "wasm32"))]
        if input.modifiers.ctrl && input.key_pressed(egui::Key::L) {
            self.send_me(AppEvents::RequestLoadGame);
        }
        if input.modifiers.ctrl && input.key_pressed(egui::Key::S) {
            let _ = self.send_me(AppEvents::RequestSaveGame);
        }
        if self.input_locked { return; }
        let new_delay = input.events.iter()
            .filter_map(|e| if let egui::Event::Key { key, pressed, modifiers } = e {
                Some((key, *pressed, modifiers))
            } else {
                None
            })
            .filter(|(_, pressed, _)| *pressed)
            .filter_map(|(key, _, _)| Self::map_key_to_frame_delay(key))
            .next();
        if let Some(new_delay) = new_delay {
            let _ = self.send_me(AppEvents::DelayRequest(new_delay));
        }
        if matches!(&self.game_state, GameState::Launched) {
            if input.events.iter().any(|e| matches!(e, egui::Event::Key { key: Key::P, .. })) {
                let _ = self.send_me(AppEvents::RequestPause);
            }
        }
        if input.key_pressed(Key::P) && matches!(self.game_state, GameState::Launched) {
            self.send_me(AppEvents::RequestPause);
        }
        input.events.iter()
            .filter_map(|event| if let Event::Key { key, pressed, ..} = event {
                pressed.then_some(key)
            } else {
                None
            })
            .filter_map(|key| match key {
                Key::C => Some(AntSimCell::Path { pheromone_food: NonMaxU16::new(0), pheromone_home: NonMaxU16::new(0) }),
                Key::B => Some(AntSimCell::Blocker),
                Key::H => Some(AntSimCell::Home),
                Key::F => Some(AntSimCell::Food {
                    amount: u16::MAX
                }),
                _ => None,
            })
            .take(1)
            .map(BrushMaterial::Cell)
            .for_each(|key| self.send_me(AppEvents::SetBrushMaterial(key)));
        if input.key_pressed(Key::A) {
            self.send_me(AppEvents::SetBrushMaterial(BrushMaterial::AntSpawn));
        } else if input.key_pressed(Key::K) {
            self.send_me(AppEvents::SetBrushMaterial(BrushMaterial::AntKill))
        }
        if input.key_pressed(Key::ArrowRight) {
            self.send_me(AppEvents::ImmediateNextFrame);

        }
    }

    fn map_key_to_frame_delay(key: &egui::Key) -> Option<Duration> {
        let delay_millis = match key {
            Key::Num1 => 10,
            Key::Num2 => 20,
            Key::Num3 => 50,
            Key::Num4 => 100,
            Key::Num5 => 200,
            Key::Num6 => 500,
            Key::Num7 => 700,
            Key::Num8 => 1000,
            Key::Num9 => 3000,
            Key::Num0 => 0,
            _ => return None
        };
        Some(Duration::from_millis(delay_millis))
    }

    fn edit_side_panel(&mut self, ctx: &egui::Context) {
        macro_rules! send_me {
            ($message: expr) => {
                let _ = ChannelSender::try_send(&self.services.mailbox_in, $message);
            };
        }
        let e = if let GameState::Edit(ref mut e) = self.game_state {
            e
        } else {
            return;
        };
        let GameStateEdit { sim, width_text_buffer, height_text_buffer, seed_text_buffer, brush_circle_radius, brush_material, points_radius_buf,.. } = e.as_mut();
        let input_locked = &mut self.input_locked;
        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.heading("Edit game values");
            ui.horizontal(|ui| {
                ui.label("width: ");
                let width = ui.text_edit_singleline(width_text_buffer);
                if width.gained_focus() {
                    *input_locked = true;
                }
                if width.lost_focus() {
                    *input_locked = false;
                    send_me!(AppEvents::RequestSetBoardWidth);
                }
                width.on_hover_text("set the width of the board")
            });
            ui.horizontal(|ui| {
                ui.label("height: ");
                let height = ui.text_edit_singleline(height_text_buffer);
                if height.gained_focus() {
                    *input_locked = true;
                }
                if height.lost_focus() {
                    *input_locked = false;
                    send_me!(AppEvents::RequestSetBoardHeight);
                }
                height.on_hover_text("Set the height of the board")
            });
            ui.horizontal(|ui| {
                ui.label("seed: ");
                let seed = ui.text_edit_singleline(seed_text_buffer);
                if seed.gained_focus() {
                    *input_locked = true;
                }
                if seed.lost_focus() {
                    *input_locked = false;
                    send_me!(AppEvents::RequestSetSeed);
                }
                seed.on_hover_text("controls the seed of the game; A different seed will lead to different actions performed by the ants")
            });
            ui.horizontal(|ui| {
                ui.label("ant count: ");
                let mut dmp = sim.ants.len().to_string();
                ui.add_enabled(false, egui::TextEdit::singleline(&mut dmp).interactive(false));
            });
            ui.horizontal(|ui| {
                ui.label("stubbornness");
                let slider = egui::Slider::new(points_radius_buf, 0.0..=5.0).ui(ui);
                slider.on_hover_text(String::from("Determines the likelihood, with which the ant will turn, a low value means the ant is more prone to running in cicrles"))
            });
            ui.horizontal(|ui| {
                ui.label("brush radius: ");
                let seed = egui::Slider::new(brush_circle_radius, 1..=100).ui(ui);
                if seed.changed() {
                    send_me!(AppEvents::SetBrushType(BrushType::Circle(*brush_circle_radius)));
                }
            });
            ui.horizontal(|ui| {
                ui.label("brush kind: ");
                ui.horizontal(|ui| {
                    let mut new = brush_material.clone();
                    ui.vertical(|ui| {
                        ui.radio_value(&mut new, BrushMaterial::Cell(AntSimCell::Path { pheromone_food: NonMaxU16::new(0), pheromone_home: NonMaxU16::new(0) }), "clear");
                        ui.radio_value(&mut new, BrushMaterial::Cell(AntSimCell::Blocker), "blocker");
                        ui.radio_value(&mut new, BrushMaterial::AntSpawn, "spawn ant");
                    });
                    ui.vertical(|ui| {
                        ui.radio_value(&mut new, BrushMaterial::Cell(AntSimCell::Food { amount: u16::MAX }), "food");
                        ui.radio_value(&mut new, BrushMaterial::Cell(AntSimCell::Home), "home");
                        ui.radio_value(&mut new, BrushMaterial::AntKill, "remove ant");
                    });
                    if &new != brush_material {
                        send_me!(AppEvents::SetBrushMaterial(new));
                    }
                });
            });

            /*ui.horizontal(|ui| {
                ui.label("Write something: ");
                ui.text_edit_singleline(&mut self.label).changed();
            });
            let mut value = &mut self.value;
            ui.add(egui::Slider::new(value, 0.0..=10.0).text("value"));
            if ui.button("Increment").clicked() {
                *value += 1.0;
            }*/
            if ui.button("Start").clicked() {
                send_me!(AppEvents::RequestLaunch);
            }
        });
    }
}

impl eframe::App for AppState {
    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_input(ctx);
        handle_events(self, ctx);
        // Examples of how to create different panels and windows.
        // Pick whichever suits you.
        // Tip: a good default choice is to just keep the `CentralPanel`.
        // For inspiration and more examples, go to https://emilk.github.io/egui


        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:
            egui::menu::bar(ui, |ui| {
                ui.menu_button(RichText::new("File").size(16.0), |ui| {
                    #[cfg(not(target_arch = "wasm32"))] // no File->Quit on web pages!
                    if ui.button("Quit").clicked() {
                        _frame.close();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("Load").clicked() {
                        self.send_me(AppEvents::RequestLoadGame)
                    }
                    if ui.button("Save").clicked() {
                        self.send_me(AppEvents::RequestSaveGame)
                    }
                });
            });
        });
        if let GameState::Edit(_) = self.game_state {
            self.edit_side_panel(ctx);
        }
        egui::panel::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(Align::Min).with_cross_align(Align::Max), |ui| {
                let text = if matches!(self.game_state, GameState::Edit { .. }) {
                    String::from("Edit")
                } else if self.game_speed.paused {
                    String::from("Paused")
                } else if self.game_speed.delay == Duration::ZERO {
                    String::from("Fastest")
                } else {
                    let mut str = format!("{:.2}s", self.game_speed.delay.as_secs_f64());
                    if str.ends_with(".0s") {
                        str.replace_range((str.len() - 3).., "s");
                    }
                    str
                };
                ui.label(RichText::new(text).size(20.));
            });
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.with_layout(egui::Layout::top_down(egui::Align::Center).with_cross_align(egui::Align::Center), |ui| {
                let max = ui.max_rect();
                let max_ratio = max.width() / max.height();
                let image_size = self.game_image.size_vec2();
                let image_ratio = image_size.x / image_size.y;
                let size = if image_ratio < max_ratio {
                    let height = max.height();
                    let width = height * image_ratio;
                    [width, height]
                } else {
                    let width = max.width();
                    let height = width / image_ratio;
                    [width, height]
                };
                let image = Image::new(self.game_image.id(), size).ui(ui).interact(Sense::click_and_drag());
                if image.dragged() {
                    let current = image.interact_pointer_pos().unwrap() - image.rect.min;
                    let starting = current - image.drag_delta();
                    let x_ratio = image_size.x / size[0];
                    let y_ratio = image_size.y / size[1];
                    let on_image_starting = [starting.x * x_ratio, starting.y * y_ratio];
                    let on_image_current = [current.x * x_ratio, current.y * y_ratio];
                    if ((0.0..image_size.x).contains(&on_image_starting[0]) && (0.0..image_size.y).contains(&on_image_starting[1]))
                        || (on_image_current[0] < image_size.x && on_image_current[1] < image_size.y) {
                        self.send_me(AppEvents::PaintStroke { from: on_image_starting, to: on_image_current })
                    }
                }
                if image.clicked() {
                    let current = image.interact_pointer_pos().unwrap() - image.rect.min;
                    let x_ratio = image_size.x / size[0];
                    let y_ratio = image_size.y / size[1];
                    let on_image_current = [current.x * x_ratio, current.y * y_ratio];
                    self.send_me(AppEvents::BoardClick(on_image_current))
                }
            });
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label("powered by ");
                    ui.hyperlink_to("egui", "https://github.com/emilk/egui");
                    ui.label(" and ");
                    ui.hyperlink_to(
                        "eframe",
                        "https://github.com/emilk/egui/tree/master/crates/eframe",
                    );
                    ui.label(".");
                    egui::warn_if_debug_build(ui);
                });
            });
        });

        let error_stack = &mut self.error_stack;
        if let Some(err) = error_stack.last().cloned() {
            egui::Window::new("Error")
                .default_size(ctx.used_size() * egui::Vec2::new(0.5, 0.5))
                .frame(Frame::popup(ctx.style().as_ref()).fill(Color32::LIGHT_RED))
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        egui::Label::new(RichText::new(err).color(Color32::BLACK).size(25.0)).wrap(true).ui(ui);
                        let dismiss = ui.button(RichText::new("Dismiss").size(25.0));
                        if dismiss.clicked() {
                            error_stack.pop();
                        }
                    });
                });
        }
    }

    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        //eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

fn default_ant_sim() -> AntSimulator<AntSimFrame> {
    let sim = AntSimFrame::new(300, 300).unwrap();

    AntSimulator {
        sim,
        ants: Vec::new(),
        seed: 42,
        config: AntSimConfig {
            distance_points: Box::new(POINTS_R1),
            food_haul_amount: 255,
            pheromone_decay_amount: 255,
            seed_step: 0,
            visual_range: AntVisualRangeBuffer::new(3),
        },
    }
}

pub static POINTS_R1: [(f64, f64); 8] = [
    (1.0, 0.0),
    (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (0.0, 1.0),
    (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (-1.0, 0.0),
    (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (-0.0, -1.0),
    (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
];

pub fn try_classify_points_radius_from(p: &[(f64, f64); 8]) -> Option<f64> {
    let mult_by = p[0].0;
    let all_approx_eq = POINTS_R1.iter().zip(p.iter()).all(|((expa, expb), (a, b))|{
        let amult = (a * mult_by);
        let a_valid = *expa - 0.05 < amult && amult < *expa + 0.05;
        let bmult = (b * mult_by);
        let b_valid = *expb - 0.05 < bmult && bmult < *expb + 0.05;
        a_valid && b_valid
    });
    all_approx_eq.then_some(mult_by)
}