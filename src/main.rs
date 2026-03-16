mod db;
mod hook;
mod input;
mod overlay;
mod state;
mod suggest;
mod telegram_import;

use crate::db::SuggestionDb;
use crate::hook::{run_hook_loop, ACCEPT_VK};
use crate::input::{resolve_char, KeyEvent};
use crate::state::AppState;
use crate::suggest::{finalize_phrase_and_word, refresh_suggestion};
use crossbeam_channel::unbounded;
use std::env;
use std::thread;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_UNICODE,
};

fn main() {
    let args: Vec<String> = env::args().collect();

    let db = SuggestionDb::open("typing_memory.db").expect("failed to open sqlite");

    if args.get(1).map(|s| s.as_str()) == Some("import") {
        telegram_import::import_default_files(&db);
        return;
    }

    println!("GhostType started.");
    println!("Accept hotkey: Tab");
    println!("Run `ghosttype import` to import Telegram JSON.");

    let (tx, rx) = unbounded::<KeyEvent>();

    thread::spawn(move || {
        if let Err(e) = run_hook_loop(tx) {
            eprintln!("hook error: {e}");
        }
    });

    let mut state = AppState::default();

    while let Ok(event) = rx.recv() {
        handle_event(&db, &mut state, event);
    }
}

fn handle_event(db: &SuggestionDb, state: &mut AppState, event: KeyEvent) {
    if event.vk_code == ACCEPT_VK {
        if let Some(suffix) = state.current_suffix.clone() {
            send_unicode_text(&suffix);
            state.typed_buffer.push_str(&suffix);
            state.clear_suggestion();
            overlay::hide_suggestion();
        }
        return;
    }

    match event.vk_code {
        0x08 => {
            state.typed_buffer.pop();
            refresh_suggestion(db, state);
            show_or_hide(state);
            return;
        }
        0x0D => {
            finalize_phrase_and_word(db, state);
            overlay::hide_suggestion();
            return;
        }
        0x20 => {
            state.typed_buffer.push(' ');
            refresh_suggestion(db, state);
            show_or_hide(state);
            return;
        }
        _ => {}
    }

    if let Some(ch) = resolve_char(event) {
        if ch.is_control() {
            return;
        }

        state.typed_buffer.push(ch);

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

        refresh_suggestion(db, state);
        show_or_hide(state);
    }
}

fn show_or_hide(state: &AppState) {
    if let Some(suffix) = &state.current_suffix {
        overlay::show_suggestion(suffix);
    } else {
        overlay::hide_suggestion();
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
                    dwFlags: KEYEVENTF_UNICODE | windows::Win32::UI::Input::KeyboardAndMouse::KEYEVENTF_KEYUP,
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