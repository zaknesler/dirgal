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
    SwitchView,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    PrevPage,
    NextPage,
    FocusSearch,
    JumpToTop,
    JumpToBottom,
]);

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum OpenInFinder {
    Current,
    Path(PathBuf),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum Bookmark {
    Current,
    Thumb(super::model::ImageHash),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub enum CopyPathToClipboard {
    Current,
    Thumb(super::model::ImageHash),
}

#[derive(Clone, PartialEq, Eq, Action, Deserialize, JsonSchema)]
pub struct RevealInGallery(pub super::model::ImageHash);

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
        ("v", actions::SwitchView),
        ("b", actions::Bookmark::Current),
        ("k", actions::CopyPathToClipboard::Current),
        ("o", actions::OpenInFinder::Current),
        ("=", actions::ZoomIn),
        ("-", actions::ZoomOut),
        ("0", actions::ZoomReset),
        ("c", actions::CollapseAll),
    );
}
