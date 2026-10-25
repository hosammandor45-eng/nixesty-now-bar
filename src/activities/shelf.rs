//! The shelf: files dropped on the pill, kept (newest first) until removed or the app restarts.

use std::path::{Path, PathBuf};

use crate::i18n::tr;

/// A 13th item pushes the oldest out (three rows of four on the card).
pub const SHELF_MAX: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub struct ShelfItem {
    pub path: PathBuf,
    pub name: String,
    pub kind: FileKind,
}

/// What a tile without a preview shows (its colour and glyph).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Image,
    Video,
    Audio,
    Pdf,
    Archive,
    Document,
    Code,
    Folder,
    Other,
}

/// The kind by the name's extension (any case); folders whatever they are called.
pub fn file_kind(name: &str, is_dir: bool) -> FileKind {
    use FileKind::*;
    if is_dir {
        return Folder;
    }
    let Some(ext) = extension(name) else { return Other };
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic" | "avif" | "svg" | "ico" | "tif" | "tiff" => Image,
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "wmv" | "m4v" => Video,
        "mp3" | "flac" | "wav" | "m4a" | "ogg" | "opus" | "aac" => Audio,
        "pdf" => Pdf,
        "zip" | "rar" | "7z" | "gz" | "tar" | "xz" | "bz2" => Archive,
        "doc" | "docx" | "txt" | "md" | "rtf" | "odt" | "xls" | "xlsx" | "csv" | "ppt" | "pptx" => Document,
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "json" | "html" | "css" | "c" | "cpp" | "h" | "cs" | "go" | "java"
        | "kt" | "swift" | "toml" | "yaml" | "yml" | "xml" | "sh" | "ps1" | "sql" => Code,
        _ => Other,
    }
}

/// Lowercase extension; a dot file (".gitignore") has none.
fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then(|| ext.to_lowercase())
}

/// The tile's corner chip: "PDF", "MP4"; none past five letters.
pub fn extension_label(name: &str) -> Option<String> {
    extension(name).filter(|e| e.chars().count() <= 5).map(|e| e.to_uppercase())
}

#[derive(Clone, Debug, Default)]
pub struct Shelf {
    /// Newest first: the grid's order, and `items[0]` is the pill's thumbnail.
    items: Vec<ShelfItem>,
    /// The tiles clicked for the footer's chips, by path (a drop moves them down the grid).
    selected: Vec<PathBuf>,
}

/// Windows paths compare without case and either separator.
fn key(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

impl Shelf {
    /// Adds the paths not already on the shelf; returns how many were added.
    pub fn add(&mut self, paths: &[PathBuf], is_dir: impl Fn(&Path) -> bool) -> usize {
        // Only the newest SHELF_MAX can stay: a 10 000-file drop doesn't scan (or ask the disk)
        // for the rest.
        let paths = &paths[paths.len().saturating_sub(SHELF_MAX)..];
        let mut added = 0;
        for path in paths {
            if self.items.iter().any(|i| key(&i.path) == key(path)) {
                continue;
            }
            let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
            let kind = file_kind(&name, is_dir(path));
            self.items.insert(0, ShelfItem { path: path.clone(), name, kind });
            added += 1;
        }
        self.items.truncate(SHELF_MAX);
        added
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.items.len() {
            let gone = self.items.remove(i);
            self.selected.retain(|p| key(p) != key(&gone.path));
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.selected.clear();
    }

    /// A click on tile `i`: selected, or let go.
    pub fn toggle(&mut self, i: usize) {
        let Some(item) = self.items.get(i) else { return };
        let k = key(&item.path);
        if self.selected.iter().any(|p| key(p) == k) {
            self.selected.retain(|p| key(p) != k);
        } else {
            self.selected.push(item.path.clone());
        }
    }

    /// Only `paths` selected (those on the shelf): what a tool just made.
    pub fn select(&mut self, paths: &[PathBuf]) {
        let items = &self.items;
        self.selected = paths.iter().filter(|p| items.iter().any(|i| key(&i.path) == key(p))).cloned().collect();
    }

    pub fn is_selected(&self, i: usize) -> bool {
        self.items.get(i).is_some_and(|item| self.selected.iter().any(|p| key(p) == key(&item.path)))
    }

    /// What the footer acts on: the selected items, or all of them with none selected; newest first.
    pub fn targets(&self) -> Vec<&ShelfItem> {
        let picked: Vec<&ShelfItem> = (0..self.items.len()).filter(|i| self.is_selected(*i)).map(|i| &self.items[i]).collect();
        if picked.is_empty() { self.items.iter().collect() } else { picked }
    }

    pub fn items(&self) -> &[ShelfItem] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Drops the items whose files are gone; true if any went.
    pub fn retain_existing(&mut self, exists: impl Fn(&Path) -> bool) -> bool {
        let before = self.items.len();
        self.items.retain(|i| exists(&i.path));
        let items = &self.items;
        self.selected.retain(|p| items.iter().any(|i| key(&i.path) == key(p)));
        self.items.len() != before
    }
}

/// What the shelf card's footer does with its files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShelfOp {
    /// Windows' share sheet (Quick Share, Nearby Sharing, the apps).
    Share,
    /// A picture's text onto the clipboard (Windows' own recogniser).
    CopyText,
    /// Pictures made smaller, as JPEGs beside them.
    Shrink,
    /// One zip of them all.
    Zip,
}

/// A chip's label.
pub fn op_label(op: ShelfOp) -> &'static str {
    match op {
        ShelfOp::Share => tr("Paylaş"),
        ShelfOp::CopyText => tr("Metni kopyala"),
        ShelfOp::Shrink => tr("Küçült"),
        ShelfOp::Zip => tr("Zip yap"),
    }
}

