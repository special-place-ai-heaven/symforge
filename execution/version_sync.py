#!/usr/bin/env python
"""Check and repair release-version alignment across repo manifests."""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


class VersionSyncError(RuntimeError):
    """Raised when release version metadata is missing or inconsistent."""


def repo_root(path: str | None = None) -> Path:
    if path is not None:
        return Path(path).resolve()
    return Path(__file__).resolve().parent.parent


def manifest_path(root: Path) -> Path:
    return root / ".github" / ".release-please-manifest.json"


def cargo_path(root: Path) -> Path:
    return root / "Cargo.toml"


def npm_path(root: Path) -> Path:
    return root / "npm" / "package.json"


def read_manifest_version(root: Path) -> str:
    data = json.loads(manifest_path(root).read_text(encoding="utf-8"))
    version = data.get(".")
    if not isinstance(version, str) or not version:
        raise VersionSyncError("Missing root version in .github/.release-please-manifest.json.")
    return version


def read_cargo_version(root: Path) -> str:
    manifest = tomllib.loads(cargo_path(root).read_text(encoding="utf-8"))
    version = manifest.get("package", {}).get("version")
    if not isinstance(version, str) or not version:
        raise VersionSyncError("Missing package.version in Cargo.toml.")
    return version


def read_npm_version(root: Path) -> str:
    manifest = json.loads(npm_path(root).read_text(encoding="utf-8"))
    version = manifest.get("version")
    if not isinstance(version, str) or not version:
        raise VersionSyncError("Missing version in npm/package.json.")
    return version

def read_npm_optional_dependencies(root: Path) -> dict[str, str]:
    manifest = json.loads(npm_path(root).read_text(encoding="utf-8"))
    deps = manifest.get("optionalDependencies", {})
    if not isinstance(deps, dict):
        raise VersionSyncError("npm/package.json optionalDependencies must be an object.")
    return {str(name): str(pin) for name, pin in deps.items()}


def collect_versions(root: Path) -> dict[str, str]:
    return {
        "manifest": read_manifest_version(root),
        "cargo": read_cargo_version(root),
        "npm": read_npm_version(root),
    }


def normalize_tag(tag: str | None) -> str | None:
    if tag is None:
        return None
    cleaned = tag.strip()
    if not cleaned:
        return None
    if cleaned.startswith("v") and SEMVER_RE.fullmatch(cleaned[1:]):
        return cleaned[1:]
    if SEMVER_RE.fullmatch(cleaned):
        return cleaned
    raise VersionSyncError(
        f"canonical release tags must use plain vX.Y.Z format, got '{cleaned}'."
    )


def check_versions(root: Path, tag: str | None = None) -> list[str]:
    versions = collect_versions(root)
    canonical = versions["manifest"]
    problems: list[str] = []

    for label in ("cargo", "npm"):
        if versions[label] != canonical:
            problems.append(
                f"{label} version '{versions[label]}' does not match release manifest '{canonical}'."
            )

    try:
        tag_version = normalize_tag(tag)
    except VersionSyncError as error:
        problems.append(str(error))
    else:
        if tag_version is not None and tag_version != canonical:
            problems.append(
                f"tag version '{tag_version}' does not match release manifest '{canonical}'."
            )

    for name, pin in sorted(read_npm_optional_dependencies(root).items()):
        if pin != canonical:
            problems.append(
                f"npm optionalDependencies '{name}' pin '{pin}' does not match "
                f"release manifest '{canonical}'."
            )

    return problems


def validate_semver(version: str) -> str:
    if not SEMVER_RE.fullmatch(version):
        raise VersionSyncError(
            f"'{version}' is not a supported semantic version (expected x.y.z or x.y.z-suffix)."
        )
    return version


def replace_cargo_version(text: str, version: str) -> str:
    start = text.find("[package]")
    if start == -1:
        raise VersionSyncError("Cargo.toml is missing a [package] section.")

    next_section = text.find("\n[", start + len("[package]"))
    end = len(text) if next_section == -1 else next_section + 1
    section = text[start:end]
    updated, count = re.subn(
        r'(?m)^(version\s*=\s*")([^"]+)(")$',
        rf'\g<1>{version}\g<3>',
        section,
        count=1,
    )
    if count != 1:
        raise VersionSyncError("Could not update Cargo.toml package.version.")
    return text[:start] + updated + text[end:]


def write_manifest_version(root: Path, version: str) -> None:
    manifest = json.loads(manifest_path(root).read_text(encoding="utf-8"))
    manifest["."] = version
    manifest_path(root).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def write_cargo_version(root: Path, version: str) -> None:
    path = cargo_path(root)
    path.write_text(
        replace_cargo_version(path.read_text(encoding="utf-8"), version),
        encoding="utf-8",
    )


def write_npm_version(root: Path, version: str) -> None:
    path = npm_path(root)
    manifest = json.loads(path.read_text(encoding="utf-8"))
    manifest["version"] = version
    path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def set_version(root: Path, version: str) -> list[Path]:
    version = validate_semver(version)
    changed: list[Path] = []

    if read_manifest_version(root) != version:
        write_manifest_version(root, version)
        changed.append(manifest_path(root))

    if read_cargo_version(root) != version:
        write_cargo_version(root, version)
        changed.append(cargo_path(root))

    if read_npm_version(root) != version:
        write_npm_version(root, version)
        changed.append(npm_path(root))

    return changed


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Check or update SymForge release-version metadata."
    )
    parser.add_argument(
        "--root",
        default=None,
        help="Repository root to inspect. Defaults to the current repo.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("current", help="Print the canonical release version.")

    check = subparsers.add_parser(
        "check",
        help="Verify release manifest, Cargo.toml, npm/package.json, and an optional tag agree.",
    )
    check.add_argument(
        "--tag",
        default=None,
        help="Optional release tag to validate, for example v0.3.12.",
    )

    set_cmd = subparsers.add_parser(
        "set",
        help="Update the release manifest and publishable package versions together.",
    )
    set_cmd.add_argument("version", help="Version to apply.")

    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root = repo_root(args.root)

    try:
        if args.command == "current":
            print(read_manifest_version(root))
            return 0

        if args.command == "check":
            problems = check_versions(root, tag=args.tag)
            if problems:
                for problem in problems:
                    print(problem, file=sys.stderr)
                return 1
            print(f"Version check passed: {read_manifest_version(root)}")
            return 0

        if args.command == "set":
            changed = set_version(root, args.version)
            if changed:
                for path in changed:
                    print(path.relative_to(root).as_posix())
            else:
                print("Version already up to date.")
            return 0
    except VersionSyncError as error:
        print(error, file=sys.stderr)
        return 1

    return 2


if __name__ == "__main__":
    raise SystemExit(main())
