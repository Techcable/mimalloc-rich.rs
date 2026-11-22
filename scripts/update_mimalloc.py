#!/usr/bin/env python3
"""Update vendored mimalloc releases and their Rust version metadata."""

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"


def git(directory: Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-C", str(directory), *args],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()


def parse_version(value: str) -> tuple[int, int, int]:
    match = re.fullmatch(VERSION, value)
    if not match:
        raise ValueError(f"Expected a stable version X.Y.Z, got {value!r}")
    return tuple(map(int, match.groups()))


def select_tag(tags: str, major: int, requested: str | None) -> str:
    releases = {
        parse_version(tag[1:]): tag
        for tag in tags.splitlines()
        if re.fullmatch("v" + VERSION, tag) and tag.startswith(f"v{major}.")
    }
    if not releases:
        raise ValueError(f"No stable v{major} releases found")
    version = parse_version(requested.removeprefix("v")) if requested else max(releases)
    if version not in releases:
        raise ValueError(f"No stable v{major} release matching {requested!r}")
    return releases[version]


def replace_once(pattern: str, value: str, text: str) -> str:
    result, count = re.subn(
        pattern, lambda m: m[1] + value + m[3], text, flags=re.MULTILINE
    )
    if count != 1:
        raise ValueError(f"Expected exactly one version field matching {pattern!r}")
    return result


def update_metadata(
    manifest: str, source: str, major: int, target: str
) -> tuple[str, str]:
    # Match the package section only; preserve comments and all other formatting.
    section = re.search(r"(?ms)^\[package\]\s*\n(.*?)(?=^\[|\Z)", manifest)
    if not section:
        raise ValueError("Missing [package] section")
    package = section[1]
    field = r'^(version\s*=\s*")([^"]+)(")'
    current = re.search(field, package, re.MULTILINE)
    expected_field = r'^(const EXPECTED_MIMALLOC_VERSION: &str = ")([^"]+)(";)'
    expected = re.search(expected_field, source, re.MULTILINE)
    if not current or not expected:
        raise ValueError("Missing crate version or EXPECTED_MIMALLOC_VERSION")
    crate_major, minor, patch = parse_version(current[2])
    old_version = parse_version(expected[2])
    new_version = parse_version(target)
    if old_version != (major, minor, patch // 100):
        raise ValueError(
            "Existing crate version and EXPECTED_MIMALLOC_VERSION disagree"
        )
    if new_version[0] != major or new_version < old_version:
        raise ValueError(
            f"Refusing to downgrade or change major: {expected[2]} -> {target}"
        )
    # Keep local patch releases when the upstream release hasn't changed.
    new_crate = current[2]
    if new_version != old_version:
        new_crate = f"{crate_major}.{new_version[1]}.{new_version[2] * 100}"
    package = replace_once(field, new_crate, package)
    manifest = manifest[: section.start(1)] + package + manifest[section.end(1) :]
    return manifest, replace_once(expected_field, target, source)


def update(major: int, requested: str | None, dry_run: bool) -> None:
    crate = ROOT / "libs" / f"mimalloc-rich-src-v{major}"
    module = crate / f"mimalloc-v{major}"
    if not (module / ".git").exists():
        git(ROOT, "submodule", "update", "--init", "--", str(module.relative_to(ROOT)))
    if git(module, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError(f"Submodule has local changes: {module}")
    git(module, "fetch", "--tags", "origin")
    tag = select_tag(git(module, "tag", "--list", f"v{major}.*"), major, requested)
    paths = [crate / "Cargo.toml", crate / "src/lib.rs"]
    original = [path.read_text() for path in paths]
    updated = update_metadata(*original, major, tag[1:])
    print(f"{module.relative_to(ROOT)} -> {tag}")
    if dry_run:
        return
    commit = git(module, "rev-parse", f"refs/tags/{tag}^{{commit}}")
    if git(module, "rev-parse", "HEAD") != commit:
        git(module, "checkout", "--detach", commit)
    for path, old, new in zip(paths, original, updated):
        if old != new:
            path.write_text(new)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--v2", metavar="VERSION", help="explicit stable v2 release")
    parser.add_argument("--v3", metavar="VERSION", help="explicit stable v3 release")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="fetch and validate updates without changing existing checkouts or metadata",
    )
    parser.epilog = (
        "Without version options, update both majors to their latest stable tags. "
        "With version options, update only the specified majors. Missing submodules "
        "are initialized, including during dry runs. Changes are left uncommitted; "
        "if an update fails, earlier updates are kept."
    )
    args = parser.parse_args(argv)
    selected = [(major, getattr(args, f"v{major}")) for major in (2, 3)]
    if args.v2 or args.v3:
        selected = [(major, version) for major, version in selected if version]
    try:
        # Validate CLI versions before any network or working-tree changes.
        for major, version in selected:
            if version and parse_version(version.removeprefix("v"))[0] != major:
                raise ValueError(f"--v{major} requires a v{major} release")
        for major, version in selected:
            update(major, version, args.dry_run)
        return 0
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        detail = (
            error.stderr
            if isinstance(error, subprocess.CalledProcessError)
            else str(error)
        )
        print(f"error: {detail.strip()}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
