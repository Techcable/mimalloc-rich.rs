# Changelog

Notable changes to this project should be documented in this file.
Make sure it is up to date before performing a release.

This project follows the [Keep a Changelog](https://keepachangelog.com/en/2.0.0/) format wherever that is reasonable.

The "title" of each release should be its first line.
A title is required for publishing a github release, so all versions should have one.

Most changes include the relevant [jj](https://jj-vcs.dev) change ids in parens. An example of a change id is wuoxvnsw.

## Unreleased

### Changes
- Rename `mimalloc-rich-sys/override` feature to `mimalloc-rich-sys/override-libc-malloc` (zzxlwnyv)

## 0.1.0-alpha.0 - 2026-09-28
Initial release.

Code largely taken from [DuckLogic](https://ducklogic.org), but cleaned up for public release.

Uses mimalloc v2.5.2, as v3 wasn't stable when this code was originally written.

Uses allocator-api2 v0.2.* as that is what hashbrown v0.17 and bumpalo v3 both use.
