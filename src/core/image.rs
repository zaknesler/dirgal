use crate::core::path::compare_paths;
use crate::error::AppResult;
use crate::ui::model::{Sort, SortKey};
use gpui::Img;
use humansize::{BINARY, FormatSizeOptions, format_size};
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::{
    cmp::Ordering,
    collections::HashSet,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

pub const THUMB_PX: u32 = 336;

/// Number of bytes a source image must be to not warrant a thumbnail (32 KB)
pub const SMALL_FILE_BYTES: u64 = 32 * 1024;

/// Number of bytes read from the head of a file to get the EXIF data (64 KB)
const EXIF_READ_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ImageId(Arc<Path>);

impl ImageId {
    pub fn new(path: PathBuf) -> Self {
        Self(Arc::from(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn clone_path(&self) -> Arc<Path> {
        self.0.clone()
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.0.to_path_buf()
    }
}

impl AsRef<Path> for ImageId {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

/// Hash identifying image content, shared by duplicate files
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, serde::Deserialize, schemars::JsonSchema)]
pub struct ContentHash(pub u64);

#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub id: ImageId,
    pub content_hash: ContentHash,
    pub bytes: u64,
    #[allow(unused)]
    pub modified: Option<SystemTime>,
    #[allow(unused)]
    pub created: Option<SystemTime>,
    pub thumb_path: Arc<Path>,
    pub thumb_exists: bool,
    pub dimensions: Option<(u32, u32)>,
}

pub struct FoundFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
}

impl ImageEntry {
    pub fn new(
        file: FoundFile,
        thumb_dir: &Path,
        content_hash: ContentHash,
        dimensions: Option<(u32, u32)>,
    ) -> Self {
        let thumb = thumb_dir.join(format!("{:016x}.png", content_hash.0));
        let thumb_exists = thumb.exists();

        Self {
            id: ImageId::new(file.path),
            content_hash,
            bytes: file.bytes,
            modified: file.modified,
            created: file.created,
            thumb_path: Arc::from(thumb),
            thumb_exists,
            dimensions,
        }
    }

    /// Generate and save the thumbnail in the thumbnail directory
    pub fn generate_thumbnail(&self) -> AppResult<()> {
        let src = self.id.path();
        let dst = &self.thumb_path;

        if dst.exists() {
            return Ok(());
        }

        let image = image::ImageReader::open(src)?
            .with_guessed_format()?
            .decode()?;
        let image = apply_exif_orientation(image, orientation(src));

        let image_already_small = image.width() <= THUMB_PX && image.height() <= THUMB_PX;
        let thumb = if image_already_small {
            image
        } else {
            image.thumbnail(THUMB_PX, THUMB_PX)
        };

        let tmp = dst.with_extension("tmp");
        thumb.save_with_format(&tmp, image::ImageFormat::Png)?;
        fs::rename(&tmp, dst)?;
        Ok(())
    }
}

/// Read an image's displayed pixel dimensions without decoding it
pub fn read_dimensions(path: &Path) -> Option<(u32, u32)> {
    let (width, height) = image::image_dimensions(path).ok()?;

    // Orientations 5-8 rotate by 90 deg, so the sides need to be swapped
    // Holy fuck EXIF is weird: https://en.wikipedia.org/wiki/Exif#Exif_fields
    Some(match orientation(path) {
        5..=8 => (height, width),
        _ => (width, height),
    })
}

/// Read the EXIF orientation tag from the head of the given file
fn orientation(path: &Path) -> u32 {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 1,
    };

    // Read into a buffer first; given the file, the reader scans all of it when there is no EXIF
    let mut buf = Vec::new();
    if file.take(EXIF_READ_BYTES).read_to_end(&mut buf).is_err() {
        return 1;
    }

    // Create a cursor so it can read from the buffer without consuming it
    let mut cursor = std::io::Cursor::new(&buf);
    let exif = match exif::Reader::new().read_from_container(&mut cursor) {
        Ok(e) => e,
        Err(_) => return 1,
    };

    // Read the orientation tag from the EXIF data, defaulting to 1 (no rotation)
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .unwrap_or(1)
}

/// Rotate/flip an image so its pixels match the EXIF orientation tag's intended display
fn apply_exif_orientation(image: image::DynamicImage, orientation: u32) -> image::DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

/// Construct a walker that will recursively walk the given root directory
pub fn build_root_walker(root: &Path) -> ignore::Walk {
    WalkBuilder::new(root).build()
}