/// A picture WIC decodes (the shrink and the text recogniser take these; a drawing has no pixels).
pub fn is_raster(name: &str) -> bool {
    matches!(extension(name).as_deref(), Some("png" | "jpg" | "jpeg" | "bmp" | "gif" | "tif" | "tiff" | "webp" | "heic" | "jxr"))
}

/// The footer's chips for `targets`, in their order: Paylaş with a file among them, Metni
/// kopyala for one picture, Küçült for pictures only, Zip yap for two or more or a folder.
pub fn ops(targets: &[&ShelfItem]) -> Vec<ShelfOp> {
    let picture = |i: &&ShelfItem| i.kind != FileKind::Folder && is_raster(&i.name);
    let mut out = Vec::new();
    if targets.iter().any(|i| i.kind != FileKind::Folder) {
        out.push(ShelfOp::Share);
    }
    if targets.len() == 1 && targets.iter().all(picture) {
        out.push(ShelfOp::CopyText);
    }
    if !targets.is_empty() && targets.iter().all(picture) {
        out.push(ShelfOp::Shrink);
    }
    if targets.len() > 1 || targets.iter().any(|i| i.kind == FileKind::Folder) {
        out.push(ShelfOp::Zip);
    }
    out
}

/// Grid rows on the card: four tiles a row, one to three rows.
pub fn rows(n: usize) -> usize {
    n.div_ceil(4).clamp(1, 3)
}

/// The card's height: the header, 75 DIP tiles 10 DIP apart, then the footer's chips (12 DIP
/// above, 28 tall, 16 below).
pub fn card_height_dip(n: usize) -> f32 {
    44.0 + 85.0 * rows(n) as f32 + FOOTER_GAP_DIP + FOOTER_CHIP_DIP + 16.0
}

