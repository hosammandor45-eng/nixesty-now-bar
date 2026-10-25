//! The shelf footer's tools on their own thread: a picture's text read by Windows' own
//! recogniser (Windows.Media.Ocr, offline), pictures shrunk to JPEGs (Windows' imaging), files
//! zipped (zip.rs). Nothing leaves the PC.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use windows::core::HSTRING;
use windows::Globalization::{ApplicationLanguages, Language};
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform, ColorManagementMode, ExifOrientationMode};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::{FileAccessMode, StorageFile};
use windows::Win32::Foundation::GENERIC_WRITE;
use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_WICPixelFormat24bppBGR, IWICImagingFactory, WICBitmapEncoderNoCache};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

use super::shelf::ShelfOp;
use super::shelf_tools::{flatten_on_white, join_lines, ocr_scale, pick_recognizer, shrink_size, unique_path, SHRINK_SIDE};
use super::zip::{write_zip, TOO_BIG};
use crate::platform::log::log;

pub enum Job {
    /// The text of this picture.
    Read(PathBuf),
    /// Pictures made smaller: each picture and where its JPEG goes.
    Shrink(Vec<(PathBuf, PathBuf)>),
    /// One zip of `files` at `out`.
    Zip { files: Vec<PathBuf>, out: PathBuf },
}

/// Why a tool gave nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Windows has no text recogniser at all.
    NoRecognizer,
    /// The file isn't a picture Windows opens.
    Unreadable,
    /// Nothing could be written, beside the files or in the Now Bar's own folder.
    Unwritable,
    /// Past a zip's 4 GB.
    TooBig,
}

pub enum Done {
    /// A picture's text (empty: none found), and the name of the user's language Windows had no
    /// recogniser for, when another read it.
    Text { text: String, missing: Option<String> },
    /// What a tool made (the shrunk pictures, the zip), and how many pictures came out no smaller.
    Made { op: ShelfOp, files: Vec<PathBuf>, not_smaller: usize },
    Failed { op: ShelfOp, why: Why },
}

pub type ToolMailbox = Arc<Mutex<Vec<Done>>>;

/// Where a tool's file goes when the source's folder takes none (`%LOCALAPPDATA%\NowBar\Raf`).
pub fn made_dir() -> PathBuf {
    crate::platform::log::log_path().with_file_name("Raf")
}

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
                    Job::Shrink(pictures) => shrink_all(&pictures),
                    Job::Zip { files, out } => match zip(&files, &out) {
                        Ok(made) => Done::Made { op: ShelfOp::Zip, files: vec![made], not_smaller: 0 },
                        Err(why) => Done::Failed { op: ShelfOp::Zip, why },
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

/// The picture's decoder, from its file.
fn decoder_of(path: &Path) -> windows::core::Result<BitmapDecoder> {
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))?.join()?;
    let stream = file.OpenAsync(FileAccessMode::Read)?.join()?;
    BitmapDecoder::CreateAsync(&stream)?.join()
}

/// The picture's lines, scaled to what reads best (small text twice as big).
fn read_text(path: &Path) -> Result<(String, Option<String>), Why> {
    let (engine, missing) = recognizer()?;
    let open = || -> windows::core::Result<Vec<String>> {
        let decoder = decoder_of(path)?;
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
            log(&format!("shelf tools: can't read a picture ({e})"));
            Err(Why::Unreadable)
        }
    }
}

/// Each picture shrunk: beside it, or in the Now Bar's folder when its own takes nothing.
fn shrink_all(pictures: &[(PathBuf, PathBuf)]) -> Done {
    let (mut files, mut not_smaller, mut failed) = (Vec::new(), 0, None);
    for (src, dest) in pictures {
        let fallback = || unique_path(&made_dir(), &dest.file_name().unwrap_or_default().to_string_lossy(), Path::exists);
        let made = shrink(src, dest).or_else(|e| {
            log(&format!("shelf tools: can't write beside the picture ({e}), into the Now Bar's folder"));
            let _ = std::fs::create_dir_all(made_dir());
            shrink(src, &fallback())
        });
        match made {
            Ok(Some(path)) => files.push(path),
            Ok(None) => not_smaller += 1,
            Err(e) => {
                log(&format!("shelf tools: can't shrink a picture ({e})"));
                failed = Some(if e.code() == windows::Win32::Foundation::E_ACCESSDENIED { Why::Unwritable } else { Why::Unreadable });
            }
        }
    }
    match failed {
        Some(why) if files.is_empty() && not_smaller == 0 => Done::Failed { op: ShelfOp::Shrink, why },
        _ => Done::Made { op: ShelfOp::Shrink, files, not_smaller },
    }
}

