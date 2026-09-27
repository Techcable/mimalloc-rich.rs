use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader};

use anyhow::{Context, anyhow, bail};
use itertools::Itertools;

use crate::VendoredSource;

const CMAKE_VERSION_FILE: &str = "cmake/mimalloc-config-version.cmake";

pub(crate) fn detect_version(source: &VendoredSource) -> anyhow::Result<String> {
    let cmake_version_file = source.source_root.join(CMAKE_VERSION_FILE);
    let file = BufReader::new(File::open(&cmake_version_file)?);
    let mut by_part = BTreeMap::new();
    for line in file.lines() {
        let line = line?;
        let line = line.trim();
        if !line.starts_with(VersionPart::CMAKE_LINE_COMMON_PREFIX) {
            continue;
        }
        let part = VersionPart::ALL
            .iter()
            .find(|&&part| line.starts_with(&part.full_prefix()))
            .ok_or_else(|| anyhow!("Unable to parse version part for {line:?}"))?;
        let remaining = line
            .strip_prefix(&part.full_prefix())
            .and_then(|text| text.strip_suffix(VersionPart::CMAKE_LINE_COMMON_SUFFIX))
            .ok_or_else(|| anyhow!("Failed to parse {part} version (unexpected prefix or suffix): {line:?}"))?;
        let value = remaining
            .trim()
            .parse::<u32>()
            .with_context(|| format!("Failed to parse {part} version from {line:?}"))?;
        if let Some(existing) = by_part.get(&part) {
            bail!("Found duplicate {part:?} version parts: {value} and {existing}");
        }
        by_part.insert(part, value);
    }
    Ok(by_part.values().join("."))
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
enum VersionPart {
    // order reflects printing
    Major,
    Minor,
    Patch,
}
impl VersionPart {
    const CMAKE_LINE_COMMON_PREFIX: &str = "set(mi_version_";
    const CMAKE_LINE_COMMON_SUFFIX: &str = ")";
    const ALL: [VersionPart; 3] = [VersionPart::Major, VersionPart::Minor, VersionPart::Patch];
    fn name(self) -> &'static str {
        match self {
            VersionPart::Major => "major",
            VersionPart::Minor => "minor",
            VersionPart::Patch => "patch",
        }
    }
    fn full_prefix(self) -> String {
        format!("{common_prefix}{self}", common_prefix = Self::CMAKE_LINE_COMMON_PREFIX)
    }
}
impl Display for VersionPart {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}
