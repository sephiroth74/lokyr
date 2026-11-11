//! The save panel, a sheet on a window, which tells the path that the user
//! chose, or none.

use std::path::{Path, PathBuf};
use std::rc::Weak;

use block2::RcBlock;
use lokyr_platform_core::{AppEvent, SavePanelId};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSModalResponse, NSModalResponseOK, NSSavePanel, NSWindow};
use objc2_foundation::{NSArray, NSString};
use objc2_uniform_type_identifiers::UTType;

use crate::shared::{Pending, Shared};

/// Opens the save panel on `window`, of `name` suggested and of its
/// extension's type, whose end posts [`AppEvent::SavePanelClosed`] of
/// `id`.
pub(crate) fn begin(
    mtm: MainThreadMarker,
    window: &NSWindow,
    name: &str,
    shared: Weak<Shared>,
    id: SavePanelId,
) {
    let panel = NSSavePanel::savePanel(mtm);
    panel.setNameFieldStringValue(&NSString::from_str(name));
    panel.setCanCreateDirectories(true);
    // The file's type of its extension, which the panel keeps on the name
    // that the user types.
    let kind = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(|extension| UTType::typeWithFilenameExtension(&NSString::from_str(extension)));
    if let Some(kind) = kind {
        panel.setAllowedContentTypes(&NSArray::from_retained_slice(&[kind]));
    }
    let shown = panel.clone();
    let completion = RcBlock::new(move |response: NSModalResponse| {
        let path = (response == NSModalResponseOK)
            .then(|| shown.URL())
            .flatten()
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()));
        if let Some(shared) = shared.upgrade() {
            shared.post(Pending::App(AppEvent::SavePanelClosed { panel: id, path }));
        }
    });
    panel.beginSheetModalForWindow_completionHandler(window, &completion);
}
