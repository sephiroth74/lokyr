//! The open panel, a sheet on a window, of files of some extensions,
//! several chosen at once, which tells the files that the user chose, or
//! none.

use std::path::PathBuf;
use std::rc::Weak;

use block2::RcBlock;
use lokyr_platform_core::AppEvent;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSModalResponse, NSModalResponseOK, NSOpenPanel, NSWindow};
use objc2_foundation::{NSArray, NSString};
use objc2_uniform_type_identifiers::UTType;

use crate::shared::{Pending, Shared};

/// Opens the open panel on `window`, of the files of `extensions`, several
/// chosen at once, of `message` over it, whose end posts
/// [`AppEvent::FilesChosen`].
pub(crate) fn begin(
    mtm: MainThreadMarker,
    window: &NSWindow,
    (extensions, message): (&[&str], &str),
    shared: Weak<Shared>,
) {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setMessage(Some(&NSString::from_str(message)));
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(true);
    let kinds: Vec<_> = extensions
        .iter()
        .filter_map(|extension| UTType::typeWithFilenameExtension(&NSString::from_str(extension)))
        .collect();
    if !kinds.is_empty() {
        panel.setAllowedContentTypes(&NSArray::from_retained_slice(&kinds));
    }
    let shown = panel.clone();
    let completion = RcBlock::new(move |response: NSModalResponse| {
        let paths: Vec<PathBuf> = if response == NSModalResponseOK {
            let urls = shown.URLs();
            (0..urls.count())
                .filter_map(|index| urls.objectAtIndex(index).path())
                .map(|path| PathBuf::from(path.to_string()))
                .collect()
        } else {
            Vec::new()
        };
        if let Some(shared) = shared.upgrade() {
            shared.post(Pending::App(AppEvent::FilesChosen(paths)));
        }
    });
    panel.beginSheetModalForWindow_completionHandler(window, &completion);
}
