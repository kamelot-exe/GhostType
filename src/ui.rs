#![allow(clippy::too_many_arguments)]

use crate::config::{parse_hex_color, Config};
use crate::db::SuggestionDb;
use crate::overlay::OverlayCmd;
use crate::telegram_import;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, FontId, Frame, Margin, RichText, Rounding, Stroke, Vec2};
use std::sync::Arc;

// ─── color palette ───────────────────────────────────────────────────────────
const BG_APP: Color32 = Color32::from_rgb(10, 11, 20);
const BG_PANEL: Color32 = Color32::from_rgb(17, 18, 32);
const BG_CARD: Color32 = Color32::from_rgb(22, 24, 42);
const BG_WIDGET: Color32 = Color32::from_rgb(30, 32, 55);
const BG_WIDGET_HOV: Color32 = Color32::from_rgb(45, 48, 78);
const BG_WIDGET_ACT: Color32 = Color32::from_rgb(85, 70, 148);
const ACCENT: Color32 = Color32::from_rgb(120, 100, 200);
const ACCENT_DIM: Color32 = Color32::from_rgb(70, 58, 120);
const TEXT_MAIN: Color32 = Color32::from_rgb(220, 220, 235);
const TEXT_DIM: Color32 = Color32::from_rgb(130, 130, 155);
const TEXT_ACCENT: Color32 = Color32::from_rgb(160, 140, 230);
const GREEN: Color32 = Color32::from_rgb(80, 200, 120);
const RED: Color32 = Color32::from_rgb(200, 80, 80);
const BORDER: Color32 = Color32::from_rgb(45, 48, 75);

// ─── public types ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum EngineCmd {
    Start,
    Stop,
    UpdateConfig(Config),
    RefreshCache,
    StartRebind,
}

// ─── app state ────────────────────────────────────────────────────────────────
pub struct GhostTypeApp {
    config: Config,
    db: Arc<SuggestionDb>,
    overlay_tx: Sender<OverlayCmd>,
    engine_tx: Sender<EngineCmd>,
    rebind_rx: Receiver<String>,

    // UI transient state
    import_status: String,
    new_ignored_app: String,
    stats: DbStats,
    rebind_pending: bool,
    initialized: bool,
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
        rebind_rx: Receiver<String>,
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
            rebind_rx,
            import_status: String::new(),
            new_ignored_app: String::new(),
            stats,
            rebind_pending: false,
            initialized: false,
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
        let (br, bg, bb) = self.config.parse_bg_color();
        let _ = self.overlay_tx.send(OverlayCmd::UpdateConfig {
            color: (r, g, b),
            bg_color: (br, bg, bb),
            opacity: self.config.opacity,
            font_name: self.config.font.clone(),
            font_size: self.config.font_size,
            corner_radius: self.config.corner_radius,
            padding: self.config.padding,
            position_mode: self.config.position_mode.clone(),
        });
    }
}

// ─── egui App impl ────────────────────────────────────────────────────────────
impl eframe::App for GhostTypeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // One-time style configuration
        if !self.initialized {
            self.initialized = true;
            configure_visuals(ctx);
            configure_style(ctx);
        }

        // Poll rebind result from engine
        if self.rebind_pending {
            if let Ok(key_str) = self.rebind_rx.try_recv() {
                self.config.accept_key = key_str;
                self.rebind_pending = false;
                self.send_config_update();
            }
            ctx.request_repaint();
        }

        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG_APP))
            .show(ctx, |ui| {
                // Header
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("GhostType")
                            .size(22.0)
                            .strong()
                            .color(TEXT_ACCENT),
                    );
                    ui.label(RichText::new("Settings").size(22.0).color(TEXT_DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(12.0);
                        let running = self.config.engine_enabled;
                        let dot_color = if running { GREEN } else { RED };
                        let status_text = if running { "Running" } else { "Stopped" };
                        ui.label(RichText::new(status_text).size(11.0).color(dot_color));
                        ui.add_space(4.0);
                        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);
                        ui.add_space(4.0);
                    });
                });
                ui.add_space(4.0);
                separator(ui);
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.set_width(ui.available_width());

                        self.section_general(ui);
                        self.section_suggestions(ui);
                        self.section_appearance(ui);
                        self.section_hotkeys(ui);
                        self.section_dataset(ui);
                        self.section_ignored_apps(ui);
                        self.section_preview(ui);
                        ui.add_space(12.0);
                    });
            });
    }
}

