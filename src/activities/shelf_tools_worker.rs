//! The shelf footer's tools on their own thread: a picture's text read by Windows' own
//! recogniser (Windows.Media.Ocr, offline). Nothing leaves the PC.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use windows::core::HSTRING;
use windows::Globalization::{ApplicationLanguages, Language};
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform, ColorManagementMode, ExifOrientationMode};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::{FileAccessMode, StorageFile};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use super::shelf::ShelfOp;
use super::shelf_tools::{join_lines, ocr_scale, pick_recognizer};
use crate::platform::log::log;

pub enum Job {
    /// The text of this picture.
    Read(PathBuf),
}

/// Why a tool gave nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Windows has no text recogniser at all.
    NoRecognizer,
    /// The file isn't a picture Windows opens.
    Unreadable,
}

pub enum Done {
    /// A picture's text (empty: none found), and the name of the user's language Windows had no
    /// recogniser for, when another read it.
    Text { text: String, missing: Option<String> },
    Failed { op: ShelfOp, why: Why },
}

pub type ToolMailbox = Arc<Mutex<Vec<Done>>>;

pub struct ToolWorker {
    tx: Sender<Job>,
}

impl ToolWorker {
    /// `post` runs on the worker thread after each result (the mailbox then holds it).
    pub fn start(mailbox: ToolMailbox, post: fn()) -> Self {
        let (tx, rx) = channel::<Job>();
        let spawned = std::thread::Builder::new().name("shelf-tools".into()).spawn(move || {
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            for job in rx {
                let done = match job {
                    Job::Read(path) => match read_text(&path) {
                        Ok((text, missing)) => Done::Text { text, missing },
                        Err(why) => Done::Failed { op: ShelfOp::CopyText, why },
                    },
                };
                if let Ok(mut m) = mailbox.lock() {
                    m.push(done);
                }
                post();
            }
        });
        if let Err(e) = spawned {
            log(&format!("shelf tools: no thread ({e})"));
        }
        ToolWorker { tx }
    }

    pub fn send(&self, job: Job) {
        let _ = self.tx.send(job);
    }
}

/// The recogniser for the user's languages (or any Windows has), and the name of the user's first
/// language when it had none.
fn recognizer() -> Result<(OcrEngine, Option<String>), Why> {
    let available: Vec<String> = OcrEngine::AvailableRecognizerLanguages()
        .map(|list| list.into_iter().filter_map(|l| l.LanguageTag().ok()).map(|t| t.to_string()).collect())
        .unwrap_or_default();
    let preferred: Vec<String> = ApplicationLanguages::Languages().map(|list| list.into_iter().map(|t| t.to_string()).collect()).unwrap_or_default();
    let (tag, missing) = pick_recognizer(&available, &preferred).ok_or(Why::NoRecognizer)?;
    let engine = Language::CreateLanguage(&HSTRING::from(&tag)).and_then(|l| OcrEngine::TryCreateFromLanguage(&l)).map_err(|_| Why::NoRecognizer)?;
    let missing = missing.then(|| preferred.first().and_then(|p| Language::CreateLanguage(&HSTRING::from(p)).and_then(|l| l.DisplayName()).ok())).flatten();
    log(&format!("shelf tools: reading with {tag} (of {available:?}, wanted {preferred:?})"));
    Ok((engine, missing.map(|n| n.to_string())))
}

/// The picture's lines, scaled to what reads best (small text twice as big).
fn read_text(path: &Path) -> Result<(String, Option<String>), Why> {
    let (engine, missing) = recognizer()?;
    let open = || -> windows::core::Result<Vec<String>> {
        let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))?.join()?;
        let stream = file.OpenAsync(FileAccessMode::Read)?.join()?;
        let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
        // The transform scales before it turns: the stored width and height.
        let (w, h) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
        let scale = ocr_scale(w, h, OcrEngine::MaxImageDimension()?);
        let transform = BitmapTransform::new()?;
        transform.SetScaledWidth(((w as f32 * scale).floor() as u32).max(1))?;
        transform.SetScaledHeight(((h as f32 * scale).floor() as u32).max(1))?;
        transform.SetInterpolationMode(if scale > 1.0 { BitmapInterpolationMode::Cubic } else { BitmapInterpolationMode::Fant })?;
        let bitmap = decoder
            .GetSoftwareBitmapTransformedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied, &transform, ExifOrientationMode::RespectExifOrientation, ColorManagementMode::DoNotColorManage)?
            .join()?;
        let result = engine.RecognizeAsync(&bitmap)?.join()?;
        let lines = result.Lines()?;
        (0..lines.Size()?).map(|i| lines.GetAt(i)?.Text().map(|t| t.to_string())).collect()
    };
    match open() {
        Ok(lines) => Ok((join_lines(&lines), missing)),
        Err(e) => {
            log(&format!("shelf tools: can't read {} ({e})", path.display()));
            Err(Why::Unreadable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::crest::write_test_png;

    /// Windows' recogniser on `NOWBAR_OCR_FILE` (a sheet's PNG), or on made-up bars:
    /// `cargo t --lib live_ocr -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_ocr() {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let path = match std::env::var_os("NOWBAR_OCR_FILE") {
            Some(p) => PathBuf::from(p),
            None => {
                let path = std::env::temp_dir().join("nowbar-ocr-test.png");
                // Black bars on white: no text, but the whole road from the file to the recogniser.
                write_test_png(&path, 400, 120, |x, _| if (x / 20) % 2 == 0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] });
                path
            }
        };
        match read_text(&path) {
            Ok((text, missing)) => println!("missing {missing:?}\n{text}"),
            Err(why) => println!("{why:?}"),
        }
    }
}
