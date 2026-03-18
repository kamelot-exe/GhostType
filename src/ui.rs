#![allow(clippy::too_many_arguments)]

use crate::config::{parse_hex_color, Config};
use crate::db::SuggestionDb;
use crate::overlay::OverlayCmd;
use crate::telegram_import;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Color32, FontId, Frame, Id, Margin, Pos2, Rect, RichText, Rounding, Stroke, Vec2};
use std::sync::Arc;

// ── colour palette ─────────────────────────────────────────────────────────
const C_BG: Color32      = Color32::from_rgb(9,  10, 18);
const C_SIDE: Color32    = Color32::from_rgb(11, 12, 22);
const C_CARD: Color32    = Color32::from_rgb(15, 16, 28);
const C_INPUT: Color32   = Color32::from_rgb(20, 22, 36);
const C_BORDER: Color32  = Color32::from_rgb(32, 34, 56);
const C_ACCENT: Color32  = Color32::from_rgb(108, 88, 220);
const C_ADIM: Color32    = Color32::from_rgb(38,  32, 80);
const C_AHOV: Color32    = Color32::from_rgb(130, 110, 245);
const C_TEXT: Color32    = Color32::from_rgb(218, 218, 236);
const C_DIM: Color32     = Color32::from_rgb(100, 102, 132);
const C_TACC: Color32    = Color32::from_rgb(162, 148, 255);
const C_GREEN: Color32   = Color32::from_rgb(72, 200, 142);
const C_RED: Color32     = Color32::from_rgb(225,  85,  85);
#[allow(dead_code)]
const C_ORANGE: Color32  = Color32::from_rgb(253, 180,  84);

// ── tabs ───────────────────────────────────────────────────────────────────
#[derive(PartialEq, Clone, Copy, Default, Debug)]
enum Tab { #[default] General, Suggestions, Appearance, Hotkeys, Dataset, Ignored, Preview }

const TABS: &[(Tab, &str)] = &[
    (Tab::General,     "General"),
    (Tab::Suggestions, "Suggestions"),
    (Tab::Appearance,  "Appearance"),
    (Tab::Hotkeys,     "Hotkeys"),
    (Tab::Dataset,     "Dataset"),
    (Tab::Ignored,     "Ignored Apps"),
    (Tab::Preview,     "Preview"),
];

// ── hotkey groups ──────────────────────────────────────────────────────────
const HK_GROUPS: &[(&str, &[&str])] = &[
    ("Common", &[
        "Tab", "Right", "Down", "Insert", "CapsLock", "Escape", "PageDown", "End",
    ]),
    ("F-Keys", &[
        "F1","F2","F3","F4","F5","F6","F7","F8","F9","F10","F11","F12",
    ]),
    ("Ctrl +", &[
        "Ctrl+Space","Ctrl+Enter","Ctrl+Tab","Ctrl+Right","Ctrl+;",
        "Ctrl+`","Ctrl+.","Ctrl+\\","Ctrl+Z","Ctrl+X",
    ]),
    ("Alt +", &[
        "Alt+Space","Alt+Z","Alt+X","Alt+`","Alt+;","Alt+Enter","Alt+Right",
    ]),
    ("Shift +", &[
        "Shift+Tab","Shift+Space","Shift+Enter","Shift+Right","Shift+Down",
    ]),
    ("Solo mod", &[
        "RCtrl","LCtrl","RAlt","LAlt","RShift","LShift",
    ]),
];

// ── public types ───────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum EngineCmd {
    Start, Stop,
    UpdateConfig(Config),
    RefreshCache,
    StartRebind,
}

// ── app state ──────────────────────────────────────────────────────────────
pub struct GhostTypeApp {
    config: Config,
    db: Arc<SuggestionDb>,
    overlay_tx: Sender<OverlayCmd>,
    engine_tx: Sender<EngineCmd>,
    rebind_rx: Receiver<String>,

    active_tab: Tab,
    import_status: String,
    new_ignored_app: String,
    stats: DbStats,
    rebind_pending: bool,
    initialized: bool,
}

#[derive(Default, Debug)]
struct DbStats { unigrams: i64, bigrams: i64, trigrams: i64 }