// ─── sections ─────────────────────────────────────────────────────────────────
impl GhostTypeApp {
    fn section_general(&mut self, ui: &mut egui::Ui) {
        card(ui, "General", true, |ui| {
            ui.horizontal(|ui| {
                // Engine toggle
                let eng = self.config.engine_enabled;
                let btn_text = if eng { "Stop Engine" } else { "Start Engine" };
                let btn_color = if eng { RED } else { GREEN };
                if ui
                    .add(egui::Button::new(RichText::new(btn_text).color(btn_color).size(12.0))
                        .fill(BG_WIDGET)
                        .stroke(Stroke::new(1.0, btn_color)))
                    .clicked()
                {
                    self.config.engine_enabled = !eng;
                    if self.config.engine_enabled {
                        let _ = self.engine_tx.send(EngineCmd::Start);
                    } else {
                        let _ = self.engine_tx.send(EngineCmd::Stop);
                    }
                    self.send_config_update();
                }

                ui.add_space(8.0);

                // Overlay toggle
                let ov = self.config.overlay_enabled;
                let ov_text = if ov { "Hide Overlay" } else { "Show Overlay" };
                if ui
                    .add(egui::Button::new(RichText::new(ov_text).size(12.0))
                        .fill(BG_WIDGET)
                        .stroke(Stroke::new(1.0, BORDER)))
                    .clicked()
                {
                    self.config.overlay_enabled = !ov;
                    if !self.config.overlay_enabled {
                        let _ = self.overlay_tx.send(OverlayCmd::Hide);
                    }
                    self.send_config_update();
                }
            });

            ui.add_space(4.0);

            row_label_value(ui, "Engine", if self.config.engine_enabled { "Active" } else { "Stopped" },
                if self.config.engine_enabled { GREEN } else { TEXT_DIM });
            row_label_value(ui, "Overlay", if self.config.overlay_enabled { "Enabled" } else { "Disabled" },
                if self.config.overlay_enabled { GREEN } else { TEXT_DIM });
        });
    }

    fn section_suggestions(&mut self, ui: &mut egui::Ui) {
        card(ui, "Suggestions", true, |ui| {
            let mut changed = false;

            // Mode
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mode").color(TEXT_DIM).size(12.0));
                ui.add_space(4.0);
                for mode in &["word", "phrase", "hybrid"] {
                    let selected = &self.config.mode == mode;
                    let btn = egui::Button::new(RichText::new(*mode).size(12.0))
                        .fill(if selected { ACCENT_DIM } else { BG_WIDGET })
                        .stroke(Stroke::new(1.0, if selected { ACCENT } else { BORDER }));
                    if ui.add(btn).clicked() && !selected {
                        self.config.mode = mode.to_string();
                        changed = true;
                    }
                }
            });

            ui.add_space(6.0);

            // Min prefix length
            ui.horizontal(|ui| {
                ui.label(RichText::new("Min prefix length").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                let mut pl = self.config.prefix_length as u32;
                if ui.add(
                    egui::Slider::new(&mut pl, 1..=6)
                        .text("")
                ).changed() {
                    self.config.prefix_length = pl as usize;
                    changed = true;
                }
                ui.label(RichText::new(format!("{}", self.config.prefix_length)).size(12.0).color(TEXT_ACCENT));
            });

            // Debounce
            ui.horizontal(|ui| {
                ui.label(RichText::new("Debounce (ms)").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                let mut d = self.config.debounce_ms as u32;
                if ui.add(egui::Slider::new(&mut d, 10..=200).text("")).changed() {
                    self.config.debounce_ms = d as u64;
                    changed = true;
                }
                ui.label(RichText::new(format!("{}", self.config.debounce_ms)).size(12.0).color(TEXT_ACCENT));
            });

            if changed {
                self.send_config_update();
            }
        });
    }

