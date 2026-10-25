//! Windows' share sheet for the shelf's files: Quick Share, Nearby Sharing, mail and the apps
//! that take files are Windows' own to offer.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use windows::core::{factory, AgileReference, Interface, Result, HSTRING};
use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
use windows::Foundation::TypedEventHandler;
use windows::Storage::{IStorageItem, StorageFile};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::IDataTransferManagerInterop;
use windows_collections::IIterable;

use super::log::log;

thread_local! {
    /// The window's manager and the handler of the last share: one at a time, the next replaces it.
    static LAST: RefCell<Option<(DataTransferManager, i64)>> = const { RefCell::new(None) };
}

/// Files looked up for the sheet, and what names them in it ("ekran.png", "3 dosya").
pub struct Prepared {
    list: AgileReference<IIterable<IStorageItem>>,
    title: String,
}

/// What `prepare` found: the files, or nothing to share (a file that's gone or won't open).
pub type ShareMailbox = Arc<Mutex<Option<Option<Prepared>>>>;

/// Looks the files up on a thread of its own (on a slow network share that takes a while), then
/// leaves them in `mailbox` and calls `post`.
pub fn prepare(paths: Vec<PathBuf>, title: String, mailbox: ShareMailbox, post: fn()) {
    let spawned = std::thread::Builder::new().name("share".into()).spawn(move || {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let found = || -> Result<Prepared> {
            let mut items: Vec<Option<IStorageItem>> = Vec::new();
            for p in &paths {
                let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(p.as_os_str()))?.join()?;
                items.push(Some(file.cast()?));
            }
            // The sheet's request may come on another thread: the list goes as an agile reference.
            Ok(Prepared { list: AgileReference::new(&IIterable::from(items))?, title })
        };
        let prepared = found().inspect_err(|e| log(&format!("share: files not found ({e})"))).ok();
        if let Ok(mut m) = mailbox.lock() {
            *m = Some(prepared);
        }
        post();
    });
    if let Err(e) = spawned {
        log(&format!("share: no thread ({e})"));
    }
}

/// Opens the share sheet over `hwnd` with what `prepare` found.
pub fn show(hwnd: HWND, p: Prepared) -> Result<()> {
    let interop = factory::<DataTransferManager, IDataTransferManagerInterop>()?;
    let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
    let (list, title) = (p.list, HSTRING::from(p.title));
    let token = manager.DataRequested(&TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(move |_, args| {
        let data = args.ok()?.Request()?.Data()?;
        // Without a title the sheet says the app has nothing to share.
        data.Properties()?.SetTitle(&title)?;
        data.SetStorageItems(&list.resolve()?, true)
    }))?;
    LAST.with(|last| {
        if let Some((old, token)) = last.borrow_mut().replace((manager, token)) {
            let _ = old.RemoveDataRequested(token);
        }
    });
    unsafe { interop.ShowShareUIForWindow(hwnd) }
}
