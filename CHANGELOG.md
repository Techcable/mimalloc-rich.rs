# Changelog

Notable changes to this project should be documented in this file.
Make sure it is up to date before performing a release.

This project follows the [Keep a Changelog](https://keepachangelog.com/en/2.0.0/) format wherever that is reasonable.

The "title" of each release should be its first line.
A title is required for publishing a github release, so all versions should have one.

Most changes include the relevant [jj](https://jj-vcs.dev) change ids in parens. An example of a change id is wuoxvnsw.

## Unreleased

### Added
- Have mimalloc-rich-src emit version & include build metadata (sykvonmx)

### Changes
- Add support for mimalloc v3 and make it the default version (vklkptmv)
  - Almost completely rewrites the heap API
- Update mimalloc v3 to v3.6.1
- Add `links = "mimalloc"` to mimalloc-rich-src (tuplmswo)
- Automatically enable allocator API on Rust 1.100, removing `nightly-allocator-api` feature (zzqowmvy)
  - This means we no longer support very old 1.100 nightlies,
    which is fine since 1.100 is already in beta and will soon be stable.
- Mark `declare-global-allocator` feature with `discouraged` prefix (qlpzksly)
  - This is not idiomatic, as the top-level binary or library should do the declaration itself.
  - Not clear if there are any good use cases at all.

### Fixed
- On Rust 1.93 or later, precisely detect `cfg!(debug_assertions)` for the `debug-if-debug` feature
  - Before [cargo#16160], build-script detection was imperfect as cargo didn't set the appropriate environment variables
- Run `mi_heap_delete` on drop (as documented) rather than `mi_heap_destroy` (xwzsmntp)
  - This would have been a serious bug, freeing all allocated memory while documentation said otherwise.
- Ensure `<MiMalloc as GlobalAlloc>::realloc` respects alignment by using `mi_realloc_aligned` rather than plain `mi_realloc` (turxpuuu)
- Treat any non-zero value of a flag option as enabled in `mi_option_get`, matching mimalloc (qknyznpz)
  - Previously values other than `0` and `1` returned an `InvalidOptionGetError`

[cargo#16160]: https://github.com/rust-lang/cargo/pull/16160

## 0.1.0-alpha.1 - 2026-09-30
Split vendored sources into their own crate.

### Added
- Support building mimalloc v3 (lnslyosl)
  - Using this version breaks most of the high-level bindings in `mimalloc-rich`
- Support allocator-api v0.2, v0.3, and v0.4, all at once (ynxstnuk)
  - Enabled by default with `mimalloc-rich/allocator-api2-all` feature.

### Changes
- Deprecate the MiHeap API, as it will need to be rewritten when mimalloc v3 support is added (mztqvrpx)
- Rename `mimalloc-rich-sys/override` feature to `mimalloc-rich-sys/override-libc-malloc` (zzxlwnyv)
- Split vendored sources into own crate (pvvpzosl)
  - Will reduce download time when there are multiple mimalloc versions.
  - Will reduces download time when building against a system library (not currently supported)
- Rename `mimalloc-rich-sys/vendored-mimalloc` to `mimalloc-rich-sys/vendored` (nqpsmsnn)

### Associated Versions
- `mimalloc@2.5.2`
- `mimalloc@3.5.3`
- `mimalloc-rich-src-v2@1.5.200`
- `mimalloc-rich-src-v3@1.5.300`

## 0.1.0-alpha.0 - 2026-09-28
Initial release.

Code largely taken from [DuckLogic](https://ducklogic.org), but cleaned up for public release.

Uses mimalloc v2.5.2, as v3 wasn't stable when this code was originally written.

Uses allocator-api2 v0.2.* as that is what hashbrown v0.17 and bumpalo v3 both use.
