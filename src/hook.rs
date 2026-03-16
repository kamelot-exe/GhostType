use crate::input::KeyEvent;
use crossbeam_channel::Sender;
use once_cell::sync::OnceCell;
use windows::core::Error;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

static KEY_TX: OnceCell<Sender<KeyEvent>> = OnceCell::new();

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);
            if let Some(tx) = KEY_TX.get() {
                let _ = tx.send(KeyEvent {
                    vk_code: kb.vkCode,
                    scan_code: kb.scanCode,
                    is_keydown: true,
                });
            }
        }
    }

    unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
}

pub fn run_hook_loop(tx: Sender<KeyEvent>) -> Result<(), Error> {
    let _ = KEY_TX.set(tx);

    unsafe {
        let h_instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), h_instance, 0)?;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hook);
    }

    Ok(())
}

