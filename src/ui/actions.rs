use crate::ui::{CONTEXT_GALLERY, CONTEXT_GALLERY_UNFOCUSED, actions};
use gpui::{Action, App, KeyBinding, actions};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

actions!([
    Quit,
    Minimize,
    Refresh,
    Up,
    Down,
    Left,
    Right,
    CollapseAll,
    OpenLightbox,
    CloseLightbox,
    ToggleView,
    ToggleThumbnailFit,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomFill,
    PrevPage,
    NextPage,
    FocusSearch,
    JumpToTop,
    JumpToBottom,
]);

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum CopyImage {
    Current,
    Path(PathBuf),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum TrashFile {
    Current,
    Path(PathBuf),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum DeleteFile {
    Current,
    Path(PathBuf),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum OpenInFinder {
    Current,
    Path(PathBuf),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum Bookmark {
    Current,
    Hash(crate::core::image::ContentHash),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum CopyPathToClipboard {
    Current,
    Path(PathBuf),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub struct RevealInGallery(pub crate::core::image::ContentHash);

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub struct ApplyPreset(pub u32);

/// Register keybinds and actions to the app
pub fn register_actions(cx: &mut App) {
    macro_rules! bind_keys {
         ($context:expr, $(($key:expr, $action:expr)),* $(,)?) => {
             cx.bind_keys([$( KeyBinding::new($key, $action, $context) ),*]);
         };
    }

    cx.on_action(|_: &actions::Quit, cx| cx.quit());

    // Global
    bind_keys!(
        None,
        ("secondary-q", actions::Quit),
        ("ctrl-shift-w", actions::Quit),
        ("cmd-m", actions::Minimize)
    );

    // Gallery
    bind_keys!(
        Some(CONTEXT_GALLERY),
        ("ctrl-tab", actions::NextPage),
        ("ctrl-shift-tab", actions::PrevPage),
        ("escape", actions::CloseLightbox),
        ("secondary-k", actions::FocusSearch),
        ("secondary-r", actions::Refresh),
        ("secondary-0", actions::ApplyPreset(0)),
        ("secondary-1", actions::ApplyPreset(1)),
        ("secondary-2", actions::ApplyPreset(2)),
        ("secondary-3", actions::ApplyPreset(3)),
        ("secondary-4", actions::ApplyPreset(4)),
        ("secondary-5", actions::ApplyPreset(5)),
        ("secondary-6", actions::ApplyPreset(6)),
        ("secondary-7", actions::ApplyPreset(7)),
        ("secondary-8", actions::ApplyPreset(8)),
        ("secondary-9", actions::ApplyPreset(9)),
    );

    // Gallery (unfocused)
    bind_keys!(
        Some(CONTEXT_GALLERY_UNFOCUSED),
        ("up", actions::Up),
        ("down", actions::Down),
        ("left", actions::Left),
        ("right", actions::Right),
        ("secondary-up", actions::JumpToTop),
        ("secondary-down", actions::JumpToBottom),
        ("pageup", actions::JumpToTop),
        ("pagedown", actions::JumpToBottom),
        ("space", actions::OpenLightbox),
        ("enter", actions::OpenLightbox),
        ("backspace", actions::TrashFile::Current),
        ("secondary-backspace", actions::DeleteFile::Current),
        ("v", actions::ToggleView),
        ("g", actions::ToggleThumbnailFit),
        ("b", actions::Bookmark::Current),
        ("k", actions::CopyPathToClipboard::Current),
        ("o", actions::OpenInFinder::Current),
        ("[", actions::ZoomOut),
        ("]", actions::ZoomIn),
        ("0", actions::ZoomReset),
        ("1", actions::ZoomFill),
        ("c", actions::CollapseAll),
    );
}
