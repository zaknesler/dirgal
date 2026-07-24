<img width="100" src=".github/assets/logo.svg" />

**dirgal** _[directory gallery]_ is a fast, friendly image gallery you can open within a directory from your terminal.

Thanks to Zed's wonderful [GPUI](https://gpui.rs) library, _dirgal_ is nible, responsive, and cross-platform by default. It currently supports a handful of features such as bookmarks, group/grid/list views, sorting, basic duplicate detection, grid sizing, etc. with more features planned.

This app is a work-in-progress side project and is mainly for casual (read: not professional) use. It is intended for quickly browsing through images in a directory, and is not some replacement for an image cataloging tool like Capture One or Lightroom.

![dirgal screenshot](.github/assets/screenshot.png)

### Usage

1. Pre-release binaries are available for Windows, macOS, and Linux. Go to the latest result of the [Release action](https://github.com/zaknesler/dirgal/actions/workflows/release.yml) to download the latest build for your platform.

2. Install via Cargo:

   ```sh
   cargo install --git https://github.com/zaknesler/dirgal
   ```

   Run inside your terminal:

   ```sh
   # Scan all images (recursively) in the current directory and open a gallery window
   dirgal

   # Or pass in multiple roots...
   dirgal ~/Downloads ~/Pictures
   ```

### Ideas

Some of these are completely out-of-scope and unrealistic, but would be nice to have:

- ZOOM!
- Improved filtering/searching
- File actions (e.g. copy, rename, delete, etc.)
- Metadata info (including EXIF data)
- Better duplicate detection
- Better experience when selecting multiple items
- Stats (e.g. number of images, duplicate count, total size, num folders, etc.)
- More keyboard navigation
- `--no-cache` to... bypass the cache of course
- Improved duplicate experience (it currently hides duplicate images from the main view)
- Save the hash cache periodically during a scan, not just at the end, so a big scan over a slow drive (like my really slow NAS) doesn't lose all its progress if interrupted
- Watch mode?
- Image tagging?
- RAW images?
- Videos?
- Similar image detection?
- Batch renaming?