impl GhostTypeApp {
    pub fn new(
        config: Config, db: Arc<SuggestionDb>,
        overlay_tx: Sender<OverlayCmd>, engine_tx: Sender<EngineCmd>,
        rebind_rx: Receiver<String>,
    ) -> Self {
        let stats = DbStats {
            unigrams: db.count_unigrams(),
            bigrams:  db.count_bigrams(),
            trigrams: db.count_trigrams(),
        };
        Self {
            config, db, overlay_tx, engine_tx, rebind_rx,
            active_tab: Tab::General,
            import_status: String::new(),
            new_ignored_app: String::new(),
            stats,
            rebind_pending: false,
            initialized: false,
        }
    }

    fn refresh_stats(&mut self) {
        self.stats.unigrams = self.db.count_unigrams();
        self.stats.bigrams  = self.db.count_bigrams();
        self.stats.trigrams = self.db.count_trigrams();
    }

    fn push_config(&self) {
        self.config.save();
        let _ = self.engine_tx.send(EngineCmd::UpdateConfig(self.config.clone()));
        let (r, g, b)       = self.config.parse_color();
        let (br, bg_c, bb)  = self.config.parse_bg_color();
        let _ = self.overlay_tx.send(OverlayCmd::UpdateConfig {
            color: (r, g, b), bg_color: (br, bg_c, bb),
            opacity: self.config.opacity,
            font_name: self.config.font.clone(),
            font_size: self.config.font_size,
            corner_radius: self.config.corner_radius,
            padding: self.config.padding,
            position_mode: self.config.position_mode.clone(),
        });
    }
}

// ── eframe::App ────────────────────────────────────────────────────────────
impl eframe::App for GhostTypeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.initialized {
            self.initialized = true;
            setup_visuals(ctx);
        }

        if self.rebind_pending {
            if let Ok(key) = self.rebind_rx.try_recv() {
                self.config.accept_key = key;
                self.rebind_pending = false;
                self.push_config();
            }
            ctx.request_repaint();
        }

        // ── Sidebar ────────────────────────────────────────────────────────
        egui::SidePanel::left("nav")
            .exact_width(152.0)
            .resizable(false)
            .frame(Frame::none().fill(C_SIDE))
            .show(ctx, |ui| { self.draw_sidebar(ui, ctx); });

        // ── Content ────────────────────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(Frame::none().fill(C_BG))
            .show(ctx, |ui| { self.draw_content(ui, ctx); });
    }
}

// ── sidebar ────────────────────────────────────────────────────────────────
impl GhostTypeApp {
    fn draw_sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Header
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(RichText::new("Ghost").size(20.0).strong().color(C_TACC));
            ui.label(RichText::new("Type").size(20.0).strong().color(C_TEXT));
        });
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(RichText::new("v0.2  ·  Rust").size(10.0).color(C_DIM));
        });
        ui.add_space(14.0);
        hdiv(ui);
        ui.add_space(6.0);

        // Tabs
        for &(tab, label) in TABS {
            if self.nav_item(ui, ctx, tab, label) {
                self.active_tab = tab;
            }
        }

        // Bottom status strip
        let avail = ui.available_height();
        ui.add_space((avail - 44.0).max(0.0));
        hdiv(ui);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            let running = self.config.engine_enabled;
            let col = if running { C_GREEN } else { C_RED };
            // Animated glow dot
            let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
            let anim = ctx.animate_value_with_time(
                Id::new("dot_glow"), if running { 1.0f32 } else { 0.0f32 }, 0.4,
            );
            ui.painter().circle_filled(r.center(), 3.0 + anim * 1.0,
                Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (anim * 60.0) as u8 + 40));
            ui.painter().circle_filled(r.center(), 3.5, col);
            ui.add_space(6.0);
            ui.label(RichText::new(if running { "Running" } else { "Stopped" }).size(11.0).color(col));
        });
        ui.add_space(4.0);
    }

    fn nav_item(&self, ui: &mut egui::Ui, ctx: &egui::Context, tab: Tab, label: &str) -> bool {
        let h = 38.0;
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), egui::Sense::click());
        let active = self.active_tab == tab;
        let anim = ctx.animate_value_with_time(
            resp.id.with("nav"), if active { 1.0f32 } else { 0.0f32 }, 0.18,
        );

        if ui.is_rect_visible(rect) {
            let p = ui.painter();
            // Background fill
            let bg_a = if resp.hovered() && !active { 14 } else { 0 };
            let bg = Color32::from_rgba_unmultiplied(108, 88, 220, (anim * 22.0) as u8 + bg_a);
            p.rect_filled(rect, Rounding::ZERO, bg);
            // Left accent bar
            if anim > 0.01 {
                let bh = h * 0.55;
                let bar = Rect::from_min_size(
                    rect.min + Vec2::new(0.0, (h - bh) / 2.0), Vec2::new(3.0, bh),
                );
                p.rect_filled(bar, Rounding::same(2.0),
                    Color32::from_rgba_unmultiplied(108, 88, 220, (anim * 255.0) as u8));
            }
            // Label
            let text_col = lerp_col(C_DIM, C_TACC, anim);
            let text_col = if resp.hovered() && !active { lerp_col(text_col, C_TEXT, 0.6) } else { text_col };
            p.text(
                rect.min + Vec2::new(22.0, (h - 13.0) / 2.0),
                egui::Align2::LEFT_TOP, label,
                FontId::proportional(13.0), text_col,
            );
        }
        resp.clicked()
    }
}