/// The footer's gap under the last tile and its chips' height.
pub const FOOTER_GAP_DIP: f32 = 12.0;
pub const FOOTER_CHIP_DIP: f32 = 28.0;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn files(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    fn no_dirs(_: &Path) -> bool {
        false
    }

    #[test]
    fn add_skips_duplicates_ignoring_case_and_separators() {
        let mut s = Shelf::default();
        assert_eq!(s.add(&files(&[r"C:\A\x.png"]), no_dirs), 1);
        assert_eq!(s.add(&files(&["c:/a/X.PNG", r"C:\A\x.png"]), no_dirs), 0);
        assert_eq!(s.items().len(), 1);
    }

    #[test]
    fn thirteenth_item_pushes_the_oldest_out() {
        let mut s = Shelf::default();
        for i in 0..13 {
            s.add(&[PathBuf::from(format!(r"C:\f{i}.txt"))], no_dirs);
        }
        assert_eq!(s.items().len(), SHELF_MAX);
        assert_eq!(s.items()[0].name, "f12.txt");
        assert!(!s.items().iter().any(|i| i.name == "f0.txt"));
    }

    #[test]
    fn a_huge_drop_only_looks_at_what_it_keeps() {
        let mut s = Shelf::default();
        let many: Vec<PathBuf> = (0..10_000).map(|i| PathBuf::from(format!(r"C:\x\f{i}.jpg"))).collect();
        let asked = std::cell::Cell::new(0);
        let added = s.add(&many, |_| {
            asked.set(asked.get() + 1);
            false
        });
        assert_eq!((added, s.items().len()), (12, 12));
        assert_eq!(s.items()[0].name, "f9999.jpg");
        assert_eq!(asked.get(), 12, "the folder check (disk IO) runs for the kept 12, not 10 000");
    }

    #[test]
    fn newest_first_within_one_drop() {
        let mut s = Shelf::default();
        s.add(&files(&[r"C:\a.txt", r"C:\b.txt"]), no_dirs);
        let names: Vec<&str> = s.items().iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["b.txt", "a.txt"]);
    }

    #[test]
    fn remove_and_clear() {
        let mut s = Shelf::default();
        s.add(&files(&[r"C:\a.txt", r"C:\b.txt"]), no_dirs);
        s.remove(99);
        assert_eq!(s.items().len(), 2);
        s.remove(0);
        assert_eq!(s.items()[0].name, "a.txt");
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn names_for_roots_and_long_names() {
        let long = "a".repeat(200) + ".txt";
        let mut s = Shelf::default();
        s.add(&files(&[r"D:\", r"C:\x\Makefile"]), |p| p == Path::new(r"D:\"));
        s.add(&[PathBuf::from(format!(r"C:\x\{long}"))], no_dirs);
        let names: Vec<&str> = s.items().iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, [long.as_str(), "Makefile", r"D:\"]);
        assert_eq!(s.items()[2].kind, FileKind::Folder);
        assert_eq!(s.items()[1].kind, FileKind::Other);
    }

    #[test]
    fn retain_existing_drops_missing() {
        let mut s = Shelf::default();
        s.add(&files(&[r"C:\a.txt", r"C:\gone.txt", r"C:\b.txt"]), no_dirs);
        assert!(s.retain_existing(|p| p != Path::new(r"C:\gone.txt")));
        assert_eq!(s.items().len(), 2);
        assert!(!s.retain_existing(|_| true));
    }

    #[test]
    fn rows_and_height() {
        assert_eq!([rows(1), rows(4), rows(5), rows(12)], [1, 1, 2, 3]);
        // The tiles, then the footer's chips (12 + 28 + 16).
        assert_eq!(card_height_dip(1), 185.0);
        assert_eq!(card_height_dip(5), 270.0);
        assert_eq!(card_height_dip(12), 355.0);
    }

    #[test]
    fn a_click_selects_and_the_selection_follows_the_files() {
        let mut s = Shelf::default();
        s.add(&files(&[r"C:\a.png", r"C:\b.png", r"C:\c.png"]), no_dirs);
        s.toggle(1);
        assert_eq!((s.is_selected(0), s.is_selected(1)), (false, true), "b, the middle one");
        s.add(&files(&[r"C:\d.png"]), no_dirs);
        assert!(s.is_selected(2), "b moved down a place and stays selected");
        s.toggle(2);
        assert!(!s.is_selected(2), "a second click lets it go");
        s.toggle(0);
        s.remove(0);
        assert!(!(0..3).any(|i| s.is_selected(i)), "removed with its file");
        s.toggle(0);
        assert!(s.retain_existing(|p| p != Path::new(r"C:\c.png")));
        assert!(!s.is_selected(0) && s.targets().len() == 2, "gone from disk: from the selection too");
        s.toggle(1);
        s.clear();
        s.add(&files(&[r"C:\a.png"]), no_dirs);
        assert!(!s.is_selected(0), "cleared with the shelf");
        s.toggle(9);
        assert!(s.targets().len() == 1, "no tile there");
    }

    #[test]
    fn what_a_tool_made_comes_selected() {
        let mut s = Shelf::default();
        s.add(&files(&[r"C:\a.png", r"C:\b.png"]), no_dirs);
        s.toggle(1);
        s.add(&files(&[r"C:\a (küçük).jpg"]), no_dirs);
        s.select(&files(&[r"C:\a (küçük).jpg", r"C:\yok.jpg"]));
        let names: Vec<&str> = s.targets().iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["a (küçük).jpg"], "the new one alone, ready to share");
    }

    #[test]
    fn the_footer_acts_on_the_selection_or_everything() {
        let mut s = Shelf::default();
        s.add(&files(&[r"C:\a.png", r"C:\b.pdf", r"C:\c.jpg"]), no_dirs);
        let names = |t: Vec<&ShelfItem>| t.iter().map(|i| i.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(s.targets()), ["c.jpg", "b.pdf", "a.png"], "none selected: all, newest first");
        s.toggle(2);
        s.toggle(0);
        assert_eq!(names(s.targets()), ["c.jpg", "a.png"], "the shelf's order");
    }

    #[test]
    fn the_footer_offers_what_fits() {
        use ShelfOp::*;
        let item = |name: &str, dir: bool| ShelfItem { path: PathBuf::from(format!(r"C:\x\{name}")), name: name.into(), kind: file_kind(name, dir) };
        let ops_of = |items: &[ShelfItem]| ops(&items.iter().collect::<Vec<_>>());
        assert_eq!(ops_of(&[item("ekran.png", false)]), [Share, CopyText, Shrink], "one picture");
        assert_eq!(ops_of(&[item("a.png", false), item("b.JPG", false)]), [Share, Shrink, Zip], "pictures");
        assert_eq!(ops_of(&[item("a.png", false), item("rapor.pdf", false)]), [Share, Zip], "a picture and a document");
        assert_eq!(ops_of(&[item("rapor.pdf", false)]), [Share], "one document");
        assert_eq!(ops_of(&[item("logo.svg", false)]), [Share], "a drawing: no pixels to read or shrink");
        assert_eq!(ops_of(&[item("Fotoğraflar", true)]), [Zip], "a folder: zipped, not shared");
        assert_eq!(ops_of(&[item("Fotoğraflar", true), item("a.png", false)]), [Share, Zip], "the file among them is shared");
        assert_eq!(ops_of(&[]), Vec::<ShelfOp>::new());
    }

    #[test]
    fn file_kinds_by_extension() {
        use FileKind::*;
        let kind = |n: &str| file_kind(n, false);
        assert_eq!([kind("a.PNG"), kind("b.jpeg"), kind("c.webp"), kind("d.heic")], [Image; 4]);
        assert_eq!([kind("a.mp4"), kind("b.MKV"), kind("c.mov")], [Video; 3]);
        assert_eq!([kind("a.mp3"), kind("b.flac"), kind("c.wav")], [Audio; 3]);
        assert_eq!(kind("rapor.pdf"), Pdf);
        assert_eq!([kind("a.zip"), kind("b.rar"), kind("c.7z"), kind("d.tar.gz")], [Archive; 4]);
        assert_eq!([kind("a.docx"), kind("b.txt"), kind("c.xlsx"), kind("d.md")], [Document; 4]);
        assert_eq!([kind("main.rs"), kind("app.tsx"), kind("x.json"), kind("y.py")], [Code; 4]);
        assert_eq!([kind("Makefile"), kind("setup.exe"), kind(".gitignore")], [Other; 3]);
        assert_eq!(file_kind("Ekran görüntüleri", true), Folder);
        assert_eq!(file_kind("arsiv.zip", true), Folder, "a folder named like a file is still a folder");
    }

    #[test]
    fn extension_labels() {
        assert_eq!(extension_label("rapor.pdf").as_deref(), Some("PDF"));
        assert_eq!(extension_label("video.Mp4").as_deref(), Some("MP4"));
        assert_eq!(extension_label("Makefile"), None);
        assert_eq!(extension_label(".gitignore"), None, "a dot file has no extension");
        assert_eq!(extension_label("x.verylongext"), None, "longer than five letters says nothing");
    }

    #[test]
    fn items_carry_their_kind() {
        let mut s = Shelf::default();
        s.add(&files(&[r"C:\x\film.mkv", r"C:\x\Fotoğraflar"]), |p| p.ends_with("Fotoğraflar"));
        assert_eq!(s.items()[0].kind, FileKind::Folder);
        assert_eq!(s.items()[1].kind, FileKind::Video);
    }
}
