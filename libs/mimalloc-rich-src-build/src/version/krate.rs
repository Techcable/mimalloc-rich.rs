use anyhow::{Context, anyhow};

use crate::VendoredSource;

pub fn determine_version(vendored_source: &VendoredSource) -> anyhow::Result<String> {
    let parsed_crate_version = semver::Version::parse(vendored_source.vendored_crate_version).with_context(|| {
        format!(
            "The `vendored_crate_version = {ver:?}` is not valid semver",
            ver = vendored_source.vendored_crate_version
        )
    })?;
    let major_version = determine_major_version(vendored_source)
        .with_context(|| format!("Failed to determine major version for {}", vendored_source.crate_desc()))?;
    let minor_version = parsed_crate_version.minor;
    let patch_version = parsed_crate_version.patch / 100;
    let version_from_crate_version = semver::Version::new(major_version, minor_version, patch_version);
    Ok(version_from_crate_version.to_string())
}

fn determine_major_version(vendored_source: &VendoredSource) -> anyhow::Result<u64> {
    const EXPECTED_NAME_PREFIX: &str = "mimalloc_rich_src_v";
    let vendored_crate_name = vendored_source.vendored_crate_name;
    let remaining_text = vendored_crate_name.strip_prefix(EXPECTED_NAME_PREFIX).ok_or_else(|| {
        anyhow!("Crate name {vendored_crate_name:?} doesn't have expected prefix {EXPECTED_NAME_PREFIX:?}")
    })?;
    remaining_text.parse::<u64>().with_context(|| {
        format!("Crate name {vendored_crate_name:?} doesn't end with `src-v{{major_version}}` as expected")
    })
}