// ── content dispatch ───────────────────────────────────────────────────────
impl GhostTypeApp {
    fn draw_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let title = TABS.iter().find(|&&(t, _)| t == self.active_tab)
            .map(|&(_, l)| l).unwrap_or("");

        // Top bar
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.label(RichText::new(title).size(18.0).strong().color(C_TEXT));
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            let avail = ui.available_width() - 4.0;
            let (r, _) = ui.allocate_exact_size(Vec2::new(avail, 1.0), egui::Sense::hover());
            ui.painter().rect_filled(r, Rounding::ZERO, C_BORDER);
        });
        ui.add_space(14.0);

        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.set_width(ui.available_width());
            match self.active_tab {
                Tab::General     => self.tab_general(ui, ctx),
                Tab::Suggestions => self.tab_suggestions(ui),
                Tab::Appearance  => self.tab_appearance(ui),
                Tab::Hotkeys     => self.tab_hotkeys(ui, ctx),
                Tab::Dataset     => self.tab_dataset(ui),
                Tab::Ignored     => self.tab_ignored(ui),
                Tab::Preview     => self.tab_preview(ui),
            }
            ui.add_space(20.0);
        });
    }
}

// ── General tab ────────────────────────────────────────────────────────────
impl GhostTypeApp {
    fn tab_general(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card(ui, |ui| {
            sec_title(ui, "Engine");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Active").size(12.0).color(C_DIM));
                ui.add_space(8.0);
                let mut v = self.config.engine_enabled;
                if toggle_switch(ui, ctx, Id::new("eng_tog"), &mut v) {
                    self.config.engine_enabled = v;
                    if v { let _ = self.engine_tx.send(EngineCmd::Start); }
                    else { let _ = self.engine_tx.send(EngineCmd::Stop); }
                    self.push_config();
                }
                ui.add_space(12.0);
                let col = if self.config.engine_enabled { C_GREEN } else { C_RED };
                ui.label(RichText::new(if self.config.engine_enabled { "Running" } else { "Stopped" })
                    .size(11.0).color(col));
            });
            ui.add_space(8.0);
            sec_title(ui, "Overlay");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Visible").size(12.0).color(C_DIM));
                ui.add_space(8.0);
                let mut v = self.config.overlay_enabled;
                if toggle_switch(ui, ctx, Id::new("ov_tog"), &mut v) {
                    self.config.overlay_enabled = v;
                    if !v { let _ = self.overlay_tx.send(OverlayCmd::Hide); }
                    self.push_config();
                }
            });
        });
    }
}

// ── Suggestions tab ────────────────────────────────────────────────────────
impl GhostTypeApp {
    fn tab_suggestions(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            sec_title(ui, "Mode");
            ui.horizontal_wrapped(|ui| {
                for mode in &["word", "phrase", "hybrid"] {
                    let sel = &self.config.mode == mode;
                    if pill_btn(ui, mode, sel) && !sel {
                        self.config.mode = mode.to_string();
                        self.push_config();
                    }
                    ui.add_space(2.0);
                }
            });
            ui.add_space(10.0);
            sec_title(ui, "Parameters");
            let mut changed = false;
            row(ui, "Min prefix", |ui| {
                let mut pl = self.config.prefix_length as u32;
                if ui.add(egui::DragValue::new(&mut pl).range(1..=6).speed(0.05)).changed() {
                    self.config.prefix_length = pl as usize;
                    changed = true;
                }
                ui.label(RichText::new(format!("{}", self.config.prefix_length)).size(11.0).color(C_TACC));
            });
            row(ui, "Debounce (ms)", |ui| {
                let mut v = self.config.debounce_ms as u32;
                if ui.add(egui::Slider::new(&mut v, 10..=200).show_value(false)).changed() {
                    self.config.debounce_ms = v as u64;
                    changed = true;
                }
                ui.label(RichText::new(format!("{} ms", self.config.debounce_ms)).size(11.0).color(C_TACC));
            });
            if changed { self.push_config(); }
        });
    }
}

