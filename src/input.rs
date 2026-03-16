use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyboardLayout, GetKeyboardState, ToUnicodeEx, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub vk_code: u32,
    pub scan_code: u32,
    pub is_keydown: bool,
}

pub fn resolve_char(event: KeyEvent) -> Option<char> {
    if !event.is_keydown {
        return None;
    }

    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        let thread_id = GetWindowThreadProcessId(hwnd, None);
        let layout = GetKeyboardLayout(thread_id);

        let mut key_state = [0u8; 256];
        if GetKeyboardState(&mut key_state).is_err() {
            return None;
        }

        if (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 {
            key_state[VK_SHIFT.0 as usize] |= 0x80;
        }
        if (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 {
            key_state[VK_MENU.0 as usize] |= 0x80;
        }

        let mut buf = [0u16; 8];
        let rc = ToUnicodeEx(
            event.vk_code,
            event.scan_code,
            &key_state,
            &mut buf,
            0,
            layout,
        );

        if rc > 0 {
            let slice = &buf[..rc as usize];
            let s = String::from_utf16_lossy(slice);
            return s.chars().next();
        }
    }

    None
}