    fn section_appearance(&mut self, ui: &mut egui::Ui) {
        card(ui, "Appearance", false, |ui| {
            let mut changed = false;

            // Text color picker
            ui.horizontal(|ui| {
                ui.label(RichText::new("Text color").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                let mut rgb = hex_to_srgb(&self.config.color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut rgb).changed() {
                    self.config.color = srgb_to_hex(rgb);
                    changed = true;
                }
                ui.label(RichText::new(&self.config.color).monospace().size(11.0).color(TEXT_DIM));
            });

            // Background color picker
            ui.horizontal(|ui| {
                ui.label(RichText::new("BG color").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                let mut rgb = hex_to_srgb(&self.config.bg_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut rgb).changed() {
                    self.config.bg_color = srgb_to_hex(rgb);
                    changed = true;
                }
                ui.label(RichText::new(&self.config.bg_color).monospace().size(11.0).color(TEXT_DIM));
            });

            // Opacity
            ui.horizontal(|ui| {
                ui.label(RichText::new("Opacity").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                if ui.add(egui::Slider::new(&mut self.config.opacity, 0.1..=1.0).text("")).changed() {
                    changed = true;
                }
                ui.label(RichText::new(format!("{:.0}%", self.config.opacity * 100.0)).size(12.0).color(TEXT_ACCENT));
            });

            // Font family
            ui.horizontal(|ui| {
                ui.label(RichText::new("Font").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                let mut font = self.config.font.clone();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut font)
                        .desired_width(120.0)
                        .font(FontId::proportional(12.0))
                );
                if resp.changed() {
                    self.config.font = font;
                    changed = true;
                }
            });

            // Font size
            ui.horizontal(|ui| {
                ui.label(RichText::new("Font size").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                let mut fs = self.config.font_size;
                if ui.add(egui::Slider::new(&mut fs, 8..=36).text("")).changed() {
                    self.config.font_size = fs;
                    changed = true;
                }
                ui.label(RichText::new(format!("{}px", self.config.font_size)).size(12.0).color(TEXT_ACCENT));
            });

            // Corner radius
            ui.horizontal(|ui| {
                ui.label(RichText::new("Corner radius").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                let mut cr = self.config.corner_radius;
                if ui.add(egui::Slider::new(&mut cr, 0..=20).text("")).changed() {
                    self.config.corner_radius = cr;
                    changed = true;
                }
                ui.label(RichText::new(format!("{}", self.config.corner_radius)).size(12.0).color(TEXT_ACCENT));
            });

            // Padding
            ui.horizontal(|ui| {
                ui.label(RichText::new("Padding").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);
                let mut p = self.config.padding;
                if ui.add(egui::Slider::new(&mut p, 2..=20).text("")).changed() {
                    self.config.padding = p;
                    changed = true;
                }
                ui.label(RichText::new(format!("{}px", self.config.padding)).size(12.0).color(TEXT_ACCENT));
            });

            // Position mode
            ui.horizontal(|ui| {
                ui.label(RichText::new("Position").color(TEXT_DIM).size(12.0));
                ui.add_space(4.0);
                for (id, label) in &[
                    ("above_caret", "Above caret"),
                    ("below_caret", "Below caret"),
                    ("near_mouse", "Near mouse"),
                ] {
                    let selected = &self.config.position_mode == id;
                    let btn = egui::Button::new(RichText::new(*label).size(11.0))
                        .fill(if selected { ACCENT_DIM } else { BG_WIDGET })
                        .stroke(Stroke::new(1.0, if selected { ACCENT } else { BORDER }));
                    if ui.add(btn).clicked() && !selected {
                        self.config.position_mode = id.to_string();
                        changed = true;
                    }
                }
            });

            if changed {
                self.send_config_update();
            }
        });
    }

    fn section_hotkeys(&mut self, ui: &mut egui::Ui) {
        card(ui, "Hotkeys", true, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Accept suggestion").color(TEXT_DIM).size(12.0));
                ui.add_space(8.0);

                if self.rebind_pending {
                    // Pulsing "waiting" indicator
                    key_badge_colored(ui, "Press any key...", ACCENT);
                    ui.add_space(8.0);
                    if ui.add(
                        egui::Button::new(RichText::new("Cancel").size(11.0))
                            .fill(BG_WIDGET)
                            .stroke(Stroke::new(1.0, BORDER))
                    ).clicked() {
                        self.rebind_pending = false;
                    }
                } else {
                    key_badge(ui, &self.config.accept_key);
                    ui.add_space(8.0);
                    if ui.add(
                        egui::Button::new(RichText::new("Rebind").size(11.0))
                            .fill(ACCENT_DIM)
                            .stroke(Stroke::new(1.0, ACCENT))
                    ).clicked() {
                        self.rebind_pending = true;
                        let _ = self.engine_tx.send(EngineCmd::StartRebind);
                    }
                    ui.add_space(4.0);
                    if ui.add(
                        egui::Button::new(RichText::new("Reset").size(11.0))
                            .fill(BG_WIDGET)
                            .stroke(Stroke::new(1.0, BORDER))
                    ).clicked() {
                        self.config.accept_key = "Tab".into();
                        self.send_config_update();
                    }
                }
            });

            ui.add_space(4.0);
            ui.label(
                RichText::new("Click Rebind, then press any key or key combination (e.g. Tab, Ctrl+Space, CapsLock, Alt+X)")
                    .size(10.0)
                    .color(TEXT_DIM)
            );
        });
    }

    fn section_dataset(&mut self, ui: &mut egui::Ui) {
        card(ui, "Dataset", true, |ui| {
            // Stats row
            ui.horizontal(|ui| {
                stat_chip(ui, "Unigrams", self.stats.unigrams);
                ui.add_space(6.0);
                stat_chip(ui, "Bigrams", self.stats.bigrams);
                ui.add_space(6.0);
                stat_chip(ui, "Trigrams", self.stats.trigrams);
            });

            ui.add_space(8.0);

            // Import buttons
            ui.horizontal_wrapped(|ui| {
                if ui.add(
                    egui::Button::new(RichText::new("Import Telegram JSON").size(12.0))
                        .fill(BG_WIDGET)
                        .stroke(Stroke::new(1.0, ACCENT))
                ).clicked() {
                    if let Some(paths) = rfd::FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .pick_files()
                    {
                        let mut total_msgs = 0usize;
                        let mut total_uni = 0usize;
                        let mut total_bi = 0usize;
                        let mut total_tri = 0usize;
                        let mut errors: Vec<String> = Vec::new();

                        for path in &paths {
                            let path_str = path.display().to_string();
                            match telegram_import::import_file(&self.db, &path_str) {
                                Ok(stats) => {
                                    total_msgs += stats.messages;
                                    total_uni += stats.unigrams;
                                    total_bi += stats.bigrams;
                                    total_tri += stats.trigrams;
                                }
                                Err(e) => {
                                    errors.push(format!("{}: {e}", path.display()));
                                }
                            }
                        }

                        if errors.is_empty() {
                            self.import_status = format!(
                                "Imported {} file(s) — {} msgs, {} unigrams, {} bigrams, {} trigrams",
                                paths.len(), total_msgs, total_uni, total_bi, total_tri
                            );
                        } else {
                            self.import_status = format!(
                                "{} ok / {} failed. {}",
                                paths.len() - errors.len(), errors.len(), errors.join("; ")
                            );
                        }
                        self.refresh_stats();
                        let _ = self.engine_tx.send(EngineCmd::RefreshCache);
                    }
                }

                ui.add_space(4.0);

                if ui.add(
                    egui::Button::new(RichText::new("Import Default Files").size(12.0))
                        .fill(BG_WIDGET)
                        .stroke(Stroke::new(1.0, BORDER))
                ).clicked() {
                    telegram_import::import_default_files(&self.db);
                    self.import_status = "Default files imported.".into();
                    self.refresh_stats();
                    let _ = self.engine_tx.send(EngineCmd::RefreshCache);
                }

                ui.add_space(4.0);

                if ui.add(
                    egui::Button::new(RichText::new("Rebuild Database").size(12.0))
                        .fill(BG_WIDGET)
                        .stroke(Stroke::new(1.0, Stroke::new(1.0, RED).color))
                ).clicked() {
                    match self.db.clear_ngrams() {
                        Err(e) => self.import_status = format!("Clear error: {e}"),
                        Ok(()) => {
                            telegram_import::import_default_files(&self.db);
                            self.import_status = "Database rebuilt from default files.".into();
                            self.refresh_stats();
                            let _ = self.engine_tx.send(EngineCmd::RefreshCache);
                        }
                    }
                }

                ui.add_space(4.0);

                if ui.add(
                    egui::Button::new(RichText::new("Reload Cache").size(12.0))
                        .fill(BG_WIDGET)
                        .stroke(Stroke::new(1.0, BORDER))
                ).clicked() {
                    let _ = self.engine_tx.send(EngineCmd::RefreshCache);
                    self.import_status = "Cache reload requested.".into();
                }
            });

            if !self.import_status.is_empty() {
                ui.add_space(6.0);
                Frame::none()
                    .fill(BG_WIDGET)
                    .rounding(Rounding::same(4.0))
                    .inner_margin(Margin::same(6.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&self.import_status)
                                .size(11.0)
                                .color(TEXT_DIM)
                        );
                    });
            }
        });
    }

    fn section_ignored_apps(&mut self, ui: &mut egui::Ui) {
        card(ui, "Ignored Apps", false, |ui| {
            ui.label(
                RichText::new("GhostType will be silent in these processes:")
                    .size(11.0)
                    .color(TEXT_DIM)
            );
            ui.add_space(6.0);

            let mut to_remove: Option<usize> = None;

            if self.config.ignored_apps.is_empty() {
                ui.label(RichText::new("No apps ignored.").size(12.0).color(TEXT_DIM));
            } else {
                for (i, app) in self.config.ignored_apps.iter().enumerate() {
                    ui.horizontal(|ui| {
                        Frame::none()
                            .fill(BG_WIDGET)
                            .rounding(Rounding::same(4.0))
                            .inner_margin(Margin::symmetric(8.0, 3.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(app).size(12.0).monospace().color(TEXT_MAIN));
                            });
                        if ui.add(
                            egui::Button::new(RichText::new("✕").size(11.0).color(RED))
                                .fill(egui::Color32::TRANSPARENT)
                                .frame(false)
                        ).clicked() {
                            to_remove = Some(i);
                        }
                    });
                }
            }

            if let Some(idx) = to_remove {
                self.config.ignored_apps.remove(idx);
                self.send_config_update();
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.new_ignored_app)
                        .hint_text("e.g. chrome.exe")
                        .desired_width(140.0)
                        .font(FontId::proportional(12.0))
                );
                if (ui.add(
                    egui::Button::new(RichText::new("Add").size(12.0))
                        .fill(ACCENT_DIM)
                        .stroke(Stroke::new(1.0, ACCENT))
                ).clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                    && !self.new_ignored_app.is_empty()
                {
                    self.config.ignored_apps.push(self.new_ignored_app.trim().to_lowercase());
                    self.new_ignored_app.clear();
                    self.send_config_update();
                }
            });
        });
    }

    fn section_preview(&mut self, ui: &mut egui::Ui) {
        card(ui, "Preview", false, |ui| {
            ui.label(
                RichText::new("Live preview of suggestion block based on current Appearance settings:")
                    .size(11.0)
                    .color(TEXT_DIM)
            );
            ui.add_space(8.0);

            let avail_w = ui.available_width() - 4.0;
            let preview_h = 64.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, preview_h), egui::Sense::hover());

            let painter = ui.painter_at(rect);

            // Draw simulated input area
            painter.rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(18, 20, 35));
            painter.rect_stroke(rect, Rounding::same(4.0), Stroke::new(1.0, BORDER));

            // Simulated typed text
            let typed = "Hello wo";
            let text_y = rect.min.y + rect.height() * 0.58;
            let text_x = rect.min.x + 14.0;
            painter.text(
                egui::Pos2::new(text_x, text_y),
                egui::Align2::LEFT_TOP,
                typed,
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            let approx_text_w = typed.len() as f32 * 7.1;

            // Cursor
            painter.line_segment(
                [
                    egui::Pos2::new(text_x + approx_text_w, text_y),
                    egui::Pos2::new(text_x + approx_text_w, text_y + 14.0),
                ],
                Stroke::new(1.5, Color32::WHITE),
            );

            // Suggestion bubble
            let suggestion = "rld";
            let (r, g, b) = self.config.parse_color();
            let text_color = Color32::from_rgb(r, g, b);
            let (br, bg_c, bb) = self.config.parse_bg_color();
            let bg_alpha = (self.config.opacity * 255.0) as u8;
            let bg_color = Color32::from_rgba_unmultiplied(br, bg_c, bb, bg_alpha);

            let fsize = 11.5_f32;
            let pad = (self.config.padding as f32 * 0.7).max(3.0);
            let corner_r = self.config.corner_radius as f32;

            let bubble_w = suggestion.len() as f32 * 6.8 + pad * 2.0;
            let bubble_h = fsize + pad * 2.0;
            let bubble_x = text_x + approx_text_w;
            let bubble_y = text_y - bubble_h - 4.0;

            let bubble_rect = egui::Rect::from_min_size(
                egui::Pos2::new(bubble_x, bubble_y),
                Vec2::new(bubble_w, bubble_h),
            );

            painter.rect_filled(bubble_rect, Rounding::same(corner_r), bg_color);
            painter.text(
                bubble_rect.min + Vec2::splat(pad),
                egui::Align2::LEFT_TOP,
                suggestion,
                FontId::proportional(fsize),
                text_color,
            );

            // Label
            ui.add_space(4.0);
            ui.label(
                RichText::new("The grey box simulates the input field; the bubble shows your current appearance settings.")
                    .size(10.0)
                    .color(TEXT_DIM)
            );
        });
    }
}

