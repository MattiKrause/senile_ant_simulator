use std::fmt::Debug;
use std::mem::replace;
use std::ops::Mul;
use std::time::Duration;
use async_std::channel::{Receiver as ChannelReceiver, Sender as ChannelSender, Sender, TryRecvError};
use eframe::epaint::color::hsv_from_rgb;
use eframe::epaint::textures::TextureFilter;
use egui::{Color32, ColorImage, DroppedFile, Frame, Image, ImageData, Rect, RichText, TextFormat, TextureHandle, Widget, WidgetText};
use egui::style::Margin;
use egui::text::LayoutJob;
use egui_extras::RetainedImage;
use ant_sim::ant_sim::{AntSimConfig, AntSimulator, AntVisualRangeBuffer};
use ant_sim::ant_sim_frame::AntSim;
use rgba_adapter::SetRgb;
use ant_sim::ant_sim_frame_impl::AntSimVecImpl;
use crate::app_services::{load_file_service, Services, update_service};
use crate::load_file_service::{DroppedFileMessage, LoadFileMessages, LoadFileService};
use crate::service_handle::{SenderDiedError, ServiceHandle, TransService};
use crate::sim_update_service::SimUpdaterMessage;

type AntSimFrame = AntSimVecImpl;

pub enum AppEvents {
    ReplaceSim(Result<Box<AntSimulator<AntSimFrame>>, String>),
    NewStateImage(ImageData),
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
pub struct AppState {
    game_image: TextureHandle,
    mailbox: ChannelReceiver<AppEvents>,
    error_stack: Vec<String>,

    // Example stuff:
    label: String,

    // this how you opt-out of serialization of a member
    value: f32,
    services: Services,
}


impl AppState {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        log::debug!("started app");

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
        let colored_image = ColorImage::new([1, 1], Color32::from_rgba_unmultiplied(0, 0, 0, 0xFF));
        let texture = cc.egui_ctx.load_texture("ant_sim background", colored_image, TextureFilter::Nearest);
        let mailbox = async_std::channel::unbounded();
        let services = Services {
            load_file: load_file_service(mailbox.0.clone()),
            update: update_service(mailbox.0.clone(), Duration::from_millis(200), default_ant_sim()),
            mailbox_in: mailbox.0,
        };
        AppState {
            game_image: texture,
            mailbox: mailbox.1,
            error_stack: Vec::new(),
            label: "lbl".to_string(),
            value: 42.0,
            services,
        }
    }

    fn handle_dropped_file(&mut self, files: &[DroppedFile]) {
        if files.len() > 1 {
            self.error_stack.push(String::from("please drop only one file at once"));
            return;
        }
        let file = if let Some(file) = files.first() {
            file
        } else {
            return;
        };
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
                Err(err) => {
                    let err = format!("sender err");
                    log::error!(target: "LoadFileService", "{err}");
                    self.services.load_file = load_file_service(self.services.mailbox_in.clone());
                }
            }
        } else {
            log::warn!(target: "LoadFileService", "failed to handle file");
        }
    }
}

impl eframe::App for AppState {
    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_dropped_file(&ctx.input().raw.dropped_files);

        let mut event_query = self.mailbox.try_recv();
        while let Ok(event) = event_query {
            event_query = self.mailbox.try_recv();
            match event {
                AppEvents::ReplaceSim(ant_sim) => {
                    match ant_sim {
                        Ok(res) => {
                            if let Some(update) = replace(&mut self.services.update, None) {
                                if let Ok(service) = update.try_send(SimUpdaterMessage::NewSim(res)) {
                                    self.services.update = Some(service.0);
                                } else {
                                    panic!("services down!")
                                }
                            }
                        },
                        Err(err) => {
                            self.error_stack.push(format!("Failed to load save: {err}"));
                        }
                    }
                }
                AppEvents::NewStateImage(image) => {
                    self.game_image.set(image, TextureFilter::Nearest);
                    ctx.request_repaint();
                }
            }
        }
        if let Err(TryRecvError::Closed) = event_query {
            panic!("services down!");
        }

        // Examples of how to create different panels and windows.
        // Pick whichever suits you.
        // Tip: a good default choice is to just keep the `CentralPanel`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        #[cfg(not(target_arch = "wasm32"))] // no File->Quit on web pages!
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        _frame.close();
                    }
                });
            });
        });

        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.heading("Side Panel");

            ui.horizontal(|ui| {
                ui.label("Write something: ");
                ui.text_edit_singleline(&mut self.label);
            });
            let mut value = &mut self.value;
            ui.add(egui::Slider::new(value, 0.0..=10.0).text("value"));
            if ui.button("Increment").clicked() {
                *value += 1.0;
            }

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
                });
            });
        });

        egui::panel::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.with_layout(egui::Layout::top_down(egui::Align::Center).with_cross_align(egui::Align::Center), |ui| {
                egui::Image::new(self.game_image.id(), [300.0, 300.0]).ui(ui);
            });
            ui.heading("eframe template");
            ui.hyperlink("https://github.com/emilk/eframe_template");
            ui.add(egui::github_link_file!(
                "https://github.com/emilk/eframe_template/blob/master/",
                "Source code."
            ));
            egui::warn_if_debug_build(ui);
        });

        let mut error_stack = &mut self.error_stack;
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

        if false {
            egui::Window::new("Window").show(ctx, |ui| {
                ui.label("Windows can be moved by dragging them.");
                ui.label("They are automatically sized based on contents.");
                ui.label("You can turn on resizing and scrolling if you like.");
                ui.label("You would normally chose either panels OR windows.");
            });
        }
    }

    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        //eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

fn sim_to_image<A: AntSim>(sim: &AntSimulator<A>) -> ImageData {
    struct ImageRgba<'a>(&'a mut [Color32]);
    impl<'a> SetRgb for ImageRgba<'a> {
        #[inline(always)]
        fn len(&self) -> usize {
            self.0.len()
        }

        #[inline(always)]
        fn set_rgb(&mut self, index: usize, pix: [u8; 3]) {
            self.0[index] = Color32::from_rgb(pix[0], pix[1], pix[2]);
        }
    }
    let mut image_buf = vec![Color32::default(); sim.sim.width() * sim.sim.height()];
    rgba_adapter::draw_to_buf(sim, ImageRgba(&mut image_buf));
    ImageData::Color(ColorImage {
        size: [sim.sim.width(), sim.sim.height()],
        pixels: image_buf,
    })
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
            visual_range: AntVisualRangeBuffer::new(3)
        }
    }
}

static POINTS_R1: [(f64, f64); 8] = [
    (1.0, 0.0),
    (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (0.0, 1.0),
    (-std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    (-1.0, 0.0),
    (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
    (-0.0, -1.0),
    (std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2),
];
