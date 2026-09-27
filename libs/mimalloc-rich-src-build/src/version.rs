//! Scrapes the version from the cmake file.

use anyhow::{Context, ensure};

use crate::VendoredSource;

pub mod cmake;
pub mod krate;

pub fn verify_version(vendored_source: &VendoredSource) -> anyhow::Result<()> {
    // allow optout in case these version checks somehow fail in the future
    if std::env::var_os("MIMALLOC_RICH_SKIP_VERSION_VERIFY").is_some_and(|x| !x.is_empty()) {
        return Ok(());
    }
    // verify crate version matches library version
    {
        let version_from_crate_version = krate::determine_version(vendored_source)
            .context("Failed to determine mimalloc version from crate version")?;
        ensure!(
            version_from_crate_version == vendored_source.expected_version,
            "Expected {crate_desc} to use mimalloc {version_from_crate_version}, but got {expected_version} instead",
            crate_desc = vendored_source.crate_desc(),
            expected_version = vendored_source.expected_version,
        );
    }
    // verify cmake version matches
    let cmake_version = cmake::detect_version(vendored_source).context("Failed to read version from cmake")?;
    ensure!(
        cmake_version == vendored_source.expected_version,
        "Version detected from cmake ({cmake_version}) doesn't match expected version {expected_version}",
        expected_version = vendored_source.expected_version,
    );
    // TODO: Optionally verify submodule tag with git CLI.
    Ok(())
}
