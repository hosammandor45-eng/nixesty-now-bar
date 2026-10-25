//! Explorer's copy dialog (OperationStatusWindow), looked at once a second on its own thread:
//! its title's percent (else its progress bars', through UI Automation) and, once, its words for
//! what it does. Read only: nothing is clicked or moved. Minimised, it still says all this.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationRangeValuePattern, TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_ProgressBarControlTypeId, UIA_RangeValuePatternId,
    UIA_TextControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetWindowTextW, IsWindowVisible};

use super::transfer::{kind_in, percent_in, Pace, TransferKind};
use crate::platform::log::log;

/// What the dialog shows now: what it does, how far, the seconds left at its pace, the dialog.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransferNow {
    pub kind: TransferKind,
    pub pct: u8,
    pub left_s: Option<u64>,
    pub dialog: isize,
}

/// What the UI reads: the dialog now, and what finished since it last looked.
#[derive(Clone, Debug, Default)]
pub struct TransferBox {
    pub now: Option<TransferNow>,
    pub finished: Vec<TransferKind>,
}

pub type TransferMailbox = Arc<Mutex<TransferBox>>;

const LOOK_EVERY: Duration = Duration::from_secs(1);

/// The first visible copy dialog (a minimised one is visible).
fn dialog() -> Option<HWND> {
    let mut after: Option<HWND> = None;
    loop {
        let hwnd = unsafe { FindWindowExW(None, after, w!("OperationStatusWindow"), None) }.ok()?;
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return Some(hwnd);
        }
        after = Some(hwnd);
    }
}

fn title_of(hwnd: HWND) -> String {
    let mut text = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut text) };
    String::from_utf16_lossy(&text[..n as usize])
}

/// The dialog's progress bars, averaged (several operations at once have no percent in the title).
unsafe fn bars_percent(uia: &IUIAutomation, hwnd: HWND) -> Option<u8> {
    unsafe {
        let root = uia.ElementFromHandle(hwnd).ok()?;
        let bar = windows::Win32::System::Variant::VARIANT::from(UIA_ProgressBarControlTypeId.0);
        let found = root.FindAll(TreeScope_Descendants, &uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &bar).ok()?).ok()?;
        let mut shares = Vec::new();
        for i in 0..found.Length().ok()? {
            let Ok(p) = found.GetElement(i).and_then(|e| e.GetCurrentPatternAs::<IUIAutomationRangeValuePattern>(UIA_RangeValuePatternId)) else { continue };
            if let (Ok(v), Ok(max)) = (p.CurrentValue(), p.CurrentMaximum()) {
                if max > 0.0 {
                    shares.push((v / max).clamp(0.0, 1.0));
                }
            }
        }
        (!shares.is_empty()).then(|| (shares.iter().sum::<f64>() / shares.len() as f64 * 100.0).round() as u8)
    }
}

/// The dialog's texts ("3 öğe … kopyalanıyor"), for what it does.
unsafe fn words(uia: &IUIAutomation, hwnd: HWND) -> Vec<String> {
    unsafe {
        let Ok(root) = uia.ElementFromHandle(hwnd) else { return Vec::new() };
        let text = windows::Win32::System::Variant::VARIANT::from(UIA_TextControlTypeId.0);
        let Ok(cond) = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &text) else { return Vec::new() };
        let Ok(found) = root.FindAll(TreeScope_Descendants, &cond) else { return Vec::new() };
        (0..found.Length().unwrap_or(0)).filter_map(|i| found.GetElement(i).ok()?.CurrentName().ok().map(|n| n.to_string())).collect()
    }
}

/// Whether a dialog that went away had finished: nearly there at the last look.
pub fn finished(pct: u8, left_s: Option<u64>) -> bool {
    pct >= 90 || left_s.is_some_and(|s| s <= 3)
}

pub fn start(mailbox: TransferMailbox, post: fn()) {
    let spawned = std::thread::Builder::new().name("transfers".into()).spawn(move || {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let uia: Option<IUIAutomation> = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok();
        let started = Instant::now();
        let (mut pace, mut last, mut kind_of): (Pace, Option<TransferNow>, Option<(isize, TransferKind)>) = (Pace::default(), None, None);
        loop {
            std::thread::sleep(LOOK_EVERY);
            let on = crate::settings::current().enabled && crate::settings::current().transfer_on;
            let seen = on.then(dialog).flatten();
            let now = seen.and_then(|hwnd| {
                let pct = percent_in(&title_of(hwnd)).or_else(|| uia.as_ref().and_then(|u| unsafe { bars_percent(u, hwnd) }))?;
                let kind = match kind_of {
                    Some((h, k)) if h == hwnd.0 as isize => k,
                    _ => {
                        let k = uia.as_ref().map_or(TransferKind::Copy, |u| kind_in(&unsafe { words(u, hwnd) }));
                        log(&format!("transfers: a dialog ({k:?})"));
                        kind_of = Some((hwnd.0 as isize, k));
                        k
                    }
                };
                pace.push(started.elapsed().as_millis() as u64, pct as f32);
                Some(TransferNow { kind, pct, left_s: pace.left_s(), dialog: hwnd.0 as isize })
            });
            let gone = match (last, now) {
                (Some(was), None) => {
                    pace = Pace::default();
                    kind_of = None;
                    // Off by the setting isn't finished.
                    (on && finished(was.pct, was.left_s)).then_some(was.kind)
                }
                _ => None,
            };
            if now == last && gone.is_none() {
                continue;
            }
            last = now;
            if let Ok(mut m) = mailbox.lock() {
                m.now = now;
                m.finished.extend(gone);
            }
            post();
        }
    });
    if let Err(e) = spawned {
        log(&format!("transfers: no thread ({e})"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialog_that_went_away_nearly_done_had_finished() {
        assert!(finished(100, None));
        assert!(finished(92, Some(20)), "the last look a second before the end");
        assert!(finished(70, Some(2)), "a fast copy: two seconds left at the last look");
        assert!(!finished(45, Some(60)), "cancelled halfway");
        assert!(!finished(45, None));
    }
}