// ── Appearance tab ─────────────────────────────────────────────────────────
impl GhostTypeApp {
    fn tab_appearance(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            sec_title(ui, "Colours");
            let mut changed = false;
            row(ui, "Suggestion text", |ui| {
                let mut rgb = hex_to_rgb(&self.config.color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut rgb).changed() {
                    self.config.color = rgb_to_hex(rgb);
                    changed = true;
                }
                ui.label(RichText::new(&self.config.color).monospace().size(10.5).color(C_DIM));
            });
            row(ui, "Popup background", |ui| {
                let mut rgb = hex_to_rgb(&self.config.bg_color);
                if egui::color_picker::color_edit_button_rgb(ui, &mut rgb).changed() {
                    self.config.bg_color = rgb_to_hex(rgb);
                    changed = true;
                }
                ui.label(RichText::new(&self.config.bg_color).monospace().size(10.5).color(C_DIM));
            });
            ui.add_space(8.0);
            sec_title(ui, "Opacity & Size");
            row(ui, "Opacity", |ui| {
                if ui.add(egui::Slider::new(&mut self.config.opacity, 0.1..=1.0).show_value(false)).changed() {
                    changed = true;
                }
                ui.label(RichText::new(format!("{:.0}%", self.config.opacity * 100.0)).size(11.0).color(C_TACC));
            });
            row(ui, "Font size", |ui| {
                if ui.add(egui::Slider::new(&mut self.config.font_size, 8..=36).show_value(false)).changed() {
                    changed = true;
                }
                ui.label(RichText::new(format!("{} px", self.config.font_size)).size(11.0).color(C_TACC));
            });
            row(ui, "Corner radius", |ui| {
                if ui.add(egui::Slider::new(&mut self.config.corner_radius, 0..=20).show_value(false)).changed() {
                    changed = true;
                }
                ui.label(RichText::new(format!("{}", self.config.corner_radius)).size(11.0).color(C_TACC));
            });
            row(ui, "Padding", |ui| {
                if ui.add(egui::Slider::new(&mut self.config.padding, 2..=20).show_value(false)).changed() {
                    changed = true;
                }
                ui.label(RichText::new(format!("{} px", self.config.padding)).size(11.0).color(C_TACC));
            });
            ui.add_space(8.0);
            sec_title(ui, "Font");
            row(ui, "Family", |ui| {
                let mut f = self.config.font.clone();
                if ui.add(egui::TextEdit::singleline(&mut f).desired_width(130.0)
                    .font(FontId::proportional(12.0))).changed()
                {
                    self.config.font = f;
                    changed = true;
                }
            });
            ui.add_space(8.0);
            sec_title(ui, "Popup Position");
            ui.horizontal_wrapped(|ui| {
                for (id, label) in &[("above_caret","Above caret"),("below_caret","Below caret"),("near_mouse","Near mouse")] {
                    let sel = &self.config.position_mode == id;
                    if pill_btn(ui, label, sel) && !sel {
                        self.config.position_mode = id.to_string();
                        changed = true;
                    }
                    ui.add_space(2.0);
                }
            });
            if changed { self.push_config(); }
        });
    }
}