// ─── visual helpers ───────────────────────────────────────────────────────────

fn configure_visuals(ctx: &egui::Context) {
    let mut vis = egui::Visuals::dark();
    vis.override_text_color = Some(TEXT_MAIN);
    vis.panel_fill = BG_PANEL;
    vis.window_fill = BG_PANEL;
    vis.faint_bg_color = BG_CARD;
    vis.extreme_bg_color = Color32::from_rgb(8, 9, 18);
    vis.code_bg_color = BG_WIDGET;

    vis.widgets.noninteractive.bg_fill = BG_CARD;
    vis.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    vis.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);

    vis.widgets.inactive.bg_fill = BG_WIDGET;
    vis.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_MAIN);
    vis.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);

    vis.widgets.hovered.bg_fill = BG_WIDGET_HOV;
    vis.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_MAIN);
    vis.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);

    vis.widgets.active.bg_fill = BG_WIDGET_ACT;
    vis.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    vis.selection.bg_fill = ACCENT_DIM;
    vis.selection.stroke = Stroke::new(1.0, ACCENT);

    vis.window_rounding = Rounding::same(8.0);
    vis.window_stroke = Stroke::new(1.0, BORDER);

    ctx.set_visuals(vis);
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 5.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.indent = 18.0;
    style.spacing.slider_width = 90.0;
    ctx.set_style(style);
}

