//! Windows' share sheet for the shelf's files: Quick Share, Nearby Sharing, mail and the apps
//! that take files are Windows' own to offer.

use std::cell::RefCell;
use std::path::PathBuf;

use windows::core::{factory, AgileReference, Interface, Result, HSTRING};
use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
use windows::Foundation::TypedEventHandler;
use windows::Storage::{IStorageItem, StorageFile};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::IDataTransferManagerInterop;
use windows_collections::IIterable;

thread_local! {
    /// The window's manager and the handler of the last share: one at a time, the next replaces it.
    static LAST: RefCell<Option<(DataTransferManager, i64)>> = const { RefCell::new(None) };
}

/// Opens the share sheet over `hwnd` with `paths` (the files that open; a folder is not shared).
/// `title` names them in the sheet ("ekran.png", "3 dosya").
pub fn share_files(hwnd: HWND, paths: &[PathBuf], title: &str) -> Result<()> {
    // The files are looked up before the sheet asks for them: its request has no time to wait.
    let mut items: Vec<Option<IStorageItem>> = Vec::new();
    for p in paths {
        let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(p.as_os_str()))?.join()?;
        items.push(Some(file.cast()?));
    }
    // The handler may run on another thread: the list goes to it as an agile reference.
    let list = AgileReference::new(&IIterable::from(items))?;
    let interop = factory::<DataTransferManager, IDataTransferManagerInterop>()?;
    let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
    let title = HSTRING::from(title);
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
