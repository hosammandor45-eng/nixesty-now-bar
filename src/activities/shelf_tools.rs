//! The shelf footer's tools: which recogniser reads a picture, how big it is read, what the
//! results are called. The pure part; shelf_tools_worker.rs runs them.

use std::path::{Path, PathBuf};

use crate::i18n::{tr, trf};

/// The recogniser to read with: the first of the user's languages Windows has one for, else the
/// first it has (a Latin one still reads most of a Turkish screenshot); and whether the user's
/// first language had none. Tags compare by their language ("tr" is "tr-TR").
pub fn pick_recognizer(available: &[String], preferred: &[String]) -> Option<(String, bool)> {
    let language = |tag: &str| tag.split('-').next().unwrap_or(tag).to_ascii_lowercase();
    let has = |want: &String| available.iter().find(|a| language(a) == language(want));
    let first_missing = preferred.first().is_some_and(|p| has(p).is_none());
    let pick = preferred.iter().find_map(has).or(available.first())?;
    Some((pick.clone(), first_missing))
}

/// How much to scale a `w`×`h` picture before reading it: small text reads better twice as big,
/// and the recogniser takes nothing longer than `max` pixels.
pub fn ocr_scale(w: u32, h: u32, max: u32) -> f32 {
    match w.max(h) {
        0 => 1.0,
        long => (max as f32 / long as f32).min(2.0),
    }
}

/// The recogniser's lines as the text: one line each, Windows' line breaks.
pub fn join_lines(lines: &[String]) -> String {
    lines.join("\r\n")
}

/// The long side a shrunk picture keeps at most: a 1080p screen's.
pub const SHRINK_SIDE: u32 = 1920;

/// A `w`×`h` picture's size with its long side at most `max` (never made bigger).
pub fn shrink_size(w: u32, h: u32, max: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max {
        return (w, h);
    }
    let fit = |v: u32| ((v as u64 * max as u64 + long as u64 / 2) / long as u64).max(1) as u32;
    (fit(w), fit(h))
}

/// Premultiplied BGRA laid on white, as JPEG's BGR.
pub fn flatten_on_white(bgra: &[u8]) -> Vec<u8> {
    bgra.chunks_exact(4).flat_map(|p| {
        let clear = 255 - p[3];
        [p[0].saturating_add(clear), p[1].saturating_add(clear), p[2].saturating_add(clear)]
    }).collect()
}

/// A name without its extension (a dot file keeps its name).
fn stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// "ekran.png" → "ekran (küçük).jpg".
pub fn shrunk_name(name: &str) -> String {
    format!("{} ({}).jpg", stem(name), tr("küçük"))
}

/// The zip's name after the newest of what goes in: "Fotoğraflar.zip", "ekran.zip",
/// "ekran ve 2 dosya.zip".
pub fn zip_name(first: &str, first_is_dir: bool, count: usize) -> String {
    let base = if first_is_dir { first } else { stem(first) };
    match count {
        0 | 1 => format!("{base}.zip"),
        n => format!("{}.zip", trf("{} ve {} dosya", &[&base, &(n - 1)])),
    }
}

