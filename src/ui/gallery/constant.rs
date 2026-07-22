/// Overlays source paths on tiles when enabled
pub const DEBUG: bool = false;

/// Minimum tile width in pixels before adding another column
pub const TILE_MIN: f32 = 200.0;
/// Spacing between tiles in pixels
pub const GRID_GAP: f32 = 6.0;
/// Total horizontal padding around the grid in pixels
pub const GRID_OUTER_MARGIN: f32 = 32.0;

/// Extra vertical space (pixels) above and below the viewport whose thumbnails are eagerly queued
pub const GRID_OVERDRAW: f32 = 600.0;

/// Max images retained in the grid's LRU image cache
pub const GRID_CACHE_ITEMS: usize = 300;
/// Max images retained in the lightbox's LRU image cache
pub const LIGHTBOX_CACHE_ITEMS: usize = 10;

pub const COLOR_ACCENT: u32 = 0xca3500;
pub const COLOR_ACCENT_HOVER: u32 = 0xfc713f;
pub const COLOR_BACKDROP: u32 = 0x0a0a0af0;
