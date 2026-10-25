//! Dev tool: Explorer's copy dialogs (OperationStatusWindow) as UI Automation sees them, read-only:
//! their titles, child windows and elements, a progress bar's value. `copyprobe [--watch <s>]`
//! looks once, or once a second for that long (start a big copy first).

use windows::core::{Interface, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, EnumWindows, GetClassNameW, GetWindowRect, GetWindowTextW, IsIconic, IsWindowVisible};

fn class_of(hwnd: HWND) -> String {
    let mut name = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut name) };
    String::from_utf16_lossy(&name[..n as usize])
}

fn title_of(hwnd: HWND) -> String {
    let mut text = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut text) };
    String::from_utf16_lossy(&text[..n as usize])
}

unsafe extern "system" fn each(hwnd: HWND, out: LPARAM) -> BOOL {
    if class_of(hwnd) == "OperationStatusWindow" {
        let found = unsafe { &mut *(out.0 as *mut Vec<HWND>) };
        found.push(hwnd);
    }
    true.into()
}

unsafe extern "system" fn child(hwnd: HWND, _: LPARAM) -> BOOL {
    let mut r = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    println!("    child {} {:?} ({}, {}) - ({}, {})", class_of(hwnd), title_of(hwnd), r.left, r.top, r.right, r.bottom);
    true.into()
}

fn dialogs() -> Vec<HWND> {
    let mut found: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut _ as isize));
    }
    found
}

/// Every element under `hwnd`, with a progress bar's or a value's reading.
unsafe fn elements(uia: &IUIAutomation, hwnd: HWND, all: bool) -> windows::core::Result<()> {
    unsafe {
        let root = uia.ElementFromHandle(hwnd)?;
        let found = root.FindAll(TreeScope_Descendants, &uia.CreateTrueCondition()?)?;
        for i in 0..found.Length()? {
            let e = found.GetElement(i)?;
            let ct = e.CurrentControlType()?;
            let name = e.CurrentName().map(|s| s.to_string()).unwrap_or_default();
            let id = e.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default();
            let class = e.CurrentClassName().map(|s| s.to_string()).unwrap_or_default();
            let mut reading = String::new();
            if let Ok(p) = e.GetCurrentPatternAs::<IUIAutomationRangeValuePattern>(UIA_RangeValuePatternId) {
                reading = format!(" range {:?} of {:?}", p.CurrentValue(), p.CurrentMaximum());
            }
            if let Ok(p) = e.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                reading += &format!(" value {:?}", p.CurrentValue().map(|v| v.to_string()));
            }
            if all || ct == UIA_ProgressBarControlTypeId || ct == UIA_TextControlTypeId {
                println!("  {i:3} ct {} [{class}] id={id:?} name={name:?}{reading}", ct.0);
            }
        }
        Ok(())
    }
}

fn main() -> windows::core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let watch: u64 = args.iter().position(|a| a == "--watch").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(0);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        for tick in 0..=watch {
            let found = dialogs();
            println!("t={tick}s: {} dialog(s)", found.len());
            for hwnd in found {
                let mut r = RECT::default();
                let _ = GetWindowRect(hwnd, &mut r);
                println!(
                    "  hwnd {:?} title {:?} iconic {} visible {} ({}, {}) - ({}, {})",
                    hwnd.0,
                    title_of(hwnd),
                    IsIconic(hwnd).as_bool(),
                    IsWindowVisible(hwnd).as_bool(),
                    r.left,
                    r.top,
                    r.right,
                    r.bottom
                );
                if tick == 0 {
                    let _ = EnumChildWindows(Some(hwnd), Some(child), LPARAM(0));
                }
                if let Err(e) = elements(&uia, hwnd, tick == 0) {
                    println!("  uia: {e}");
                }
            }
            if tick < watch {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        let _ = uia.cast::<IUIAutomation>();
    }
    Ok(())
}
