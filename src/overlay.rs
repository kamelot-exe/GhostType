use crossbeam_channel::{Receiver, Sender};
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    GetTextExtentPoint32W, RedrawWindow, SelectObject, SetBkMode, SetTextColor, TextOutW,
    HBRUSH, HFONT, PAINTSTRUCT, RDW_ERASE, RDW_INVALIDATE, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

static OVERLAY_TX: OnceLock<Sender<OverlayCmd>> = OnceLock::new();

#[derive(Debug, Clone)]
pub enum OverlayCmd {
    Show(String),
    Hide,
    UpdateConfig {
        color: (u8, u8, u8),
        opacity: f32,
        font_name: String,
        font_size: u32,
    },
    Quit,
}

pub fn send_cmd(cmd: OverlayCmd) {
    if let Some(tx) = OVERLAY_TX.get() {
        let _ = tx.send(cmd);
    }
}

pub fn show_suggestion(s: &str) {
    send_cmd(OverlayCmd::Show(s.to_string()));
}

pub fn hide_suggestion() {
    send_cmd(OverlayCmd::Hide);
}

const TIMER_ID: usize = 1;
const COLORKEY: u32 = 0x00010101;

struct OverlayState {
    hwnd: HWND,
    text: String,
    visible: bool,
    color: (u8, u8, u8),
    font_name: String,
    font_size: u32,
    rx: Receiver<OverlayCmd>,
    font: HFONT,
}

static mut OVERLAY_STATE: Option<OverlayState> = None;

pub fn run_overlay_thread(rx: Receiver<OverlayCmd>, tx: Sender<OverlayCmd>) {
    let _ = OVERLAY_TX.set(tx);

    unsafe {
        let class_name = w!("GhostTypeOverlay");
        let h_instance = GetModuleHandleW(None).unwrap_or_default();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(overlay_wnd_proc),
            hInstance: h_instance.into(),
            lpszClassName: class_name,
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            ..Default::default()
        };

        RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("GhostType"),
            WS_POPUP,
            0, 0, 400, 40,
            HWND::default(),
            HMENU::default(),
            h_instance,
            None,
        )
        .unwrap_or_default();

        SetLayeredWindowAttributes(hwnd, COLORREF(COLORKEY), 255, LWA_COLORKEY).ok();

        let font = create_font("Segoe UI", 16);

        OVERLAY_STATE = Some(OverlayState {
            hwnd,
            text: String::new(),
            visible: false,
            color: (160, 160, 160),
            font_name: "Segoe UI".into(),
            font_size: 16,
            rx,
            font,
        });

        SetTimer(hwnd, TIMER_ID, 16, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if let Some(state) = OVERLAY_STATE.take() {
            DeleteObject(state.font);
        }
    }
}

unsafe fn create_font(name: &str, size: u32) -> HFONT {
    let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut face_name = [0u16; 32];
    for (i, c) in name_wide.iter().take(31).enumerate() {
        face_name[i] = *c;
    }
    CreateFontW(
        size as i32,
        0, 0, 0,
        400,
        0, 0, 0,
        0,
        0, 0, 4,
        0,
        PCWSTR(face_name.as_ptr()),
    )
}

unsafe fn process_commands() {
    let state = match OVERLAY_STATE.as_mut() {
        Some(s) => s,
        None => return,
    };

    while let Ok(cmd) = state.rx.try_recv() {
        match cmd {
            OverlayCmd::Show(text) => {
                state.text = text;
                state.visible = true;
                position_near_caret(state);
                ShowWindow(state.hwnd, SW_SHOWNOACTIVATE);
                RedrawWindow(state.hwnd, None, None, RDW_INVALIDATE | RDW_ERASE);
            }
            OverlayCmd::Hide => {
                if state.visible {
                    state.visible = false;
                    ShowWindow(state.hwnd, SW_HIDE);
                }
            }
            OverlayCmd::UpdateConfig { color, opacity: _, font_name, font_size } => {
                state.color = color;
                if state.font_name != font_name || state.font_size != font_size {
                    DeleteObject(state.font);
                    state.font = create_font(&font_name, font_size);
                    state.font_name = font_name;
                    state.font_size = font_size;
                }
            }
            OverlayCmd::Quit => {
                PostQuitMessage(0);
            }
        }
    }
}

unsafe fn position_near_caret(state: &OverlayState) {
    let fg = GetForegroundWindow();
    if !fg.is_invalid() {
        let fg_thread = GetWindowThreadProcessId(fg, None);

        let mut gui_info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };

        if GetGUIThreadInfo(fg_thread, &mut gui_info).is_ok() {
            let rc = gui_info.rcCaret;
            if rc.right > 0 || rc.bottom > 0 {
                let mut pt = POINT {
                    x: rc.left,
                    y: rc.bottom + 2,
                };
                ClientToScreen(gui_info.hwndCaret, &mut pt);
                let _ = SetWindowPos(
                    state.hwnd,
                    HWND_TOPMOST,
                    pt.x, pt.y, 0, 0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
                return;
            }
        }
    }

    // Fallback: near mouse cursor
    let mut cursor_pos = POINT::default();
    if GetCursorPos(&mut cursor_pos).is_ok() {
        let _ = SetWindowPos(
            state.hwnd,
            HWND_TOPMOST,
            cursor_pos.x + 20,
            cursor_pos.y + 20,
            0, 0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER => {
            if wparam.0 == TIMER_ID {
                process_commands();
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let bg_brush = CreateSolidBrush(COLORREF(COLORKEY));
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc).ok();
            FillRect(hdc, &rc, bg_brush);
            DeleteObject(bg_brush);

            if let Some(state) = OVERLAY_STATE.as_ref() {
                if !state.text.is_empty() {
                    let old_font = SelectObject(hdc, state.font);
                    SetBkMode(hdc, TRANSPARENT);
                    let (r, g, b) = state.color;
                    SetTextColor(hdc, COLORREF(
                        r as u32 | ((g as u32) << 8) | ((b as u32) << 16),
                    ));

                    let wide: Vec<u16> = state.text.encode_utf16().collect();
                    TextOutW(hdc, 4, 4, &wide);

                    let mut text_size = SIZE::default();
                    GetTextExtentPoint32W(hdc, &wide, &mut text_size);
                    let new_w = text_size.cx + 12;
                    let new_h = text_size.cy + 10;
                    let _ = SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0, 0,
                        new_w, new_h,
                        SWP_NOMOVE | SWP_NOACTIVATE,
                    );

                    SelectObject(hdc, old_font);
                }
            }

            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
