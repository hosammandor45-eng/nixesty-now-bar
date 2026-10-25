//! Explorer's copy progress in the pill: what its dialog's title and text say (in Turkish or
//! English Windows), how long is left by the percent's own pace. The pure part;
//! transfer_worker.rs reads the dialog.

use std::collections::VecDeque;

use super::games::played_text;
use crate::i18n::{percent, tr, trf};

/// A finished copy says so this long.
pub const TRANSFER_DONE_MS: u64 = 4000;
/// The pace is read over this much of the past.
const PACE_WINDOW_MS: u64 = 30_000;
/// A time left needs this much of the past to go on.
const PACE_MIN_MS: u64 = 3_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferKind {
    Copy,
    Move,
    Delete,
}

/// The percent in a dialog's title: "45% complete", "%45 tamamlandı", "45 % terminé".
pub fn percent_in(title: &str) -> Option<u8> {
    let chars: Vec<char> = title.chars().collect();
    let digits = |it: &mut dyn Iterator<Item = &char>| it.take_while(|c| c.is_ascii_digit()).collect::<String>();
    for (i, _) in chars.iter().enumerate().filter(|(_, c)| **c == '%') {
        // "45%", "45 %" (a space of any kind between), or "%45".
        let end = if i > 0 && chars[i - 1].is_whitespace() { i - 1 } else { i };
        let before: String = digits(&mut chars[..end].iter().rev()).chars().rev().collect();
        let after = digits(&mut chars[i + 1..].iter());
        if let Some(n) = [before, after].iter().find_map(|d| d.parse::<u32>().ok()).filter(|n| *n <= 100) {
            return Some(n as u8);
        }
    }
    None
}

/// What the dialog does, by its words in either language (a copy when they don't say).
pub fn kind_in(texts: &[String]) -> TransferKind {
    let all = texts.join(" ").to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| all.contains(w));
    if has(&["taşın", "moving"]) {
        TransferKind::Move
    } else if has(&["silin", "geri dönüşüm", "deleting", "recycling"]) {
        TransferKind::Delete
    } else {
        TransferKind::Copy
    }
}

/// The percent's recent readings, for the time left.
#[derive(Clone, Debug, Default)]
pub struct Pace {
    samples: VecDeque<(u64, f32)>,
}

impl Pace {
    /// A reading at `ms`; one going back (another operation) starts again.
    pub fn push(&mut self, ms: u64, pct: f32) {
        if self.samples.back().is_some_and(|(_, last)| pct < *last) {
            self.samples.clear();
        }
        self.samples.push_back((ms, pct));
        while self.samples.front().is_some_and(|(at, _)| ms.saturating_sub(*at) > PACE_WINDOW_MS) {
            self.samples.pop_front();
        }
    }

    /// Seconds left at the recent pace (`None` until it's known).
    pub fn left_s(&self) -> Option<u64> {
        let (&(t0, p0), &(t1, p1)) = (self.samples.front()?, self.samples.back()?);
        let span_s = t1.saturating_sub(t0) as f32 / 1000.0;
        if t1.saturating_sub(t0) < PACE_MIN_MS || p1 <= p0 {
            return None;
        }
        Some(((100.0 - p1).max(0.0) / ((p1 - p0) / span_s)).round() as u64)
    }
}

/// "Kopyalanıyor", "Taşınıyor", "Siliniyor".
pub fn verb(kind: TransferKind) -> &'static str {
    match kind {
        TransferKind::Copy => tr("Kopyalanıyor"),
        TransferKind::Move => tr("Taşınıyor"),
        TransferKind::Delete => tr("Siliniyor"),
    }
}

/// "Kopyalama bitti", "Taşıma bitti", "Silme bitti".
pub fn done_text(kind: TransferKind) -> &'static str {
    match kind {
        TransferKind::Copy => tr("Kopyalama bitti"),
        TransferKind::Move => tr("Taşıma bitti"),
        TransferKind::Delete => tr("Silme bitti"),
    }
}

/// The pill's line after the verb: "%45 · 2 dk", "%45 · 1 dk'dan az", "%45".
pub fn pill_detail(pct: u8, left_s: Option<u64>) -> String {
    match left_s {
        None => percent(pct),
        Some(s) if s < 60 => trf("{} · 1 dk'dan az", &[&percent(pct)]),
        Some(s) => format!("{} · {}", percent(pct), played_text(s as i64)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_percent_in_either_language() {
        assert_eq!(percent_in("45% complete"), Some(45));
        assert_eq!(percent_in("%45 tamamlandı"), Some(45));
        assert_eq!(percent_in("100% complete"), Some(100));
        assert_eq!(percent_in("45 % terminé"), Some(45));
        assert_eq!(percent_in("2 işlem sürüyor"), None, "no percent: two at once");
        assert_eq!(percent_in("Dosya Değiştir veya Atla"), None);
        assert_eq!(percent_in("%250"), None, "not a percent");
    }

    #[test]
    fn what_it_does_by_its_words() {
        let words = |w: &[&str]| kind_in(&w.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(words(&["1 öğe İndirilenler konumundan Masaüstü konumuna kopyalanıyor"]), TransferKind::Copy);
        assert_eq!(words(&["Moving 3 items from Downloads to Desktop"]), TransferKind::Move);
        assert_eq!(words(&["3 öğe taşınıyor"]), TransferKind::Move);
        assert_eq!(words(&["Deleting 12 items"]), TransferKind::Delete);
        assert_eq!(words(&["12 öğe siliniyor"]), TransferKind::Delete);
        assert_eq!(words(&["Recycling 2 items"]), TransferKind::Delete);
        assert_eq!(words(&[]), TransferKind::Copy);
    }

    #[test]
    fn the_time_left_follows_the_pace() {
        let mut p = Pace::default();
        p.push(0, 10.0);
        p.push(2_000, 12.0);
        assert_eq!(p.left_s(), None, "two seconds say little");
        p.push(5_000, 15.0);
        // 5 % in 5 s: the 85 % left take 85 s.
        assert_eq!(p.left_s(), Some(85));
        p.push(40_000, 50.0);
        assert_eq!(p.left_s(), None, "the last 30 s hold one reading");
        p.push(45_000, 55.0);
        assert_eq!(p.left_s(), Some(45), "1 % a second now: 45 s");
        let mut stuck = Pace::default();
        stuck.push(0, 30.0);
        stuck.push(10_000, 30.0);
        assert_eq!(stuck.left_s(), None, "no progress, no guess");
    }

    #[test]
    fn words() {
        assert_eq!(pill_detail(45, None), "%45");
        assert_eq!(pill_detail(45, Some(30)), "%45 · 1 dk'dan az");
        assert_eq!(pill_detail(45, Some(130)), "%45 · 2 dk");
        assert_eq!((verb(TransferKind::Copy), done_text(TransferKind::Move)), ("Kopyalanıyor", "Taşıma bitti"));
        crate::i18n::in_language(crate::i18n::Language::English, || {
            assert_eq!(pill_detail(45, Some(130)), "45% · 2 min");
            assert_eq!(verb(TransferKind::Copy), "Copying");
        });
    }
}
