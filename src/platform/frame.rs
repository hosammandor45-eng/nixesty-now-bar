//! A window that keeps Windows' frame (rounded corners, shadow, open and minimise animations) but
//! draws its own title bar: the settings window's and the setup's.

use std::cell::Cell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DWMWINDOWATTRIBUTE,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DefWindowProcW, GetClassNameW, IsZoomed, SetWindowPos, SetWindowsHookExW, UnhookWindowsHookEx, CBT_CREATEWNDW,
    HCBT_CREATEWND, HHOOK, NCCALCSIZE_PARAMS, SM_CXPADDEDBORDER, SM_CYFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, WH_CBT, WM_NCCALCSIZE, WS_CHILD,
};

/// Takes over a window made without the hook (it existed already): its title bar goes now.
pub fn adopt(hwnd: HWND) {
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(frame_proc), 1, 0);
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

pub fn style_frame(hwnd: HWND, dark: bool) {
    // COLORREF is 0x00BBGGRR: Theme.ground and Theme.text.
    let (bg, text): (u32, u32) = if dark { (0x001C_1715, 0x00FC_F6F3) } else { (0x00FA_F5F3, 0x0031_2118) };
    let on: i32 = dark as i32;
    // 19 before Windows 10 20H1. The colours and corners are Windows 11's: they fail on 10.
    let dark_mode = DWMWINDOWATTRIBUTE(crate::platform::os::build().map_or(20, crate::platform::os::dark_mode_attribute) as i32);
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, dark_mode, &on as *const i32 as _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &bg as *const u32 as _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &bg as *const u32 as _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, &text as *const u32 as _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND as *const _ as _, 4);
    }
}

/// Windows keeps the whole frame (rounded corners, the shadow, the side and bottom resize
/// borders, the minimise and restore animations) and gives up only its title bar: the client
/// area reaches the window's top, where the window draws its own (as Windows Terminal does).
unsafe extern "system" fn frame_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _: usize, _: usize) -> LRESULT {
    if msg == WM_NCCALCSIZE {
        // wParam TRUE: NCCALCSIZE_PARAMS, whose first rect is the window's; FALSE (while the window
        // is created): that rect alone. Either way it becomes the client rect.
        let rect = if wp.0 != 0 { unsafe { &raw mut (*(lp.0 as *mut NCCALCSIZE_PARAMS)).rgrc[0] } } else { lp.0 as *mut RECT };
        let top = unsafe { (*rect).top };
        let result = unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
        unsafe { (*rect).top = top };
        // Maximised, the window overhangs the screen by its frame: keep that much off the top.
        if unsafe { IsZoomed(hwnd) }.as_bool() {
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            unsafe { (*rect).top += GetSystemMetricsForDpi(SM_CYFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi) };
        }
        return result;
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

thread_local! {
    /// The hook that catches the window as winit creates it, and the window's dark mode.
    static CREATE_HOOK: Cell<(HHOOK, bool)> = const { Cell::new((HHOOK(std::ptr::null_mut()), false)) };
}

/// Takes over the title bar from the window's first message: a CBT hook on this thread subclasses
/// the window as winit creates it, before it has a size. It is shown with its frame, so Windows'
/// open animation (a fade and a slight zoom) plays on the window as it stays, not on a frameless
/// one whose edges and layout change some 40 ms in. `adopt` still tells winit and Slint.
pub fn frame_from_creation(dark: bool) {
    if let Ok(hook) = unsafe { SetWindowsHookExW(WH_CBT, Some(on_create), None, GetCurrentThreadId()) } {
        CREATE_HOOK.with(|h| h.set((hook, dark)));
    }
}

unsafe extern "system" fn on_create(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HCBT_CREATEWND as i32 {
        let hwnd = HWND(wp.0 as *mut _);
        let style = unsafe { (*(*(lp.0 as *const CBT_CREATEWNDW)).lpcs).style } as u32;
        let mut class = [0u16; 32];
        let len = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
        // winit's window class (its event target and glutin's dummy window have their own).
        if style & WS_CHILD.0 == 0 && class[..len].iter().copied().eq("Window Class".encode_utf16()) {
            let (hook, dark) = CREATE_HOOK.with(|h| h.replace((HHOOK::default(), false)));
            unsafe {
                let _ = SetWindowSubclass(hwnd, Some(frame_proc), 1, 0);
                let _ = UnhookWindowsHookEx(hook);
            }
            style_frame(hwnd, dark);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}
