use gpui::{Action, actions};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

actions!([
    Quit,
    Minimize,
    Prev,
    Next,
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
