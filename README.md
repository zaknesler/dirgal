<img width="100" src=".github/assets/logo.svg" />

**dirgal** _[directory gallery]_ is a fast, friendly image gallery you can open within a directory from your terminal.

Thanks to Zed's wonderful [GPUI](https://gpui.rs) library, _dirgal_ is nible, responsive, and cross-platform by default. It currently supports a handful of features such as bookmarks, group/grid/list views, sorting, basic duplicate detection, grid sizing, etc. with more features planned.

This app is a work-in-progress side project and is mainly for casual (read: not professional) use. It is intended for quickly browsing through images in a directory, and is not some replacement for an image cataloging tool like Capture One or Lightroom.

![dirgal screenshot](.github/assets/screenshot.png)

### Installation

Pre-release binaries are available for Windows, macOS, and Linux.

Go to the latest result of the [Release action](https://github.com/zaknesler/dirgal/actions/workflows/release.yml) to download the latest build for your platform.

Alternatively, you could install from source via Cargo:

```sh
cargo install --git https://github.com/zaknesler/dirgal
```

### Usage

The `dirgal` command can simply be run inside your terminal:

```sh
# Scan all images (recursively) in the current directory and open a gallery window
dirgal

# Or pass in multiple roots...
dirgal ~/Downloads ~/Pictures
```

### Todo

- Error/warning notifications (toasts or something)
  - Make utils return results, and handle all the error popups/tracing logs in the gallery
- Switching images in grouped view with a sort applied (e.g. size/date-in-path) moves between the ungrouped images
- Arrow navigating up/down in grouped view:
  - does not scroll page
  - does not stay in the same column in groups with different sizes
- Stats (e.g. number of images, duplicate count, total size, num folders, etc.)
- Improved filtering/searching
- Better experience when selecting multiple items
- Metadata info (including EXIF data)
- Ensure GIFs don't crash
  - switching between GIFs can crash if the currently-playing GIF is on a frame that is beyond the frame count of the next GIF file
  - (not sure what I can do about this...)

### Ideas

Some of these are completely out-of-scope and unrealistic, but would be nice to have:

- Copy and rename
- More keyboard navigation
- `--no-cache` to... bypass the cache of course
- Improved duplicate detection/experience (it currently hides duplicate images from the main view)
- Save the hash cache periodically during a scan, not just at the end, so a big scan over a slow drive (like my really slow NAS) doesn't lose all its progress if interrupted
- Watch mode?
- Image tagging?
- RAW images?
- Videos?
- Similar image detection?
- Batch renaming?