// ── Hotkeys tab ────────────────────────────────────────────────────────────
impl GhostTypeApp {
    fn tab_hotkeys(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        card(ui, |ui| {
            sec_title(ui, "Current binding");
            ui.horizontal(|ui| {
                if self.rebind_pending {
                    // animated "waiting" badge
                    let anim = (ctx.input(|i| i.time) * 2.5).sin() * 0.5 + 0.5;
                    let col = lerp_col(C_ACCENT, C_AHOV, anim as f32);
                    waiting_badge(ui, "  Press any key…  ", col);
                    ui.add_space(8.0);
                    if ghost_btn(ui, "Cancel", false) {
                        self.rebind_pending = false;
                    }
                    ctx.request_repaint();
                } else {
                    hotkey_badge(ui, &self.config.accept_key, true);
                    ui.add_space(10.0);
                    if ghost_btn(ui, "Rebind", true) {
                        self.rebind_pending = true;
                        let _ = self.engine_tx.send(EngineCmd::StartRebind);
                    }
                    ui.add_space(4.0);
                    if ghost_btn(ui, "Reset to Tab", false) {
                        self.config.accept_key = "Tab".into();
                        self.push_config();
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Click Rebind, then press any key or combo (e.g. Ctrl+Space, Alt+X, RCtrl).")
                .size(10.5).color(C_DIM));
        });

        ui.add_space(8.0);

        card(ui, |ui| {
            sec_title(ui, "Quick select");
            ui.add_space(2.0);
            for &(group_label, keys) in HK_GROUPS {
                ui.label(RichText::new(group_label).size(10.5).color(C_DIM));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::splat(4.0);
                    for &key in keys {
                        let sel = self.config.accept_key == key;
                        if hk_badge(ui, key, sel) {
                            self.config.accept_key = key.into();
                            self.push_config();
                        }
                    }
                });
                ui.add_space(8.0);
            }
        });
    }
}

// ── Dataset tab ────────────────────────────────────────────────────────────
impl GhostTypeApp {
    fn tab_dataset(&mut self, ui: &mut egui::Ui) {
        // Stats row
        card(ui, |ui| {
            sec_title(ui, "N-gram Statistics");
            ui.horizontal(|ui| {
                stat_block(ui, "Unigrams",  self.stats.unigrams);
                ui.add_space(6.0);
                stat_block(ui, "Bigrams",   self.stats.bigrams);
                ui.add_space(6.0);
                stat_block(ui, "Trigrams",  self.stats.trigrams);
            });
        });

        ui.add_space(8.0);

        card(ui, |ui| {
            sec_title(ui, "Import");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                if accent_btn(ui, "Import Telegram JSON") {
                    if let Some(paths) = rfd::FileDialog::new().add_filter("JSON",&["json"]).pick_files() {
                        let (mut msgs, mut uni, mut bi, mut tri) = (0,0,0,0);
                        let mut errs: Vec<String> = Vec::new();
                        for path in &paths {
                            match telegram_import::import_file(&self.db, &path.display().to_string()) {
                                Ok(s) => { msgs+=s.messages; uni+=s.unigrams; bi+=s.bigrams; tri+=s.trigrams; }
                                Err(e) => errs.push(format!("{}: {e}", path.display())),
                            }
                        }
                        self.import_status = if errs.is_empty() {
                            format!("Imported {} file(s) — {msgs} msgs, {uni} uni, {bi} bi, {tri} tri", paths.len())
                        } else {
                            format!("{} ok / {} failed. {}", paths.len()-errs.len(), errs.len(), errs.join("; "))
                        };
                        self.refresh_stats();
                        let _ = self.engine_tx.send(EngineCmd::RefreshCache);
                    }
                }
                if ghost_btn(ui, "Re-import Embedded", false) {
                    println!("Re-importing embedded dataset…");
                    telegram_import::import_embedded(&self.db);
                    self.import_status = "Embedded dataset re-imported.".into();
                    self.refresh_stats();
                    let _ = self.engine_tx.send(EngineCmd::RefreshCache);
                }
                if ghost_btn(ui, "Rebuild DB", false) {
                    match self.db.clear_ngrams() {
                        Err(e) => self.import_status = format!("Clear error: {e}"),
                        Ok(()) => {
                            telegram_import::import_embedded(&self.db);
                            self.import_status = "Database rebuilt from embedded data.".into();
                            self.refresh_stats();
                            let _ = self.engine_tx.send(EngineCmd::RefreshCache);
                        }
                    }
                }
                if ghost_btn(ui, "Reload Cache", false) {
                    let _ = self.engine_tx.send(EngineCmd::RefreshCache);
                    self.import_status = "Cache reload requested.".into();
                }
            });
            if !self.import_status.is_empty() {
                ui.add_space(8.0);
                Frame::none().fill(C_INPUT).rounding(Rounding::same(6.0)).inner_margin(Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&self.import_status).size(11.0).color(C_DIM));
                    });
            }
        });
    }
}