/// `name` in `dir`, or "ad (2).zip", "ad (3).zip"… when it's taken.
pub fn unique_path(dir: &Path, name: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = dir.join(name);
    if !exists(&first) {
        return first;
    }
    let (base, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..).map(|n| dir.join(format!("{base} ({n}){ext}"))).find(|p| !exists(p)).unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_big_picture_comes_down_to_1920() {
        assert_eq!(shrink_size(4000, 3000, SHRINK_SIDE), (1920, 1440));
        assert_eq!(shrink_size(1080, 2400, SHRINK_SIDE), (864, 1920), "tall");
        assert_eq!(shrink_size(1280, 720, SHRINK_SIDE), (1280, 720), "never bigger");
        assert_eq!(shrink_size(30_000, 10, SHRINK_SIDE), (1920, 1), "never zero");
    }

    #[test]
    fn transparency_lies_on_white() {
        // Premultiplied: half-transparent red is (0, 0, 128, 128); clear is (0, 0, 0, 0).
        assert_eq!(flatten_on_white(&[0, 0, 128, 128, 0, 0, 0, 0, 10, 20, 30, 255]), vec![127, 127, 255, 255, 255, 255, 10, 20, 30]);
    }

    #[test]
    fn made_files_are_named_after_theirs() {
        assert_eq!(shrunk_name("ekran.png"), "ekran (küçük).jpg");
        assert_eq!(shrunk_name("tatil.JPEG"), "tatil (küçük).jpg");
        assert_eq!(shrunk_name("grafik"), "grafik (küçük).jpg");
        assert_eq!(zip_name("Fotoğraflar", true, 1), "Fotoğraflar.zip");
        assert_eq!(zip_name("rapor.v2.pdf", false, 1), "rapor.v2.zip");
        assert_eq!(zip_name("ekran.png", false, 3), "ekran ve 2 dosya.zip");
        crate::i18n::in_language(crate::i18n::Language::English, || {
            assert_eq!(shrunk_name("ekran.png"), "ekran (small).jpg");
            assert_eq!(zip_name("ekran.png", false, 3), "ekran and 2 more.zip");
        });
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = Path::new(r"C:\Raf");
        assert_eq!(unique_path(dir, "ekran.zip", |_| false), PathBuf::from(r"C:\Raf\ekran.zip"));
        let taken = [PathBuf::from(r"C:\Raf\ekran.zip"), PathBuf::from(r"C:\Raf\ekran (2).zip")];
        assert_eq!(unique_path(dir, "ekran.zip", |p| taken.iter().any(|t| t == p)), PathBuf::from(r"C:\Raf\ekran (3).zip"));
        assert_eq!(unique_path(dir, "Makefile", |p| p == Path::new(r"C:\Raf\Makefile")), PathBuf::from(r"C:\Raf\Makefile (2)"));
    }


    fn tags(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_users_language_reads_first() {
        assert_eq!(pick_recognizer(&tags(&["en-US", "tr"]), &tags(&["tr-TR", "en-US"])), Some(("tr".into(), false)));
        assert_eq!(pick_recognizer(&tags(&["en-US", "tr"]), &tags(&["en-GB"])), Some(("en-US".into(), false)), "by language, not region");
        assert_eq!(pick_recognizer(&tags(&["EN-us"]), &tags(&["en"])), Some(("EN-us".into(), false)), "any case");
    }

    #[test]
    fn without_turkish_another_reads_and_says_so() {
        assert_eq!(pick_recognizer(&tags(&["en-US"]), &tags(&["tr-TR"])), Some(("en-US".into(), true)));
        assert_eq!(pick_recognizer(&tags(&["de-DE", "en-US"]), &tags(&["tr-TR", "en-US"])), Some(("en-US".into(), true)), "the second choice before any");
        assert_eq!(pick_recognizer(&tags(&["de-DE"]), &[]), Some(("de-DE".into(), false)), "no languages known: nothing missing");
        assert_eq!(pick_recognizer(&[], &tags(&["tr-TR"])), None, "Windows has none");
    }

    #[test]
    fn small_pictures_read_bigger_and_big_ones_fit() {
        assert_eq!(ocr_scale(1920, 1080, 10_000), 2.0, "a screenshot, twice");
        assert_eq!(ocr_scale(1920, 1080, 2_600), 2600.0 / 1920.0, "as much as the recogniser takes");
        assert_eq!(ocr_scale(12_000, 800, 10_000), 10_000.0 / 12_000.0, "a long one comes down");
        assert_eq!(ocr_scale(0, 0, 10_000), 1.0);
    }

    #[test]
    fn lines_keep_their_breaks() {
        assert_eq!(join_lines(&tags(&["Hata 0x80070005", "Erişim engellendi."])), "Hata 0x80070005\r\nErişim engellendi.");
        assert_eq!(join_lines(&[]), "");
    }
}
