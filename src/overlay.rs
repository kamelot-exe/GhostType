use crossbeam_channel::{Receiver, Sender};
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, EndPaint,
    FillRect, GetTextExtentPoint32W, RoundRect, SelectObject, SetBkMode, SetTextColor, TextOutW,
    HBRUSH, HFONT, PAINTSTRUCT, PS_NULL, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::WindowsAndMessaging::*;

static OVERLAY_TX: OnceLock<Sender<OverlayCmd>> = OnceLock::new();

#[derive(Debug, Clone)]
pub enum OverlayCmd {
    Show(String),
    Hide,
    UpdateConfig {
        color: (u8, u8, u8),
        bg_color: (u8, u8, u8),
        opacity: f32,
        font_name: String,
        font_size: u32,
        corner_radius: u32,
        padding: u32,
        position_mode: String,
    },
    Quit,
}

pub fn send_cmd(cmd: OverlayCmd) {
    if let Some(tx) = OVERLAY_TX.get() {
        let _ = tx.send(cmd);
    }
}

// Colorkey: a very dark near-black used as transparent mask.
// The popup background must NOT equal this value.
const COLORKEY: u32 = 0x00010101;
const TIMER_ID: usize = 1;

struct OverlayState {
    hwnd: HWND,
    text: String,
    visible: bool,
    /// Foreground text color (r,g,b)
    color: (u8, u8, u8),
    /// Popup background color (r,g,b)
    bg_color: (u8, u8, u8),
    opacity: f32,
    font_name: String,
    font_size: u32,
    corner_radius: u32,
    padding: u32,
    position_mode: String,
    rx: Receiver<OverlayCmd>,
    font: HFONT,
}

impl OverlayState {
    fn text_colorref(&self) -> COLORREF {
        let (r, g, b) = self.color;
        COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
    }

    fn bg_colorref(&self) -> COLORREF {
        let (r, g, b) = self.bg_color;
        // Guard: bg must not equal COLORKEY
        let v = r as u32 | ((g as u32) << 8) | ((b as u32) << 16);
        if v == COLORKEY {
            COLORREF(0x00020202)
        } else {
            COLORREF(v)
        }
    }


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
            WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE,
            class_name,
            w!(""),
            WS_POPUP,
            0, 0, 10, 10,
            HWND::default(),
            HMENU::default(),
            h_instance,
            None,
        )
        .unwrap_or_default();

        // Use both colorkey (for window-shape transparency) and alpha (for popup opacity)
        apply_layered_attrs(hwnd, 0.92);

        let font = create_font("Segoe UI", 14);

        OVERLAY_STATE = Some(OverlayState {
            hwnd,
            text: String::new(),
            visible: false,
            color: (200, 200, 224),
            bg_color: (28, 28, 46),
            opacity: 0.92,
            font_name: "Segoe UI".into(),
            font_size: 14,
            corner_radius: 6,
            padding: 7,
            position_mode: "above_caret".into(),
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
            let _ = DeleteObject(state.font);
        }
    }
}

unsafe fn apply_layered_attrs(hwnd: HWND, opacity: f32) {
    let alpha = (opacity.clamp(0.1, 1.0) * 255.0) as u8;
    // LWA_COLORKEY | LWA_ALPHA: colorkey makes mask color transparent,
    // alpha makes everything else semi-transparent
    let _ = SetLayeredWindowAttributes(
        hwnd,
        COLORREF(COLORKEY),
        alpha,
        LWA_COLORKEY | LWA_ALPHA,
    );
}

