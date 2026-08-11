pub const DEBUG: bool = false;

pub const MIN_COLS: usize = 1;
pub const MAX_COLS: usize = 20;

/// Minimum tile width in pixels before adding another column
pub const GRID_TILE_MIN: f32 = 200.0;
/// Spacing between tiles in pixels
pub const GRID_GAP: f32 = 6.0;
/// Horizontal padding on either side of the grid
pub const GRID_OUTER_MARGIN: f32 = 16.0;
/// Extra vertical space (pixels) above and below the viewport whose thumbnails are eagerly queued
pub const GRID_OVERDRAW: f32 = 600.0;

// Max images retained in the cache
pub const GRID_CACHE_ITEMS: usize = 300;
pub const LIGHTBOX_CACHE_ITEMS: usize = 10;

// Min/max zoom levels (it's annoying to have it zoom smaller than 1.0, but this may be configurable later)
pub const ZOOM_MIN: f32 = 1.0;
pub const ZOOM_MAX: f32 = 20.0;

/// Multiplier applied to the zoom level with each step in or out
pub const ZOOM_STEP: f32 = 1.25;
/// Zoom applied per pixel of a modifier-held scroll
pub const ZOOM_PER_PIXEL: f32 = 0.01;

pub const COLOR_ACCENT: u32 = 0xca3500;
pub const COLOR_ACCENT_HOVER: u32 = 0xfc713f;
pub const COLOR_BACKDROP: u32 = 0x0a0a0af0;

pub const TRUNCATE_STR: &str = "…";
