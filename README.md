<img width="100" src=".github/assets/logo.svg" />

**dirgal** _[directory gallery]_ is a fast, friendly image gallery you can open in a directory with a single command.

It's not meant to be used like a photo catalog like Capture One. Just a quick way to browse through a directory of images to see what's in there.

Using Zed's wonderful GPUI library, _dirgal_ is nimble and responsive, and currently supports a few basic features such as bookmarks, group/grid/list views, sorting, basic duplicate detection, grid sizing, etc. with more features planned in the future. For faster subsequent runs, it caches image thumbnails and content hashes.

This is a work-in-progress side project and is mainly for casual (read: not professional) use.

### Usage

```sh
# Open a gallery that displays all nested images
dirgal ~/Downloads

# Prefetch all thumbnails ahead of time
dirgal --prefetch ~/Downloads
```

### Future ideas

Some of these are completely out-of-scope and unrealistic, but why not include them:

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
- Videos?
- Similar image detection?
- Batch renaming?
