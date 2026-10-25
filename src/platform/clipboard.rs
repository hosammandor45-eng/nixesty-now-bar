//! Text onto the clipboard (the shelf's "Metni kopyala", the question card's answer) and off it.

use windows::core::Result;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

/// CF_UNICODETEXT.
const UNICODE_TEXT: u32 = 13;

/// The clipboard's Unicode text, if it holds some (the question card's Ctrl+V).
pub fn get_text(owner: HWND) -> Option<String> {
    unsafe {
        OpenClipboard(Some(owner)).ok()?;
        let read = || -> Option<String> {
            let memory = HGLOBAL(GetClipboardData(UNICODE_TEXT).ok()?.0);
            let units = GlobalSize(memory) / 2;
            let at = GlobalLock(memory) as *const u16;
            if at.is_null() {
                return None;
            }
            let all = std::slice::from_raw_parts(at, units);
            let text = String::from_utf16_lossy(&all[..all.iter().position(|u| *u == 0).unwrap_or(units)]);
            let _ = GlobalUnlock(memory);
            Some(text)
        };
        let text = read();
        let _ = CloseClipboard();
        text
    }
}

/// Puts `text` on the clipboard as Unicode text, owned by `owner`.
pub fn set_text(owner: HWND, text: &str) -> Result<()> {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2)?;
        let to = GlobalLock(memory) as *mut u16;
        if to.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(windows::core::Error::from_thread());
        }
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), to, utf16.len());
        let _ = GlobalUnlock(memory);
        if let Err(e) = OpenClipboard(Some(owner)) {
            let _ = GlobalFree(Some(memory));
            return Err(e);
        }
        let _ = EmptyClipboard();
        // Once set, the clipboard owns the memory.
        let set = SetClipboardData(UNICODE_TEXT, Some(HANDLE(memory.0)));
        let _ = CloseClipboard();
        if let Err(e) = set {
            let _ = GlobalFree(Some(memory));
            return Err(e);
        }
    }
    Ok(())
}