unsafe fn create_font(name: &str, size: u32) -> HFONT {
    let name_wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut face_name = [0u16; 32];
    for (i, c) in name_wide.iter().take(31).enumerate() {
        face_name[i] = *c;
    }
    CreateFontW(
        -(size as i32), // negative = cell height (more standard)
        0, 0, 0,
        400,  // FW_NORMAL
        0, 0, 0,
        0,    // DEFAULT_CHARSET
        0, 0, 4, // CLEARTYPE_QUALITY
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
                position_overlay(state);
                let _ = ShowWindow(state.hwnd, SW_SHOWNOACTIVATE);
                invalidate(state.hwnd);
            }
            OverlayCmd::Hide => {
                if state.visible {
                    state.visible = false;
                    state.text.clear();
                    let _ = ShowWindow(state.hwnd, SW_HIDE);
                }
            }
            OverlayCmd::UpdateConfig {
                color,
                bg_color,
                opacity,
                font_name,
                font_size,
                corner_radius,
                padding,
                position_mode,
            } => {
                state.color = color;
                state.bg_color = bg_color;
                state.corner_radius = corner_radius;
                state.padding = padding;
                state.position_mode = position_mode;

                if (state.opacity - opacity).abs() > 0.01 {
                    state.opacity = opacity;
                    apply_layered_attrs(state.hwnd, opacity);
                }

                if state.font_name != font_name || state.font_size != font_size {
                    let _ = DeleteObject(state.font);
                    state.font = create_font(&font_name, font_size);
                    state.font_name = font_name;
                    state.font_size = font_size;
                }

                if state.visible {
                    invalidate(state.hwnd);
                }
            }
            OverlayCmd::Quit => {
                PostQuitMessage(0);
            }
        }
    }
}

unsafe fn invalidate(hwnd: HWND) {
    let _ = InvalidateRect(hwnd, None, false);
}

/// Measure text with current font, returning (width, height).
/// Uses a temporary screen DC so we can measure before painting.
unsafe fn measure_text(state: &OverlayState) -> (i32, i32) {
    if state.text.is_empty() {
        return (0, 0);
    }
    let hdc = windows::Win32::Graphics::Gdi::GetDC(HWND::default());
    let old_font = SelectObject(hdc, state.font);
    let wide: Vec<u16> = state.text.encode_utf16().collect();
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &wide, &mut sz);
    SelectObject(hdc, old_font);
    windows::Win32::Graphics::Gdi::ReleaseDC(HWND::default(), hdc);
    (sz.cx, sz.cy)
}

