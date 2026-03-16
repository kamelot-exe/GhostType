mod config;
mod db;
mod hook;
mod input;
mod overlay;
mod state;
mod suggest;
mod telegram_import;
mod ui;

use crate::config::Config;
use crate::db::SuggestionDb;
use crate::hook::{run_hook_loop, ACCEPT_VK};
use crate::input::{resolve_char, KeyEvent};
use crate::overlay::OverlayCmd;
use crate::state::AppState;
use crate::suggest::{finalize_phrase_and_word, refresh_suggestion};
use crate::ui::{EngineCmd, GhostTypeApp};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::env;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use eframe::egui;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
};

const DEBOUNCE_MS: u64 = 30;

fn main() {
    let args: Vec<String> = env::args().collect();
    let config = Config::load();

    let db = Arc::new(
        SuggestionDb::open("typing_memory.db").expect("failed to open sqlite"),
    );

    // CLI: import subcommand
    if args.get(1).map(|s| s.as_str()) == Some("import") {
        telegram_import::import_default_files(&db);
        return;
    }

    println!("GhostType starting...");

    // Channels
    let (key_tx, key_rx) = unbounded::<KeyEvent>();
    let (overlay_tx, overlay_rx) = unbounded::<OverlayCmd>();
    let (engine_cmd_tx, engine_cmd_rx) = unbounded::<EngineCmd>();
    let overlay_tx_clone = overlay_tx.clone();

    // Thread 1: Keyboard Hook
    thread::spawn(move || {
        if let Err(e) = run_hook_loop(key_tx) {
            eprintln!("hook error: {e}");
        }
    });

    // Thread 2: Overlay Renderer
    let overlay_tx_for_thread = overlay_tx.clone();
    thread::spawn(move || {
        overlay::run_overlay_thread(overlay_rx, overlay_tx_for_thread);
    });

    // Send initial config to overlay
    {
        let (r, g, b) = config.parse_color();
        let _ = overlay_tx.send(OverlayCmd::UpdateConfig {
            color: (r, g, b),
            opacity: config.opacity,
            font_name: config.font.clone(),
            font_size: config.font_size,
        });
    }

    // Thread 3: Suggestion Engine
    let db_engine = Arc::clone(&db);
    let overlay_tx_engine = overlay_tx.clone();
    let config_clone = config.clone();
    thread::spawn(move || {
        run_engine(
            db_engine,
            key_rx,
            overlay_tx_engine,
            engine_cmd_rx,
            config_clone,
        );
    });

    // Thread 4: UI (runs on main thread for egui compatibility)
    println!("Opening settings UI...");
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 600.0])
            .with_title("GhostType Settings"),
        ..Default::default()
    };

    let db_ui = Arc::clone(&db);
    let _ = eframe::run_native(
        "GhostType",
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(GhostTypeApp::new(
                config,
                db_ui,
                overlay_tx_clone,
                engine_cmd_tx,
            )))
        }),
    );

    // UI closed — shut down overlay
    let _ = overlay::send_cmd(OverlayCmd::Quit);
}

fn run_engine(
    db: Arc<SuggestionDb>,
    key_rx: Receiver<KeyEvent>,
    overlay_tx: Sender<OverlayCmd>,
    engine_cmd_rx: Receiver<EngineCmd>,
    initial_config: Config,
) {
    let mut state = AppState {
        engine_running: initial_config.engine_enabled,
        overlay_visible: initial_config.overlay_enabled,
        ..Default::default()
    };
    let mut config = initial_config;
    let mut last_suggestion_time = Instant::now();

    loop {
        // Check engine commands (non-blocking)
        while let Ok(cmd) = engine_cmd_rx.try_recv() {
            match cmd {
                EngineCmd::Start => state.engine_running = true,
                EngineCmd::Stop => {
                    state.engine_running = false;
                    state.clear_suggestion();
                    let _ = overlay_tx.send(OverlayCmd::Hide);
                }
                EngineCmd::UpdateConfig(new_config) => {
                    config = new_config;
                    state.engine_running = config.engine_enabled;
                    state.overlay_visible = config.overlay_enabled;
                }
            }
        }

        // Wait for keyboard event with timeout
        match key_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                if !state.engine_running {
                    continue;
                }
                handle_event(&db, &mut state, &config, event, &overlay_tx, &mut last_suggestion_time);
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn handle_event(
    db: &SuggestionDb,
    state: &mut AppState,
    _config: &Config,
    event: KeyEvent,
    overlay_tx: &Sender<OverlayCmd>,
    last_time: &mut Instant,
) {
    // Accept suggestion
    if event.vk_code == ACCEPT_VK {
        if let Some(suffix) = state.current_suffix.clone() {
            send_unicode_text(&suffix);
            state.typed_buffer.push_str(&suffix);
            state.clear_suggestion();
            let _ = overlay_tx.send(OverlayCmd::Hide);
        }
        return;
    }

    match event.vk_code {
        0x08 => {
            // Backspace
            state.typed_buffer.pop();
        }
        0x0D => {
            // Enter
            finalize_phrase_and_word(db, state);
            let _ = overlay_tx.send(OverlayCmd::Hide);
            return;
        }
        0x1B => {
            // Escape — dismiss suggestion
            state.clear_suggestion();
            let _ = overlay_tx.send(OverlayCmd::Hide);
            return;
        }
        0x20 => {
            // Space
            state.typed_buffer.push(' ');
        }
        _ => {
            if let Some(ch) = resolve_char(event) {
                if ch.is_control() {
                    return;
                }
                state.typed_buffer.push(ch);
            } else {
                return;
            }
        }
    }

    // Trim buffer
    if state.typed_buffer.len() > 200 {
        let tail: String = state
            .typed_buffer
            .chars()
            .rev()
            .take(200)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        state.typed_buffer = tail;
    }

    // Debounce suggestion refresh
    let now = Instant::now();
    if now.duration_since(*last_time) < Duration::from_millis(DEBOUNCE_MS) {
        return;
    }
    *last_time = now;

    refresh_suggestion(db, state);

    if state.overlay_visible {
        if let Some(suffix) = &state.current_suffix {
            let _ = overlay_tx.send(OverlayCmd::Show(suffix.clone()));
        } else {
            let _ = overlay_tx.send(OverlayCmd::Hide);
        }
    }
}

fn send_unicode_text(text: &str) {
    let mut inputs: Vec<INPUT> = Vec::new();

    for unit in text.encode_utf16() {
        let key_down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: Default::default(),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        let key_up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: Default::default(),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        inputs.push(key_down);
        inputs.push(key_up);
    }

    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