// ── Ignored Apps tab ───────────────────────────────────────────────────────
impl GhostTypeApp {
    fn tab_ignored(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            sec_title(ui, "Silenced Processes");
            ui.label(RichText::new("GhostType will not activate in these executables.")
                .size(11.0).color(C_DIM));
            ui.add_space(8.0);

            let mut remove: Option<usize> = None;
            if self.config.ignored_apps.is_empty() {
                ui.label(RichText::new("None configured.").size(12.0).color(C_DIM));
            } else {
                for (i, app) in self.config.ignored_apps.iter().enumerate() {
                    ui.horizontal(|ui| {
                        Frame::none()
                            .fill(C_INPUT).rounding(Rounding::same(4.0))
                            .inner_margin(Margin::symmetric(8.0, 3.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(app).size(12.0).monospace().color(C_TEXT));
                            });
                        if ui.add(egui::Button::new(RichText::new("✕").size(11.0).color(C_RED))
                            .fill(Color32::TRANSPARENT).frame(false)).clicked()
                        {
                            remove = Some(i);
                        }
                    });
                    ui.add_space(2.0);
                }
            }
            if let Some(i) = remove {
                self.config.ignored_apps.remove(i);
                self.push_config();
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let resp = ui.add(egui::TextEdit::singleline(&mut self.new_ignored_app)
                    .hint_text("e.g. chrome.exe").desired_width(150.0)
                    .font(FontId::proportional(12.0)));
                let submit = (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    || accent_btn(ui, "Add");
                if submit && !self.new_ignored_app.trim().is_empty() {
                    self.config.ignored_apps.push(self.new_ignored_app.trim().to_lowercase());
                    self.new_ignored_app.clear();
                    self.push_config();
                }
            });
        });
    }
}

// ── Preview tab ────────────────────────────────────────────────────────────
impl GhostTypeApp {
    fn tab_preview(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            sec_title(ui, "Live Preview");
            ui.label(RichText::new("Reflects your current Appearance settings in real time.")
                .size(11.0).color(C_DIM));
            ui.add_space(12.0);

            let avail = ui.available_width() - 2.0;
            let ph = 80.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(avail, ph), egui::Sense::hover());
            let p = ui.painter_at(rect);

            // Simulated input area
            p.rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(16, 18, 32));
            p.rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0, C_BORDER));

            // Typed text
            let typed = "Hello wo";
            let ty = rect.min.y + ph * 0.56;
            let tx = rect.min.x + 16.0;
            p.text(Pos2::new(tx, ty), egui::Align2::LEFT_TOP, typed,
                FontId::proportional(13.5), Color32::from_rgb(200, 200, 215));
            let tw = typed.len() as f32 * 7.2;

            // Cursor
            p.line_segment(
                [Pos2::new(tx + tw, ty), Pos2::new(tx + tw, ty + 15.0)],
                Stroke::new(1.5, Color32::from_rgba_unmultiplied(200, 200, 215, 200)),
            );

            // Suggestion bubble
            let sug = "rld";
            let (r, g, b)     = self.config.parse_color();
            let text_col = Color32::from_rgb(r, g, b);
            let (br, bg_c, bb) = self.config.parse_bg_color();
            let bg_a = (self.config.opacity * 255.0) as u8;
            let bg_col = Color32::from_rgba_unmultiplied(br, bg_c, bb, bg_a);

            let fsize = 11.5_f32;
            let pad   = (self.config.padding as f32 * 0.65).max(4.0);
            let cr    = self.config.corner_radius as f32;
            let bw    = sug.len() as f32 * 6.8 + pad * 2.0;
            let bh    = fsize + pad * 2.0;
            let bx    = tx + tw;
            let by    = ty - bh - 5.0;

            let bubble = Rect::from_min_size(Pos2::new(bx, by), Vec2::new(bw, bh));
            p.rect_filled(bubble, Rounding::same(cr), bg_col);
            p.text(bubble.min + Vec2::splat(pad), egui::Align2::LEFT_TOP, sug,
                FontId::proportional(fsize), text_col);

            ui.add_space(8.0);
            ui.label(RichText::new("The box simulates a text input; the bubble shows the current popup style.")
                .size(10.0).color(C_DIM));
        });
    }
}

