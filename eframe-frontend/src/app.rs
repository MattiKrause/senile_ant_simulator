use eframe::epaint::color::hsv_from_rgb;
use eframe::epaint::textures::TextureFilter;
use egui::{Color32, ColorImage, DroppedFile, Image, ImageData, TextureHandle, Widget};
use egui::epaint::ImageDelta;
use egui_extras::RetainedImage;
use ant_sim::ant_sim::AntSimulator;
use ant_sim::ant_sim_frame::AntSim;
use rgba_adapter::SetRgb;
use ant_sim::ant_sim_frame_impl::AntSimVecImpl;

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
pub struct AppState {
    game_image: TextureHandle,
    // Example stuff:
    label: String,

    // this how you opt-out of serialization of a member
    value: f32,
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

    fn create_new(cc: &eframe::CreationContext<'_>) -> Self  {
        let colored_image = ColorImage::new([1, 1], Color32::from_rgba_unmultiplied(0, 0, 0, 0xFF));
        let texture = cc.egui_ctx.load_texture("ant_sim background", colored_image, TextureFilter::Nearest);
        AppState {
            game_image: texture,
            label: "lbl".to_string(),
            value: 42.0
        }
    }
}

impl eframe::App for AppState {
    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let Self { game_image, label, value } = self;
        {
            let files: &[DroppedFile] = &ctx.input().raw.dropped_files;
            if let Some(f) = files.first() {
                log::debug!("loaded image");
                if let Some(b) = &f.bytes {
                    let ant_sim =ant_sim_save::save_io::decode_save(&mut b.as_ref(), |sim| {
                        let width = sim.width.try_into().map_err(|_|())?;
                        let height = sim.width.try_into().map_err(|_|())?;
                        AntSimVecImpl::new(width, height).map_err(|_|())
                    });
                    if let Ok(res) = ant_sim {
                        game_image.set(sim_to_image(&res), TextureFilter::Nearest);
                    }
                }
            }
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
                ui.text_edit_singleline(label);
            });

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

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            egui::Image::new(game_image.id(), [300.0, 300.0]).ui(ui);
            ui.heading("eframe template");
            ui.hyperlink("https://github.com/emilk/eframe_template");
            ui.add(egui::github_link_file!(
                "https://github.com/emilk/eframe_template/blob/master/",
                "Source code."
            ));
            egui::warn_if_debug_build(ui);
        });

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
    impl <'a> SetRgb for ImageRgba<'a> {
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
        pixels: image_buf
    })
}