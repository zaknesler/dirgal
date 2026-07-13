use seahash::SeaHasher;
use std::hash::Hasher;
use std::io::Read;
use std::path::Path;

use crate::error::AppResult;

pub fn hash_path(path: &Path) -> u64 {
    let mut hasher = SeaHasher::new();
    hasher.write(path.as_os_str().as_encoded_bytes());
    hasher.finish()
}

pub fn hash_content(path: &Path) -> AppResult<u64> {
    let mut file = std::fs::File::open(path)?;
    let file_len = std::fs::metadata(path)?.len();

    let mut buf = [0u8; 8192];
    let bytes_read = file.read(&mut buf)?;

    let mut hasher = SeaHasher::new();
    hasher.write_u64(file_len);
    hasher.write(&buf[..bytes_read]);

    Ok(hasher.finish())
}
