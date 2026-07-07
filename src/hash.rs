use seahash::SeaHasher;
use std::hash::Hasher;
use std::path::Path;

pub fn hash_path_mtime(path: &Path, mtime: u64) -> u64 {
    let mut h = SeaHasher::new();
    h.write(path.as_os_str().as_encoded_bytes());
    h.write_u64(mtime);
    h.finish()
}

pub fn hash_path(path: &Path) -> u64 {
    let mut h = SeaHasher::new();
    h.write(path.as_os_str().as_encoded_bytes());
    h.finish()
}