// ── Widget helpers ─────────────────────────────────────────────────────────

/// Animated toggle switch. Returns true when value changed.
fn toggle_switch(ui: &mut egui::Ui, ctx: &egui::Context, id: Id, value: &mut bool) -> bool {
    let size = Vec2::new(44.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let mut changed = false;
    if resp.clicked() { *value = !*value; changed = true; }

    let t = ctx.animate_value_with_time(id, if *value { 1.0f32 } else { 0.0f32 }, 0.15);

    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let track = lerp_col(C_INPUT, C_ACCENT, t);
        let border = lerp_col(C_BORDER, C_ACCENT, t);
        p.rect_filled(rect, Rounding::same(12.0), track);
        p.rect_stroke(rect, Rounding::same(12.0), Stroke::new(1.0, border));
        let kx = egui::lerp(rect.left() + 12.0 ..= rect.right() - 12.0, t);
        let kc = Pos2::new(kx, rect.center().y);
        // shadow
        p.circle_filled(Pos2::new(kc.x + 1.0, kc.y + 1.0), 10.0, Color32::from_black_alpha(50));
        // knob
        p.circle_filled(kc, 10.0, Color32::WHITE);
    }
    changed
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    Frame::none()
        .fill(C_CARD)
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::same(14.0))
        .outer_margin(Margin { left: 16.0, right: 16.0, bottom: 0.0, top: 0.0 })
        .stroke(Stroke::new(1.0, C_BORDER))
        .show(ui, |ui| { contents(ui); });
    ui.add_space(10.0);
}

fn sec_title(ui: &mut egui::Ui, label: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(3.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(r, Rounding::same(2.0), C_ACCENT);
        ui.add_space(6.0);
        ui.label(RichText::new(label).size(11.5).strong().color(C_TACC));
    });
    ui.add_space(8.0);
}

fn hdiv(ui: &mut egui::Ui) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(r, Rounding::ZERO, C_BORDER);
}

/// Two-column row: fixed label, then contents.
fn row(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_height(28.0);
        ui.add_sized([115.0, 20.0], egui::Label::new(
            RichText::new(label).size(12.0).color(C_DIM)
        ));
        contents(ui);
    });
    ui.add_space(2.0);
}

/// Accent (primary) button.
fn accent_btn(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(egui::Button::new(RichText::new(label).size(12.0))
        .fill(C_ADIM)
        .stroke(Stroke::new(1.0, C_ACCENT)))
        .clicked()
}

/// Ghost (secondary) button.
fn ghost_btn(ui: &mut egui::Ui, label: &str, accent: bool) -> bool {
    let border = if accent { C_ACCENT } else { C_BORDER };
    ui.add(egui::Button::new(RichText::new(label).size(12.0))
        .fill(C_INPUT)
        .stroke(Stroke::new(1.0, border)))
        .clicked()
}

/// Pill toggle button (mode selector).
fn pill_btn(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let fill   = if selected { C_ADIM  } else { C_INPUT };
    let border = if selected { C_ACCENT } else { C_BORDER };
    let text   = if selected { C_TACC  } else { C_DIM };
    ui.add(egui::Button::new(RichText::new(label).size(12.0).color(text))
        .fill(fill)
        .stroke(Stroke::new(if selected { 1.5 } else { 1.0 }, border))
        .rounding(Rounding::same(16.0)))
        .clicked()
}

/// Hotkey badge with selection state (in hotkeys grid).
fn hk_badge(ui: &mut egui::Ui, key: &str, selected: bool) -> bool {
    let font = FontId::monospace(11.0);
    let pad  = Vec2::new(9.0, 4.0);
    let text_size = ui.fonts(|f| f.layout_no_wrap(key.to_string(), font.clone(), C_TEXT).size());
    let size = text_size + pad * 2.0;

    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if !ui.is_rect_visible(rect) { return resp.clicked(); }

    let hov = resp.hovered();
    let bg  = if selected { C_ADIM } else if hov { Color32::from_rgb(24, 26, 42) } else { C_INPUT };
    let bc  = if selected { C_ACCENT } else if hov { Color32::from_rgb(60, 62, 95) } else { C_BORDER };
    let tc  = if selected { C_TACC } else if hov { C_TEXT } else { C_DIM };
    let bw  = if selected { 1.5 } else { 1.0 };

    let p = ui.painter();
    p.rect_filled(rect, Rounding::same(4.0), bg);
    p.rect_stroke(rect, Rounding::same(4.0), Stroke::new(bw, bc));
    p.text(rect.min + pad, egui::Align2::LEFT_TOP, key, font, tc);

    resp.clicked()
}

