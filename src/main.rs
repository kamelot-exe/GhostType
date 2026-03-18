#![allow(static_mut_refs)]

mod config;
mod db;
mod hook;
mod input;
mod ngram_cache;
mod overlay;
mod state;
mod suggest;
mod telegram_import;
mod ui;

use crate::config::{is_modifier_key, vk_to_name, Config};
use crate::db::SuggestionDb;
use crate::hook::run_hook_loop;
use crate::input::{resolve_char, KeyEvent};
use crate::ngram_cache::NgramCache;
use crate::overlay::OverlayCmd;
use crate::state::AppState;
use crate::suggest::{finalize_buffer, refresh_suggestion};
use crate::ui::{EngineCmd, GhostTypeApp};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;
use std::env;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

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

    // ── First-run: import embedded dataset if DB is empty ────────────────────
    if db.count_unigrams() == 0 {
        println!("First run detected — importing embedded dataset…");
        let stats = telegram_import::import_embedded(&db);
        println!(
            "Dataset ready: {} msgs, {} unigrams, {} bigrams, {} trigrams",
            stats.messages, stats.unigrams, stats.bigrams, stats.trigrams
        );
    }

    println!("GhostType starting...");

    // Channels
    let (key_tx, key_rx) = unbounded::<KeyEvent>();
    let (overlay_tx, overlay_rx) = unbounded::<OverlayCmd>();
    let (engine_cmd_tx, engine_cmd_rx) = unbounded::<EngineCmd>();
    let (rebind_tx, rebind_rx) = unbounded::<String>();
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
        let (br, bg, bb) = config.parse_bg_color();
        let _ = overlay_tx.send(OverlayCmd::UpdateConfig {
            color: (r, g, b),
            bg_color: (br, bg, bb),
            opacity: config.opacity,
            font_name: config.font.clone(),
            font_size: config.font_size,
            corner_radius: config.corner_radius,
            padding: config.padding,
            position_mode: config.position_mode.clone(),
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
            rebind_tx,
            config_clone,
        );
    });

    // Thread 4: UI (runs on main thread for egui/winit compatibility)
    println!("Opening settings UI...");
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([620.0, 720.0])
            .with_min_inner_size([480.0, 420.0])
            .with_title("GhostType"),
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
                rebind_rx,
            )))
        }),
    );

    // UI closed — shut down overlay
    overlay::send_cmd(OverlayCmd::Quit);
}

fn run_engine(
    db: Arc<SuggestionDb>,
    key_rx: Receiver<KeyEvent>,
    overlay_tx: Sender<OverlayCmd>,
    engine_cmd_rx: Receiver<EngineCmd>,
    rebind_tx: Sender<String>,
    initial_config: Config,
) {
    let mut state = AppState {
        engine_running: initial_config.engine_enabled,
        overlay_visible: initial_config.overlay_enabled,
        ..Default::default()
    };
    let mut config = initial_config;
    let mut last_suggestion_time = Instant::now();
    let mut rebind_mode = false;

    println!("Loading n-gram cache...");
    let mut cache = NgramCache::load_from_db(&db);
    println!(
        "Cache loaded: {} unigrams, {} bigram keys, {} trigram keys",
        cache.unigrams.len(),
        cache.bigrams.len(),
        cache.trigrams.len()
    );

    loop {
        // Process engine commands (non-blocking)
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
                EngineCmd::RefreshCache => {
                    println!("Refreshing n-gram cache...");
                    cache.refresh(&db);
                    println!("Cache refreshed.");
                }
                EngineCmd::StartRebind => {
                    rebind_mode = true;
                    state.clear_suggestion();
                    let _ = overlay_tx.send(OverlayCmd::Hide);
                }
            }
        }

        match key_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                // Rebind capture: grab first non-modifier key press
                if rebind_mode {
                    if !is_modifier_key(event.vk_code) {
                        let key_name = build_key_string(event.vk_code);
                        let _ = rebind_tx.send(key_name);
                        rebind_mode = false;
                    }
                    continue;
                }

                if !state.engine_running {
                    continue;
                }

                // Skip ignored apps
                if !config.ignored_apps.is_empty() {
                    if let Some(proc_name) = get_foreground_process_name() {
                        let proc_lower = proc_name.to_lowercase();
                        if config
                            .ignored_apps
                            .iter()
                            .any(|app| proc_lower.contains(&app.to_lowercase()))
                        {
                            continue;
                        }
                    }
                }

                handle_event(
                    &cache,
                    &mut state,
                    &config,
                    event,
                    &overlay_tx,
                    &mut last_suggestion_time,
                );
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Build human-readable key string with current modifier state.
/// Examples: "Tab", "Ctrl+Space", "Shift+A"
fn build_key_string(vk: u32) -> String {
    let ctrl = is_ctrl_pressed();
    let shift = is_shift_pressed();
    let alt = is_alt_pressed();
    let key_name = vk_to_name(vk);

    let mut parts: Vec<&str> = Vec::new();
    if ctrl { parts.push("Ctrl"); }
    if shift { parts.push("Shift"); }
    if alt { parts.push("Alt"); }

    if parts.is_empty() {
        key_name
    } else {
        format!("{}+{}", parts.join("+"), key_name)
    }
}

