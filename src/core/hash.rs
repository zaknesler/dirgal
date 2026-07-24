use crate::error::AppResult;
use seahash::SeaHasher;
use std::hash::Hasher;
use std::io::Read;
use std::path::Path;

/// Number of bytes to read from a file to compute its hash
const MAX_READ_BYTES: usize = 8192;

/// Hash the path of the given file
pub fn hash_path(path: &Path) -> u64 {
    let mut hasher = SeaHasher::new();
    hasher.write(path.as_os_str().as_encoded_bytes());
    hasher.finish()
}

/// Hash the content of the given file, up to `MAX_READ_BYTES` bytes
pub fn hash_content(path: &Path) -> AppResult<u64> {
    let mut file = std::fs::File::open(path)?;
    let file_len = std::fs::metadata(path)?.len();

    let mut buf = [0u8; MAX_READ_BYTES];
    let mut total_read = 0;

    // Continue reading in the file until EOF or the buffer is full
    while total_read < buf.len() {
        match file.read(&mut buf[total_read..])? {
            0 => break, // EOF
            n => total_read += n,
        }
    }

    let mut hasher = SeaHasher::new();
    hasher.write_u64(file_len);
    hasher.write(&buf[..total_read]);

    Ok(hasher.finish())
}