fn separator(ui: &mut egui::Ui) {
    let avail_w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::ZERO, BORDER);
}

fn card(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    contents: impl FnOnce(&mut egui::Ui),
) {
    Frame::none()
        .fill(BG_CARD)
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(12.0))
        .outer_margin(Margin { left: 8.0, right: 8.0, bottom: 8.0, top: 0.0 })
        .stroke(Stroke::new(1.0, BORDER))
        .show(ui, |ui| {
            egui::CollapsingHeader::new(
                RichText::new(title).size(13.0).strong().color(TEXT_ACCENT),
            )
            .default_open(default_open)
            .show(ui, |ui| {
                ui.add_space(4.0);
                contents(ui);
            });
        });
}

fn key_badge(ui: &mut egui::Ui, label: &str) {
    Frame::none()
        .fill(BG_WIDGET)
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::symmetric(10.0, 4.0))
        .stroke(Stroke::new(1.0, BORDER))
        .show(ui, |ui| {
            ui.label(RichText::new(label).monospace().size(12.0).color(TEXT_MAIN));
        });
}

fn key_badge_colored(ui: &mut egui::Ui, label: &str, color: Color32) {
    Frame::none()
        .fill(ACCENT_DIM)
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::symmetric(10.0, 4.0))
        .stroke(Stroke::new(1.0, color))
        .show(ui, |ui| {
            ui.label(RichText::new(label).monospace().size(12.0).color(color));
        });
}

fn stat_chip(ui: &mut egui::Ui, label: &str, value: i64) {
    Frame::none()
        .fill(BG_WIDGET)
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(10.0, 5.0))
        .stroke(Stroke::new(1.0, BORDER))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(format_number(value)).size(14.0).strong().color(TEXT_ACCENT));
                ui.label(RichText::new(label).size(10.0).color(TEXT_DIM));
            });
        });
}

fn row_label_value(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(TEXT_DIM));
        ui.add_space(4.0);
        ui.label(RichText::new(value).size(12.0).color(color));
    });
}

fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.0}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn hex_to_srgb(hex: &str) -> [f32; 3] {
    let (r, g, b) = parse_hex_color(hex);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn srgb_to_hex(rgb: [f32; 3]) -> String {
    let r = (rgb[0].clamp(0.0, 1.0) * 255.0) as u8;
    let g = (rgb[1].clamp(0.0, 1.0) * 255.0) as u8;
    let b = (rgb[2].clamp(0.0, 1.0) * 255.0) as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}