fn get_foreground_process_name() -> Option<String> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .ok()?;
        let _ = windows::Win32::Foundation::CloseHandle(handle);
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(|s| s.to_string())
    }
}

fn is_ctrl_pressed() -> bool {
    unsafe { (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 }
}

fn is_shift_pressed() -> bool {
    unsafe { (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 }
}

fn is_alt_pressed() -> bool {
    unsafe { (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 }
}

fn is_accept_key(event: &KeyEvent, config: &Config) -> bool {
    let (vk, need_ctrl, need_shift, need_alt) = config.accept_key_parsed();

    // Single special modifier keys (e.g. "RCtrl" used alone)
    // These arrive as their own VK code without any modifier state check needed
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU, VK_LSHIFT, VK_RSHIFT};
    let solo_modifier = [
        VK_LCONTROL.0 as u32, VK_RCONTROL.0 as u32,
        VK_LMENU.0 as u32, VK_RMENU.0 as u32,
        VK_LSHIFT.0 as u32, VK_RSHIFT.0 as u32,
    ];
    if solo_modifier.contains(&vk) {
        return event.vk_code == vk;
    }

    if event.vk_code != vk {
        return false;
    }

    if need_ctrl && !is_ctrl_pressed() { return false; }
    if need_shift && !is_shift_pressed() { return false; }
    if need_alt && !is_alt_pressed() { return false; }

    // If no modifiers required, make sure modifiers aren't accidentally held
    // (skip this check for Tab since some apps have Shift+Tab)
    if !need_ctrl && !need_shift && !need_alt {
        use windows::Win32::UI::Input::KeyboardAndMouse::VK_TAB;
        if vk == VK_TAB.0 as u32 {
            // Tab is OK even without explicit modifier check
            return true;
        }
        // For other keys, don't fire if Ctrl is held (might be Ctrl+key shortcut in another app)
        if is_ctrl_pressed() || is_alt_pressed() {
            return false;
        }
    }

    true
}

fn handle_event(
    cache: &NgramCache,
    state: &mut AppState,
    config: &Config,
    event: KeyEvent,
    overlay_tx: &Sender<OverlayCmd>,
    last_time: &mut Instant,
) {
    // Accept suggestion
    if is_accept_key(&event, config) {
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
            finalize_buffer(state);
            let _ = overlay_tx.send(OverlayCmd::Hide);
            return;
        }
        0x1B => {
            // Escape
            state.clear_suggestion();
            let _ = overlay_tx.send(OverlayCmd::Hide);
            return;
        }
        0x20 => {
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

    // Trim buffer to last 200 chars
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

    // Debounce
    let now = Instant::now();
    let debounce = Duration::from_millis(config.debounce_ms);
    if now.duration_since(*last_time) < debounce {
        return;
    }
    *last_time = now;

    refresh_suggestion(cache, state);

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
        inputs.push(INPUT {
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
        });
        inputs.push(INPUT {
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
        });
    }

    unsafe {
        let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
