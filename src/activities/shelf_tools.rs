//! The shelf footer's tools: which recogniser reads a picture, how big it is read, what the
//! results are called. The pure part; shelf_tools_worker.rs runs them.

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

#[cfg(test)]
mod tests {
    use super::*;

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