/// Current accept key badge (large, in hotkeys header).
fn hotkey_badge(ui: &mut egui::Ui, key: &str, _accent: bool) {
    Frame::none()
        .fill(C_ADIM)
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(12.0, 5.0))
        .stroke(Stroke::new(1.5, C_ACCENT))
        .show(ui, |ui| {
            ui.label(RichText::new(key).monospace().size(13.0).color(C_TACC).strong());
        });
}

/// Animated "waiting" badge for rebind mode.
fn waiting_badge(ui: &mut egui::Ui, text: &str, col: Color32) {
    Frame::none()
        .fill(Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 20))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(12.0, 5.0))
        .stroke(Stroke::new(1.5, col))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().size(13.0).color(col));
        });
}

fn stat_block(ui: &mut egui::Ui, label: &str, value: i64) {
    Frame::none()
        .fill(C_INPUT)
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::symmetric(14.0, 8.0))
        .stroke(Stroke::new(1.0, C_BORDER))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(fmt_num(value)).size(16.0).strong().color(C_TACC));
                ui.label(RichText::new(label).size(10.0).color(C_DIM));
            });
        });
}

// ── Visual helpers ─────────────────────────────────────────────────────────
fn setup_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.override_text_color      = Some(C_TEXT);
    v.panel_fill               = C_BG;
    v.window_fill              = C_CARD;
    v.faint_bg_color           = C_INPUT;
    v.extreme_bg_color         = Color32::from_rgb(7, 8, 14);
    v.code_bg_color            = C_INPUT;

    v.widgets.noninteractive.bg_fill     = C_CARD;
    v.widgets.noninteractive.fg_stroke   = Stroke::new(1.0, C_DIM);
    v.widgets.noninteractive.bg_stroke   = Stroke::new(1.0, C_BORDER);

    v.widgets.inactive.bg_fill           = C_INPUT;
    v.widgets.inactive.fg_stroke         = Stroke::new(1.0, C_TEXT);
    v.widgets.inactive.bg_stroke         = Stroke::new(1.0, C_BORDER);

    v.widgets.hovered.bg_fill            = Color32::from_rgb(30, 32, 54);
    v.widgets.hovered.fg_stroke          = Stroke::new(1.0, C_TEXT);
    v.widgets.hovered.bg_stroke          = Stroke::new(1.0, C_ACCENT);

    v.widgets.active.bg_fill             = C_ADIM;
    v.widgets.active.fg_stroke           = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.bg_stroke           = Stroke::new(1.5, C_ACCENT);

    v.selection.bg_fill                  = C_ADIM;
    v.selection.stroke                   = Stroke::new(1.0, C_ACCENT);
    v.window_rounding                    = Rounding::same(10.0);
    v.window_stroke                      = Stroke::new(1.0, C_BORDER);
    v.popup_shadow                       = egui::epaint::Shadow::NONE;

    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing    = Vec2::new(8.0, 5.0);
    style.spacing.button_padding  = Vec2::new(10.0, 5.0);
    style.spacing.slider_width    = 100.0;
    style.spacing.indent          = 16.0;
    ctx.set_style(style);
}

fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        lerp_u8(a.r(), b.r(), t),
        lerp_u8(a.g(), b.g(), t),
        lerp_u8(a.b(), b.b(), t),
        lerp_u8(a.a(), b.a(), t),
    )
}
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}
fn hex_to_rgb(s: &str) -> [f32; 3] {
    let (r, g, b) = parse_hex_color(s);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}
fn rgb_to_hex(c: [f32; 3]) -> String {
    let r = (c[0].clamp(0.0,1.0)*255.0) as u8;
    let g = (c[1].clamp(0.0,1.0)*255.0) as u8;
    let b = (c[2].clamp(0.0,1.0)*255.0) as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}
fn fmt_num(n: i64) -> String {
    if n >= 1_000_000 { format!("{:.1}M", n as f64 / 1_000_000.0) }
    else if n >= 1_000 { format!("{:.0}K", n as f64 / 1_000.0) }
    else { n.to_string() }
}