unsafe fn position_overlay(state: &OverlayState) {
    let (text_w, text_h) = measure_text(state);
    let pad = state.padding as i32;
    let popup_w = (text_w + pad * 2).max(40);
    let popup_h = (text_h + pad * 2).max(20);

    // Resize window first
    let _ = SetWindowPos(
        state.hwnd,
        HWND_TOPMOST,
        0, 0,
        popup_w, popup_h,
        SWP_NOMOVE | SWP_NOACTIVATE,
    );

    let above = state.position_mode != "below_caret";

    // Strategy 1: Win32 caret via GetGUIThreadInfo (Notepad, classic apps)
    let fg = GetForegroundWindow();
    if !fg.is_invalid() {
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let mut gui_info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(fg_thread, &mut gui_info).is_ok() {
            let rc = gui_info.rcCaret;
            if (rc.right > 0 || rc.bottom > 0) && !gui_info.hwndCaret.is_invalid() {
                let (cx, cy) = if above {
                    (rc.left, rc.top - popup_h - 4)
                } else {
                    (rc.left, rc.bottom + 4)
                };
                let mut pt = POINT { x: cx, y: cy };
                let _ = ClientToScreen(gui_info.hwndCaret, &mut pt);

                // Sanity: x on screen, y positive
                if pt.x > -200 && pt.x < 5000 && pt.y > 0 && pt.y < 4000 {
                    let _ = SetWindowPos(
                        state.hwnd,
                        HWND_TOPMOST,
                        pt.x, pt.y, 0, 0,
                        SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                    return;
                }
                // If y < 0 (caret at top of screen), show below
                if pt.y <= 0 && above {
                    let mut pt2 = POINT { x: rc.left, y: rc.bottom + 4 };
                    let _ = ClientToScreen(gui_info.hwndCaret, &mut pt2);
                    if pt2.x > -200 && pt2.x < 5000 && pt2.y > 0 {
                        let _ = SetWindowPos(
                            state.hwnd,
                            HWND_TOPMOST,
                            pt2.x, pt2.y, 0, 0,
                            SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                        return;
                    }
                }
            }
        }
    }

    // Strategy 2: Mouse cursor position (great for Chrome/Telegram/Electron)
    // User is typing → cursor is near the input field
    if state.position_mode != "fixed" {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_ok() {
            let x = cursor.x - popup_w / 2;
            let y = if above {
                cursor.y - popup_h - 24
            } else {
                cursor.y + 20
            };
            // Clamp to visible area (rough)
            let y = if y < 4 { cursor.y + 20 } else { y };
            let _ = SetWindowPos(
                state.hwnd,
                HWND_TOPMOST,
                x.max(4), y, 0, 0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
            return;
        }
    }

    // Strategy 3: fixed bottom-center of foreground window
    if !fg.is_invalid() {
        let mut win_rect = RECT::default();
        if GetWindowRect(fg, &mut win_rect).is_ok() {
            let win_w = win_rect.right - win_rect.left;
            let x = win_rect.left + win_w / 2 - popup_w / 2;
            let y = win_rect.bottom - popup_h - 60;
            let _ = SetWindowPos(
                state.hwnd,
                HWND_TOPMOST,
                x, y, 0, 0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => {
            // We handle all drawing in WM_PAINT — prevent default erase
            LRESULT(1)
        }

        WM_TIMER => {
            if wparam.0 == TIMER_ID {
                process_commands();
            }
            LRESULT(0)
        }

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let mut client_rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut client_rc);

            // Fill entire window with colorkey → transparent background
            let ck_brush = CreateSolidBrush(COLORREF(COLORKEY));
            let _ = FillRect(hdc, &client_rc, ck_brush);
            let _ = DeleteObject(ck_brush);

            if let Some(state) = OVERLAY_STATE.as_ref() {
                if !state.text.is_empty() && state.visible {
                    let old_font = SelectObject(hdc, state.font);

                    // Measure text
                    let wide: Vec<u16> = state.text.encode_utf16().collect();
                    let mut sz = SIZE::default();
                    let _ = GetTextExtentPoint32W(hdc, &wide, &mut sz);

                    let pad = state.padding as i32;
                    let popup_w = sz.cx + pad * 2;
                    let popup_h = sz.cy + pad * 2;

                    // Draw rounded rectangle background
                    // Null pen → no outline
                    let null_pen = CreatePen(PS_NULL, 0, COLORREF(0));
                    let bg_brush = CreateSolidBrush(state.bg_colorref());
                    let old_pen = SelectObject(hdc, null_pen);
                    let old_brush = SelectObject(hdc, bg_brush);

                    let r = (state.corner_radius as i32 * 2).max(2);
                    // RoundRect(hdc, left, top, right, bottom, width_ellipse, height_ellipse)
                    let _ = RoundRect(hdc, 0, 0, popup_w, popup_h, r, r);

                    SelectObject(hdc, old_pen);
                    SelectObject(hdc, old_brush);
                    let _ = DeleteObject(null_pen);
                    let _ = DeleteObject(bg_brush);

                    // Draw text
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, state.text_colorref());
                    let _ = TextOutW(hdc, pad, pad, &wide);

                    // Resize window to match content
                    if client_rc.right != popup_w || client_rc.bottom != popup_h {
                        let _ = SetWindowPos(
                            hwnd,
                            HWND_TOPMOST,
                            0, 0,
                            popup_w, popup_h,
                            SWP_NOMOVE | SWP_NOACTIVATE,
                        );
                    }

                    SelectObject(hdc, old_font);
                }
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
