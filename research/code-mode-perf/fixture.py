"""Deterministic, source-only Git fixtures. Nothing is fetched or checked out."""

import os
from pathlib import Path
import subprocess

ENTRY = "src/core/entry.rs"
TYPES = "src/core/types.rs"
HIT_PATHS = tuple(f"src/components/reconciliation_pipeline_{name}/dispatch.rs"
                  for name in ("alpha", "beta", "gamma"))
MARKER = "perf_fixture_tag"
HITS_PER_FILE = 8
SMALL_FILES = 12
LARGE_FILES = 2206


def write(root: Path, path: str, text: str) -> None:
    file = root / path
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_bytes(text.encode("utf-8"))


def create(root: Path, files: int, env: dict[str, str]) -> dict:
    root.mkdir()
    write(root, ENTRY, "use crate::types::FixtureConfig;\n\n"
          "pub fn choose_mode(config: &FixtureConfig) -> u32 {\n"
          "    config.field_00 + 1\n}\n")
    fields = "".join(f"    pub field_{i:02}: u32,\n" for i in range(30))
    write(root, TYPES, f"pub struct FixtureConfig {{\n{fields}}}\n")
    for index, path in enumerate(HIT_PATHS):
        hits = "".join(f"    total += {step + 1}; // {MARKER}\n"
                       for step in range(HITS_PER_FILE))
        write(root, path, f"use crate::entry::choose_mode;\n"
              f"use crate::types::FixtureConfig;\n\n"
              f"pub fn dispatch_{index}(config: &FixtureConfig) -> u32 {{\n"
              f"    let mut total = choose_mode(config);\n{hits}    total\n}}\n")
    for index in range(files - 2 - len(HIT_PATHS)):
        functions = "\n".join(
            f"pub fn work_{index:04}_{n}(value: u32) -> u32 {{\n"
            f"    let offset = {index + n};\n"
            f"    value.wrapping_add(offset)\n}}\n"
            for n in range(8)
        )
        write(root, f"src/family_{index // 64:02}/module_{index:04}.rs", functions)
    for args in (["init", "--quiet"], ["-c", "core.autocrlf=false", "add", "--all"]):
        subprocess.run(["git", *args], cwd=root, env=env, check=True, timeout=30)
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=root, env=env,
                                      timeout=30).split(b"\0")[:-1]
    if len(tracked) != files:
        raise RuntimeError(f"fixture has {len(tracked)} tracked files, expected {files}")
    return {"tracked_files": len(tracked), "tracked_bytes": sum(
        (root / os.fsdecode(path)).stat().st_size for path in tracked)}