/// Check whether the given path is an image file
pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| Img::extensions().contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Get the stats of the given entry (size, modified, created)
pub fn entry_stats(entry: &ignore::DirEntry) -> (u64, Option<SystemTime>, Option<SystemTime>) {
    entry
        .metadata()
        .ok()
        .map(|m| (m.len(), m.modified().ok(), m.created().ok()))
        .unwrap_or((0, None, None))
}

/// Format the given number of bytes as a human-readable string
pub fn format_bytes(bytes: u64) -> String {
    format_size(bytes, FormatSizeOptions::from(BINARY).decimal_places(1))
}

/// Deduplicate by content hash keeping the last, then sort by the active sort key
pub fn deduplicate_and_sort(
    images: Vec<ImageEntry>,
    sort: Sort,
) -> (Vec<ImageEntry>, Vec<ImageEntry>) {
    // Make a map of image content hashes to images to group duplicates together
    let mut group: HashMap<ContentHash, Vec<ImageEntry>> = HashMap::new();
    for image in images {
        group.entry(image.content_hash).or_default().push(image);
    }

    let mut unique: Vec<ImageEntry> = Vec::new();
    let mut duplicate_groups: Vec<Vec<ImageEntry>> = Vec::new();

    // Sort each group and collect unique images
    for mut group in group.into_values() {
        unique.push(group.last().expect("group should not be empty").clone());

        // Keep duplicate images sorted next to each other
        if group.len() > 1 {
            group.sort_by(|a, b| compare_key(a, b, sort));
            duplicate_groups.push(group);
        }
    }

    // Sort unique images and duplicate groups
    unique.sort_by(|a, b| compare_key(a, b, sort));
    duplicate_groups.sort_by(|a, b| compare_key(&a[0], &b[0], sort));

    let duplicates = duplicate_groups.into_iter().flatten().collect();

    (unique, duplicates)
}

/// Compare by parent directory alone so same directory images stay contiguous
pub fn compare_parents(a: &ImageEntry, b: &ImageEntry) -> Ordering {
    let parent_a = a.id.path().parent().unwrap_or(Path::new(""));
    let parent_b = b.id.path().parent().unwrap_or(Path::new(""));
    compare_paths(parent_a, parent_b)
}

/// Compare two images by the active sort key falling back to path for a stable order
pub fn compare_key(a: &ImageEntry, b: &ImageEntry, sort: Sort) -> Ordering {
    if sort.key == SortKey::DateInPath {
        return compare_date_in_path(a, b, sort.ascending);
    }

    // First compare based on the sort key, but always fall back to compare_paths
    let ord = match sort.key {
        SortKey::Name => compare_paths(a.id.path(), b.id.path()),
        SortKey::Modified => a
            .modified
            .cmp(&b.modified)
            .then_with(|| compare_paths(a.id.path(), b.id.path())),
        SortKey::Created => a
            .created
            .cmp(&b.created)
            .then_with(|| compare_paths(a.id.path(), b.id.path())),
        SortKey::Size => a
            .bytes
            .cmp(&b.bytes)
            .then_with(|| compare_paths(a.id.path(), b.id.path())),
        SortKey::Resolution => {
            let area_a = a.dimensions.map(|(w, h)| w as u64 * h as u64);
            let area_b = b.dimensions.map(|(w, h)| w as u64 * h as u64);
            area_a
                .cmp(&area_b)
                .then_with(|| compare_paths(a.id.path(), b.id.path()))
        }
        SortKey::DateInPath => unreachable!("handled above"),
    };

    if sort.ascending { ord } else { ord.reverse() }
}

/// Compare by embedded path date, keeping dateless images at the end regardless of direction
fn compare_date_in_path(a: &ImageEntry, b: &ImageEntry, ascending: bool) -> Ordering {
    match (
        crate::core::path::extract_date_from_path(a.id.path()),
        crate::core::path::extract_date_from_path(b.id.path()),
    ) {
        (None, None) => compare_paths(a.id.path(), b.id.path()),
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(da), Some(db)) => {
            let ord = da
                .cmp(&db)
                .then_with(|| compare_paths(a.id.path(), b.id.path()));
            if ascending { ord } else { ord.reverse() }
        }
    }
}

/// Resolve configured bookmark hashes against loaded images, dropping unknowns
pub fn resolve_bookmarks(hashes: &[u64], images: &[ImageEntry]) -> Vec<ContentHash> {
    let known = hashes.iter().copied().collect::<HashSet<u64>>();

    images
        .iter()
        .filter(|e| known.contains(&e.content_hash.0))
        .map(|e| e.content_hash)
        .collect()
}
