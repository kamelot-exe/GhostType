use crate::config::Config;
use crate::db::SuggestionDb;
use crate::overlay::OverlayCmd;
use crate::telegram_import;
use crossbeam_channel::Sender;
use eframe::egui;
use std::sync::Arc;

pub struct GhostTypeApp {
    config: Config,
    db: Arc<SuggestionDb>,
    overlay_tx: Sender<OverlayCmd>,
    engine_tx: Sender<EngineCmd>,
    // UI state
    import_status: String,
    new_ignored_app: String,
    stats: DbStats,
}

#[derive(Debug, Clone)]
pub enum EngineCmd {
    Start,
    Stop,
    UpdateConfig(Config),
}

#[derive(Default, Debug)]
struct DbStats {
    unigrams: i64,
    bigrams: i64,
    trigrams: i64,
}

impl GhostTypeApp {
    pub fn new(
        config: Config,
        db: Arc<SuggestionDb>,
        overlay_tx: Sender<OverlayCmd>,
        engine_tx: Sender<EngineCmd>,
    ) -> Self {
        let stats = DbStats {
            unigrams: db.count_unigrams(),
            bigrams: db.count_bigrams(),
            trigrams: db.count_trigrams(),
        };
        Self {
            config,
            db,
            overlay_tx,
            engine_tx,
            import_status: String::new(),
            new_ignored_app: String::new(),
            stats,
        }
    }

    fn refresh_stats(&mut self) {
        self.stats.unigrams = self.db.count_unigrams();
        self.stats.bigrams = self.db.count_bigrams();
        self.stats.trigrams = self.db.count_trigrams();
    }

    fn send_config_update(&self) {
        self.config.save();
        let _ = self.engine_tx.send(EngineCmd::UpdateConfig(self.config.clone()));
        let (r, g, b) = self.config.parse_color();
        let _ = self.overlay_tx.send(OverlayCmd::UpdateConfig {
            color: (r, g, b),
            opacity: self.config.opacity,
            font_name: self.config.font.clone(),
            font_size: self.config.font_size,
        });
    }
}

impl eframe::App for GhostTypeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("GhostType Settings");
            ui.separator();

            // General
            egui::CollapsingHeader::new("General").default_open(true).show(ui, |ui| {
                let mut engine = self.config.engine_enabled;
                if ui.checkbox(&mut engine, "Engine enabled").changed() {
                    self.config.engine_enabled = engine;
                    if engine {
                        let _ = self.engine_tx.send(EngineCmd::Start);
                    } else {
                        let _ = self.engine_tx.send(EngineCmd::Stop);
                    }
                    self.send_config_update();
                }

                let mut overlay = self.config.overlay_enabled;
                if ui.checkbox(&mut overlay, "Overlay enabled").changed() {
                    self.config.overlay_enabled = overlay;
                    if !overlay {
                        let _ = self.overlay_tx.send(OverlayCmd::Hide);
                    }
                    self.send_config_update();
                }
            });

            // Suggestions
            egui::CollapsingHeader::new("Suggestions").default_open(true).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    egui::ComboBox::from_id_salt("mode_combo")
                        .selected_text(&self.config.mode)
                        .show_ui(ui, |ui| {
                            let mut changed = false;
                            changed |= ui.selectable_value(&mut self.config.mode, "word".into(), "Word").changed();
                            changed |= ui.selectable_value(&mut self.config.mode, "phrase".into(), "Phrase").changed();
                            changed |= ui.selectable_value(&mut self.config.mode, "hybrid".into(), "Hybrid").changed();
                            if changed {
                                self.send_config_update();
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Min prefix length:");
                    let mut pl = self.config.prefix_length as u32;
                    if ui.add(egui::DragValue::new(&mut pl).range(1..=5)).changed() {
                        self.config.prefix_length = pl as usize;
                        self.send_config_update();
                    }
                });
            });

            // Appearance
            egui::CollapsingHeader::new("Appearance").default_open(false).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    let mut color_str = self.config.color.clone();
                    if ui.text_edit_singleline(&mut color_str).changed() {
                        self.config.color = color_str;
                        self.send_config_update();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Opacity:");
                    if ui.add(egui::Slider::new(&mut self.config.opacity, 0.1..=1.0)).changed() {
                        self.send_config_update();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Font:");
                    let mut font = self.config.font.clone();
                    if ui.text_edit_singleline(&mut font).changed() {
                        self.config.font = font;
                        self.send_config_update();
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Font size:");
                    let mut fs = self.config.font_size;
                    if ui.add(egui::DragValue::new(&mut fs).range(8..=48)).changed() {
                        self.config.font_size = fs;
                        self.send_config_update();
                    }
                });
            });

            // Dataset
            egui::CollapsingHeader::new("Dataset").default_open(true).show(ui, |ui| {
                ui.label(format!("Unigrams: {}", self.stats.unigrams));
                ui.label(format!("Bigrams: {}", self.stats.bigrams));
                ui.label(format!("Trigrams: {}", self.stats.trigrams));

                ui.horizontal(|ui| {
                    if ui.button("Import Telegram JSON").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .pick_file()
                        {
                            let path_str = path.display().to_string();
                            match telegram_import::import_file(&self.db, &path_str) {
                                Ok(stats) => {
                                    self.import_status = format!(
                                        "Imported {} messages: {} unigrams, {} bigrams, {} trigrams",
                                        stats.messages, stats.unigrams, stats.bigrams, stats.trigrams
                                    );
                                    self.refresh_stats();
                                }
                                Err(e) => {
                                    self.import_status = format!("Import error: {e}");
                                }
                            }
                        }
                    }

                    if ui.button("Import Default Files").clicked() {
                        telegram_import::import_default_files(&self.db);
                        self.import_status = "Default files imported.".into();
                        self.refresh_stats();
                    }

                    if ui.button("Rebuild Database").clicked() {
                        if let Err(e) = self.db.clear_ngrams() {
                            self.import_status = format!("Clear error: {e}");
                        } else {
                            telegram_import::import_default_files(&self.db);
                            self.import_status = "Database rebuilt.".into();
                            self.refresh_stats();
                        }
                    }
                });

                if !self.import_status.is_empty() {
                    ui.label(&self.import_status);
                }
            });

            // Hotkeys
            egui::CollapsingHeader::new("Hotkeys").default_open(false).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Accept suggestion:");
                    let mut key = self.config.accept_key.clone();
                    if ui.text_edit_singleline(&mut key).changed() {
                        self.config.accept_key = key;
                        self.send_config_update();
                    }
                });
            });

            // Ignored Apps
            egui::CollapsingHeader::new("Ignored Apps").default_open(false).show(ui, |ui| {
                let mut to_remove = None;
                for (i, app) in self.config.ignored_apps.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(app);
                        if ui.button("x").clicked() {
                            to_remove = Some(i);
                        }
                    });
                }
                if let Some(idx) = to_remove {
                    self.config.ignored_apps.remove(idx);
                    self.send_config_update();
                }

                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.new_ignored_app);
                    if ui.button("Add").clicked() && !self.new_ignored_app.is_empty() {
                        self.config.ignored_apps.push(self.new_ignored_app.clone());
                        self.new_ignored_app.clear();
                        self.send_config_update();
                    }
                });
            });
        });
    }
}
