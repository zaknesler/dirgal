use gpui::{Action, actions};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

actions!([
    Quit,
    Prev,
    Next,
    CollapseAll,
    OpenLightbox,
    CloseLightbox,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    PrevPage,
    NextPage,
    FocusSearch,
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