/// `src` as a JPEG at `dest`, its long side at most 1920 and transparency on white; `None` (and
/// nothing left behind) when that's no smaller than the picture.
fn shrink(src: &Path, dest: &Path) -> windows::core::Result<Option<PathBuf>> {
    let decoder = decoder_of(src)?;
    let (pw, ph) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
    let (ow, oh) = (decoder.OrientedPixelWidth()?, decoder.OrientedPixelHeight()?);
    let (w, h) = shrink_size(ow, oh, SHRINK_SIDE);
    // The transform scales the stored picture, then turns it upright.
    let turned = ow != pw || oh != ph;
    let transform = BitmapTransform::new()?;
    transform.SetScaledWidth(if turned { h } else { w })?;
    transform.SetScaledHeight(if turned { w } else { h })?;
    transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    let pixels = decoder
        .GetPixelDataTransformedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied, &transform, ExifOrientationMode::RespectExifOrientation, ColorManagementMode::ColorManageToSRgb)?
        .join()?
        .DetachPixelData()?;
    if pixels.len() != (w * h * 4) as usize {
        return Err(windows::Win32::Foundation::E_UNEXPECTED.into());
    }
    let written = unsafe { write_jpeg(dest, w, h, &flatten_on_white(&pixels)) };
    if let Err(e) = written {
        let _ = std::fs::remove_file(dest);
        return Err(e);
    }
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    if size(dest) >= size(src) {
        let _ = std::fs::remove_file(dest);
        return Ok(None);
    }
    Ok(Some(dest.to_path_buf()))
}

/// BGR pixels as a JPEG at Windows' default quality (0.9).
unsafe fn write_jpeg(path: &Path, w: u32, h: u32, bgr: &[u8]) -> windows::core::Result<()> {
    unsafe {
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = wic.CreateStream()?;
        stream.InitializeFromFilename(&HSTRING::from(path.as_os_str()), GENERIC_WRITE.0)?;
        let encoder = wic.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame = frame.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        frame.Initialize(None)?;
        frame.SetSize(w, h)?;
        let mut format = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut format)?;
        frame.WritePixels(h, w * 3, bgr)?;
        frame.Commit()?;
        encoder.Commit()
    }
}

/// `files` zipped at `out`, or in the Now Bar's folder when `out`'s takes nothing.
fn zip(files: &[PathBuf], out: &Path) -> Result<PathBuf, Why> {
    let write = |at: &Path| {
        let written = write_zip(at, files, crate::app::tz_offset_s());
        if let Err(e) = &written {
            // The error's kind only: its words may carry a file's name.
            log(&format!("shelf tools: can't zip ({:?})", e.kind()));
            let _ = std::fs::remove_file(at);
        }
        written
    };
    match write(out) {
        Ok(()) => return Ok(out.to_path_buf()),
        Err(e) if e.to_string() == TOO_BIG => return Err(Why::TooBig),
        Err(_) => {}
    }
    let _ = std::fs::create_dir_all(made_dir());
    let fallback = unique_path(&made_dir(), &out.file_name().unwrap_or_default().to_string_lossy(), Path::exists);
    write(&fallback).map(|()| fallback).map_err(|_| Why::Unwritable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::crest::write_test_png;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nowbar-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_big_picture_shrinks_to_a_smaller_jpeg() {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let d = dir("shrink");
        let src = d.join("büyük.png");
        // Noise doesn't compress as a PNG: the JPEG is surely smaller.
        write_test_png(&src, 2400, 1200, |x, y| {
            let mut z = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            z ^= z >> 31;
            z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z ^= z >> 29;
            [z as u8, (z >> 8) as u8, (z >> 16) as u8, 255]
        });
        let dest = d.join("büyük (küçük).jpg");
        let made = shrink(&src, &dest).expect("shrunk").expect("smaller");
        assert_eq!(made, dest);
        let decoder = decoder_of(&dest).unwrap();
        assert_eq!((decoder.PixelWidth().unwrap(), decoder.PixelHeight().unwrap()), (1920, 960));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_small_picture_is_left_alone() {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let d = dir("small");
        let src = d.join("ikon.png");
        write_test_png(&src, 16, 16, |_, _| [255, 0, 0, 255]);
        let dest = d.join("ikon (küçük).jpg");
        assert_eq!(shrink(&src, &dest).expect("read"), None, "a 16 px PNG is smaller than any JPEG");
        assert!(!dest.exists(), "nothing left behind");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn files_and_a_folder_zip_by_name() {
        let d = dir("zip");
        std::fs::write(d.join("şarkı listesi.txt"), "bir").unwrap();
        std::fs::create_dir_all(d.join("Klasör")).unwrap();
        std::fs::write(d.join("Klasör").join("iç.txt"), "iki").unwrap();
        let out = d.join("şarkı listesi ve 1 dosya.zip");
        assert_eq!(zip(&[d.join("şarkı listesi.txt"), d.join("Klasör")], &out), Ok(out.clone()));
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[..4], b"PK\x03\x04", "a zip");
        let has = |name: &str| bytes.windows(name.len()).any(|w| w == name.as_bytes());
        assert!(has("şarkı listesi.txt") && has("Klasör/iç.txt"), "by name, in UTF-8");
        assert!(!has("nowbar-tools"), "not by the path they came from");
        let _ = std::fs::remove_dir_all(&d);
    }

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
