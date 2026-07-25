#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Generate the deterministic SFBENCH-1.0 external control repositories."""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import os
from dataclasses import dataclass
from pathlib import Path
import subprocess
import sys
import textwrap
import tomllib
from typing import Iterable


PROTOCOL_ID = "SFBENCH-1.0"
FIXTURE_VERSION = 1
AUTHOR_NAME = "SymForge Fixture"
AUTHOR_EMAIL = "sfbench@example.invalid"

MARKERS = {
    "source": "SF_BENCH_SOURCE_9F31",
    "test": "SF_BENCH_TEST_ONLY_9F31",
    "generated": "SF_BENCH_GENERATED_9F31",
    "vendor": "SF_BENCH_VENDOR_9F31",
    "edit_old": "SF_BENCH_EDIT_OLD_9F31",
    "edit_new": "SF_BENCH_EDIT_NEW_9F31",
    "ccr": "SF_BENCH_CCR_9F31",
    "personal": "SF_BENCH_PERSONAL_9F31",
}


@dataclass(frozen=True)
class SymbolLocator:
    language: str
    path: str
    name: str
    kind: str
    source: str


@dataclass(frozen=True)
class ImportLocator:
    language: str
    from_path: str
    to_path: str
    alias: str | None
    source: str


@dataclass
class FixtureModel:
    files: dict[str, bytes]
    symbols: list[SymbolLocator]
    imports: list[ImportLocator]
    call_edges: dict[str, list[tuple[str, str]]]
    reference_callers: dict[str, list[str]]
    implementors: dict[str, dict[str, str]]
    dependents: dict[str, dict[str, list[str]]]
    mutation_bodies: dict[str, dict[str, str]]


def utf8(value: str) -> bytes:
    return value.encode("utf-8")


def block(value: str) -> str:
    return textwrap.dedent(value).strip("\n")


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def add_file(files: dict[str, bytes], path: str, value: str | bytes) -> None:
    data = utf8(value) if isinstance(value, str) else value
    if path in files:
        raise RuntimeError(f"duplicate fixture path: {path}")
    files[path] = data


def assemble(header: str, blocks: Iterable[str], footer: str = "") -> str:
    parts = [header.strip("\n"), *(item.strip("\n") for item in blocks)]
    if footer:
        parts.append(footer.strip("\n"))
    return "\n\n".join(parts) + "\n"


def rust_fixture(model: FixtureModel) -> None:
    language = "Rust"
    root = "sfbench_fixture/rust"
    protocol_path = f"{root}/src/protocol.rs"
    core_path = f"{root}/src/core.rs"
    protocol = block(
        """
        pub trait SfBenchProtocol {
            fn run(&self) -> String;
        }
        """
    )
    parts = {
        "leaf": block(
            """
            pub fn sfbench_leaf() -> String {
                "leaf".to_owned()
            }
            """
        ),
        "mid": block(
            """
            pub fn sfbench_mid() -> String {
                sfbench_leaf()
            }
            """
        ),
        "rename": block(
            """
            pub fn sfbench_rename_me() -> String {
                "rename".to_owned()
            }
            """
        ),
        "worker_struct": block(
            """
            pub struct SfBenchWorker;
            """
        ),
        "worker_impl": block(
            """
            impl SfBenchProtocol for SfBenchWorker {
                fn run(&self) -> String {
                    sfbench_rename_me()
                }
            }
            """
        ),
        "entry": block(
            """
            pub fn sfbench_entry() -> String {
                let worker = SfBenchWorker;
                format!("{}:{}", sfbench_mid(), worker.run())
            }
            """
        ),
        "unused": block(
            """
            pub fn sfbench_unused() -> String {
                "unused".to_owned()
            }
            """
        ),
        "user_one": block(
            """
            pub fn sfbench_rename_user_one() -> String {
                sfbench_rename_me()
            }
            """
        ),
        "user_two": block(
            """
            pub fn sfbench_rename_user_two() -> String {
                sfbench_rename_me()
            }
            """
        ),
        "user_three": block(
            """
            pub fn sfbench_rename_user_three() -> String {
                sfbench_rename_me()
            }
            """
        ),
        "mutable": block(
            f"""
            pub fn sfbench_mutable() -> (&'static str, &'static str, &'static str) {{
                let first = "{MARKERS['edit_old']}";
                let unicode_inside = "žarek λ";
                let second = "{MARKERS['edit_old']}";
                (first, unicode_inside, second)
            }}
            """
        ),
        "outside": block(
            f"""
            pub fn sfbench_outside_literal() -> &'static str {{
                "{MARKERS['edit_old']}"
            }}
            """
        ),
        "delete": block(
            """
            pub fn sfbench_delete_me() -> &'static str {
                "delete"
            }
            """
        ),
        "nested": block(
            """
            pub mod sfbench_outer {
                pub mod inner {
                    pub fn sfbench_nested() -> &'static str {
                        "nested"
                    }
                }
            }
            """
        ),
    }
    header = block(
        f"""
        use super::protocol::SfBenchProtocol;

        pub const SF_BENCH_SOURCE_MARKER: &str = "{MARKERS['source']}";
        pub const SF_BENCH_UNICODE_BEFORE: &str = "naïve λ";
        """
    )
    footer = block(
        """
        // Non-code mention: sfbench_rename_me
        pub const SF_BENCH_RENAME_TEXT: &str = "sfbench_rename_me";
        """
    )
    core = assemble(header, parts.values(), footer)
    add_file(model.files, protocol_path, protocol + "\n")
    add_file(model.files, core_path, core)
    add_file(
        model.files,
        f"{root}/src/mod.rs",
        "pub mod core;\npub mod cycle_a;\npub mod cycle_b;\npub mod protocol;\n",
    )
    cycle_a = block(
        """
        use super::cycle_b::CycleB as OtherB;

        pub struct CycleA {
            pub other: Option<Box<OtherB>>,
        }
        """
    )
    cycle_b = block(
        """
        use super::cycle_a::CycleA as OtherA;

        pub struct CycleB {
            pub other: Option<Box<OtherA>>,
        }
        """
    )
    add_file(model.files, f"{root}/src/cycle_a.rs", cycle_a + "\n")
    add_file(model.files, f"{root}/src/cycle_b.rs", cycle_b + "\n")
    test_path = f"{root}/tests/test_core.rs"
    add_file(
        model.files,
        test_path,
        f"use sfbench_fixture::rust::core::sfbench_entry;\n\nconst TEST_MARKER: &str = \"{MARKERS['test']}\";\n\n#[test]\nfn sfbench_test_entry() {{\n    assert!(!TEST_MARKER.is_empty());\n    assert!(!sfbench_entry().is_empty());\n}}\n",
    )
    for directory in ("a", "b"):
        duplicate_path = f"{root}/duplicates/{directory}/duplicate.rs"
        duplicate = "pub fn sfbench_duplicate() -> &'static str {\n    \"duplicate\"\n}\n"
        add_file(model.files, duplicate_path, duplicate)
        model.symbols.append(
            SymbolLocator(language, duplicate_path, "sfbench_duplicate", "fn", duplicate.strip("\n"))
        )
    model.symbols.extend(
        [
            SymbolLocator(language, protocol_path, "SfBenchProtocol", "trait", protocol),
            SymbolLocator(language, core_path, "sfbench_leaf", "fn", parts["leaf"]),
            SymbolLocator(language, core_path, "sfbench_mid", "fn", parts["mid"]),
            SymbolLocator(language, core_path, "sfbench_rename_me", "fn", parts["rename"]),
            SymbolLocator(language, core_path, "SfBenchWorker", "struct", parts["worker_struct"]),
            SymbolLocator(language, core_path, "run", "method", "    fn run(&self) -> String {\n        sfbench_rename_me()\n    }"),
            SymbolLocator(language, core_path, "sfbench_entry", "fn", parts["entry"]),
            SymbolLocator(language, core_path, "sfbench_unused", "fn", parts["unused"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_one", "fn", parts["user_one"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_two", "fn", parts["user_two"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_three", "fn", parts["user_three"]),
            SymbolLocator(language, core_path, "sfbench_mutable", "fn", parts["mutable"]),
            SymbolLocator(language, core_path, "sfbench_outside_literal", "fn", parts["outside"]),
            SymbolLocator(language, core_path, "sfbench_delete_me", "fn", parts["delete"]),
            SymbolLocator(language, core_path, "sfbench_nested", "fn", "        pub fn sfbench_nested() -> &'static str {\n            \"nested\"\n        }"),
            SymbolLocator(language, f"{root}/src/cycle_a.rs", "CycleA", "struct", "pub struct CycleA {\n    pub other: Option<Box<OtherB>>,\n}"),
            SymbolLocator(language, f"{root}/src/cycle_b.rs", "CycleB", "struct", "pub struct CycleB {\n    pub other: Option<Box<OtherA>>,\n}"),
        ]
    )
    model.imports.extend(
        [
            ImportLocator(language, core_path, protocol_path, None, "use super::protocol::SfBenchProtocol;"),
            ImportLocator(language, f"{root}/src/cycle_a.rs", f"{root}/src/cycle_b.rs", "OtherB", "use super::cycle_b::CycleB as OtherB;"),
            ImportLocator(language, f"{root}/src/cycle_b.rs", f"{root}/src/cycle_a.rs", "OtherA", "use super::cycle_a::CycleA as OtherA;"),
            ImportLocator(language, test_path, core_path, None, "use sfbench_fixture::rust::core::sfbench_entry;"),
        ]
    )
    register_language_relations(model, language, core_path, protocol_path, test_path, parts)


def python_fixture(model: FixtureModel) -> None:
    language = "Python"
    root = "sfbench_fixture/python"
    protocol_path = f"{root}/src/protocol.py"
    core_path = f"{root}/src/core.py"
    protocol = block(
        """
        from typing import Protocol

        class SfBenchProtocol(Protocol):
            def run(self) -> str: ...
        """
    )
    parts = {
        "leaf": "def sfbench_leaf() -> str:\n    return \"leaf\"",
        "mid": "def sfbench_mid() -> str:\n    return sfbench_leaf()",
        "rename": "def sfbench_rename_me() -> str:\n    return \"rename\"",
        "worker": "class SfBenchWorker(SfBenchProtocol):\n    def run(self) -> str:\n        return sfbench_rename_me()",
        "entry": "def sfbench_entry() -> str:\n    worker = SfBenchWorker()\n    return f\"{sfbench_mid()}:{worker.run()}\"",
        "unused": "def sfbench_unused() -> str:\n    return \"unused\"",
        "user_one": "def sfbench_rename_user_one() -> str:\n    return sfbench_rename_me()",
        "user_two": "def sfbench_rename_user_two() -> str:\n    return sfbench_rename_me()",
        "user_three": "def sfbench_rename_user_three() -> str:\n    return sfbench_rename_me()",
        "mutable": f"def sfbench_mutable() -> tuple[str, str, str]:\n    first = \"{MARKERS['edit_old']}\"\n    unicode_inside = \"žarek λ\"\n    second = \"{MARKERS['edit_old']}\"\n    return first, unicode_inside, second",
        "outside": f"def sfbench_outside_literal() -> str:\n    return \"{MARKERS['edit_old']}\"",
        "delete": "def sfbench_delete_me() -> str:\n    return \"delete\"",
        "nested": "class SfBenchOuter:\n    class Inner:\n        def sfbench_nested(self) -> str:\n            return \"nested\"",
    }
    header = f"from .protocol import SfBenchProtocol\n\nSF_BENCH_SOURCE_MARKER = \"{MARKERS['source']}\"\nSF_BENCH_UNICODE_BEFORE = \"naïve λ\""
    footer = "# Non-code mention: sfbench_rename_me\nSF_BENCH_RENAME_TEXT = \"sfbench_rename_me\""
    add_file(model.files, protocol_path, protocol + "\n")
    add_file(model.files, core_path, assemble(header, parts.values(), footer))
    add_file(model.files, f"{root}/src/__init__.py", "from .core import sfbench_entry\n")
    cycle_a = "from .cycle_b import CycleB as OtherB\n\nclass CycleA:\n    def __init__(self, other: OtherB | None = None):\n        self.other = other\n"
    cycle_b = "from .cycle_a import CycleA as OtherA\n\nclass CycleB:\n    def __init__(self, other: OtherA | None = None):\n        self.other = other\n"
    add_file(model.files, f"{root}/src/cycle_a.py", cycle_a)
    add_file(model.files, f"{root}/src/cycle_b.py", cycle_b)
    test_path = f"{root}/tests/test_core.py"
    add_file(model.files, test_path, f"from sfbench_fixture.python.src.core import sfbench_entry\n\nTEST_MARKER = \"{MARKERS['test']}\"\n\ndef test_entry() -> None:\n    assert sfbench_entry()\n")
    duplicate = "def sfbench_duplicate() -> str:\n    return \"duplicate\"\n"
    for directory in ("a", "b"):
        duplicate_path = f"{root}/duplicates/{directory}/duplicate.py"
        add_file(model.files, duplicate_path, duplicate)
        model.symbols.append(SymbolLocator(language, duplicate_path, "sfbench_duplicate", "fn", duplicate.strip("\n")))
    model.symbols.extend(
        [
            SymbolLocator(language, protocol_path, "SfBenchProtocol", "class", "class SfBenchProtocol(Protocol):\n    def run(self) -> str: ..."),
            SymbolLocator(language, core_path, "sfbench_leaf", "fn", parts["leaf"]),
            SymbolLocator(language, core_path, "sfbench_mid", "fn", parts["mid"]),
            SymbolLocator(language, core_path, "sfbench_rename_me", "fn", parts["rename"]),
            SymbolLocator(language, core_path, "SfBenchWorker", "class", parts["worker"]),
            SymbolLocator(language, core_path, "run", "method", "    def run(self) -> str:\n        return sfbench_rename_me()"),
            SymbolLocator(language, core_path, "sfbench_entry", "fn", parts["entry"]),
            SymbolLocator(language, core_path, "sfbench_unused", "fn", parts["unused"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_one", "fn", parts["user_one"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_two", "fn", parts["user_two"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_three", "fn", parts["user_three"]),
            SymbolLocator(language, core_path, "sfbench_mutable", "fn", parts["mutable"]),
            SymbolLocator(language, core_path, "sfbench_outside_literal", "fn", parts["outside"]),
            SymbolLocator(language, core_path, "sfbench_delete_me", "fn", parts["delete"]),
            SymbolLocator(language, core_path, "sfbench_nested", "method", "        def sfbench_nested(self) -> str:\n            return \"nested\""),
            SymbolLocator(language, f"{root}/src/cycle_a.py", "CycleA", "class", "class CycleA:\n    def __init__(self, other: OtherB | None = None):\n        self.other = other"),
            SymbolLocator(language, f"{root}/src/cycle_b.py", "CycleB", "class", "class CycleB:\n    def __init__(self, other: OtherA | None = None):\n        self.other = other"),
        ]
    )
    model.imports.extend(
        [
            ImportLocator(language, core_path, protocol_path, None, "from .protocol import SfBenchProtocol"),
            ImportLocator(language, f"{root}/src/cycle_a.py", f"{root}/src/cycle_b.py", "OtherB", "from .cycle_b import CycleB as OtherB"),
            ImportLocator(language, f"{root}/src/cycle_b.py", f"{root}/src/cycle_a.py", "OtherA", "from .cycle_a import CycleA as OtherA"),
            ImportLocator(language, test_path, core_path, None, "from sfbench_fixture.python.src.core import sfbench_entry"),
        ]
    )
    register_language_relations(model, language, core_path, protocol_path, test_path, parts)


def typescript_fixture(model: FixtureModel) -> None:
    language = "TypeScript"
    root = "sfbench_fixture/typescript"
    protocol_path = f"{root}/src/protocol.ts"
    core_path = f"{root}/src/core.ts"
    protocol = "export interface SfBenchProtocol {\n  run(): string;\n}"
    parts = {
        "leaf": "export function sfbench_leaf(): string {\n  return \"leaf\";\n}",
        "mid": "export function sfbench_mid(): string {\n  return sfbench_leaf();\n}",
        "rename": "export function sfbench_rename_me(): string {\n  return \"rename\";\n}",
        "worker": "export class SfBenchWorker implements SfBenchProtocol {\n  run(): string {\n    return sfbench_rename_me();\n  }\n}",
        "entry": "export function sfbench_entry(): string {\n  const worker = new SfBenchWorker();\n  return `${sfbench_mid()}:${worker.run()}`;\n}",
        "unused": "export function sfbench_unused(): string {\n  return \"unused\";\n}",
        "user_one": "export function sfbench_rename_user_one(): string {\n  return sfbench_rename_me();\n}",
        "user_two": "export function sfbench_rename_user_two(): string {\n  return sfbench_rename_me();\n}",
        "user_three": "export function sfbench_rename_user_three(): string {\n  return sfbench_rename_me();\n}",
        "mutable": f"export function sfbench_mutable(): [string, string, string] {{\n  const first = \"{MARKERS['edit_old']}\";\n  const unicodeInside = \"žarek λ\";\n  const second = \"{MARKERS['edit_old']}\";\n  return [first, unicodeInside, second];\n}}",
        "outside": f"export function sfbench_outside_literal(): string {{\n  return \"{MARKERS['edit_old']}\";\n}}",
        "delete": "export function sfbench_delete_me(): string {\n  return \"delete\";\n}",
        "nested": "export namespace SfBenchOuter {\n  export class Inner {\n    sfbench_nested(): string {\n      return \"nested\";\n    }\n  }\n}",
    }
    header = f"import type {{ SfBenchProtocol }} from \"./protocol\";\n\nexport const SF_BENCH_SOURCE_MARKER = \"{MARKERS['source']}\";\nexport const SF_BENCH_UNICODE_BEFORE = \"naïve λ\";"
    footer = "// Non-code mention: sfbench_rename_me\nexport const SF_BENCH_RENAME_TEXT = \"sfbench_rename_me\";"
    add_file(model.files, protocol_path, protocol + "\n")
    add_file(model.files, core_path, assemble(header, parts.values(), footer))
    cycle_a = "import type { CycleB as OtherB } from \"./cycle_b\";\n\nexport interface CycleA {\n  other?: OtherB;\n}\n"
    cycle_b = "import type { CycleA as OtherA } from \"./cycle_a\";\n\nexport interface CycleB {\n  other?: OtherA;\n}\n"
    add_file(model.files, f"{root}/src/cycle_a.ts", cycle_a)
    add_file(model.files, f"{root}/src/cycle_b.ts", cycle_b)
    test_path = f"{root}/tests/test_core.ts"
    add_file(model.files, test_path, f"import {{ sfbench_entry }} from \"../src/core\";\n\nconst TEST_MARKER = \"{MARKERS['test']}\";\nvoid sfbench_entry();\nvoid TEST_MARKER;\n")
    duplicate = "export function sfbench_duplicate(): string {\n  return \"duplicate\";\n}\n"
    for directory in ("a", "b"):
        duplicate_path = f"{root}/duplicates/{directory}/duplicate.ts"
        add_file(model.files, duplicate_path, duplicate)
        model.symbols.append(SymbolLocator(language, duplicate_path, "sfbench_duplicate", "fn", duplicate.strip("\n")))
    model.symbols.extend(
        [
            SymbolLocator(language, protocol_path, "SfBenchProtocol", "interface", protocol),
            SymbolLocator(language, core_path, "sfbench_leaf", "fn", parts["leaf"]),
            SymbolLocator(language, core_path, "sfbench_mid", "fn", parts["mid"]),
            SymbolLocator(language, core_path, "sfbench_rename_me", "fn", parts["rename"]),
            SymbolLocator(language, core_path, "SfBenchWorker", "class", parts["worker"]),
            SymbolLocator(language, core_path, "run", "method", "  run(): string {\n    return sfbench_rename_me();\n  }"),
            SymbolLocator(language, core_path, "sfbench_entry", "fn", parts["entry"]),
            SymbolLocator(language, core_path, "sfbench_unused", "fn", parts["unused"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_one", "fn", parts["user_one"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_two", "fn", parts["user_two"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_three", "fn", parts["user_three"]),
            SymbolLocator(language, core_path, "sfbench_mutable", "fn", parts["mutable"]),
            SymbolLocator(language, core_path, "sfbench_outside_literal", "fn", parts["outside"]),
            SymbolLocator(language, core_path, "sfbench_delete_me", "fn", parts["delete"]),
            SymbolLocator(language, core_path, "sfbench_nested", "method", "    sfbench_nested(): string {\n      return \"nested\";\n    }"),
            SymbolLocator(language, f"{root}/src/cycle_a.ts", "CycleA", "interface", "export interface CycleA {\n  other?: OtherB;\n}"),
            SymbolLocator(language, f"{root}/src/cycle_b.ts", "CycleB", "interface", "export interface CycleB {\n  other?: OtherA;\n}"),
        ]
    )
    model.imports.extend(
        [
            ImportLocator(language, core_path, protocol_path, None, "import type { SfBenchProtocol } from \"./protocol\";"),
            ImportLocator(language, f"{root}/src/cycle_a.ts", f"{root}/src/cycle_b.ts", "OtherB", "import type { CycleB as OtherB } from \"./cycle_b\";"),
            ImportLocator(language, f"{root}/src/cycle_b.ts", f"{root}/src/cycle_a.ts", "OtherA", "import type { CycleA as OtherA } from \"./cycle_a\";"),
            ImportLocator(language, test_path, core_path, None, "import { sfbench_entry } from \"../src/core\";"),
        ]
    )
    register_language_relations(model, language, core_path, protocol_path, test_path, parts)


def go_fixture(model: FixtureModel) -> None:
    language = "Go"
    root = "sfbench_fixture/go"
    protocol_path = f"{root}/src/protocol.go"
    core_path = f"{root}/src/core.go"
    protocol = "package fixture\n\ntype SfBenchProtocol interface {\n\trun() string\n}"
    parts = {
        "leaf": "func sfbench_leaf() string {\n\treturn sfstrings.TrimSpace(\" leaf \")\n}",
        "mid": "func sfbench_mid() string {\n\treturn sfbench_leaf()\n}",
        "rename": "func sfbench_rename_me() string {\n\treturn \"rename\"\n}",
        "worker": "type SfBenchWorker struct{}\n\nfunc (SfBenchWorker) run() string {\n\treturn sfbench_rename_me()\n}",
        "entry": "func sfbench_entry() string {\n\tworker := SfBenchWorker{}\n\treturn sfbench_mid() + \":\" + worker.run()\n}",
        "unused": "func sfbench_unused() string {\n\treturn \"unused\"\n}",
        "user_one": "func sfbench_rename_user_one() string {\n\treturn sfbench_rename_me()\n}",
        "user_two": "func sfbench_rename_user_two() string {\n\treturn sfbench_rename_me()\n}",
        "user_three": "func sfbench_rename_user_three() string {\n\treturn sfbench_rename_me()\n}",
        "mutable": f"func sfbench_mutable() (string, string, string) {{\n\tfirst := \"{MARKERS['edit_old']}\"\n\tunicodeInside := \"žarek λ\"\n\tsecond := \"{MARKERS['edit_old']}\"\n\treturn first, unicodeInside, second\n}}",
        "outside": f"func sfbench_outside_literal() string {{\n\treturn \"{MARKERS['edit_old']}\"\n}}",
        "delete": "func sfbench_delete_me() string {\n\treturn \"delete\"\n}",
        "nested": "type SfBenchOuter struct{}\n\ntype SfBenchInner struct{}\n\nfunc (SfBenchInner) sfbench_nested() string {\n\treturn \"nested\"\n}",
    }
    header = f"package fixture\n\nimport sfstrings \"strings\"\n\nconst SF_BENCH_SOURCE_MARKER = \"{MARKERS['source']}\"\nconst SF_BENCH_UNICODE_BEFORE = \"naïve λ\"\n\nvar _ SfBenchProtocol = SfBenchWorker{{}}"
    footer = "// Non-code mention: sfbench_rename_me\nconst SF_BENCH_RENAME_TEXT = \"sfbench_rename_me\""
    add_file(model.files, protocol_path, protocol + "\n")
    add_file(model.files, core_path, assemble(header, parts.values(), footer))
    cycle_a = "package fixture\n\ntype CycleA struct {\n\tOther *CycleAliasB\n}\n\ntype CycleAliasB = CycleB\n"
    cycle_b = "package fixture\n\ntype CycleB struct {\n\tOther *CycleAliasA\n}\n\ntype CycleAliasA = CycleA\n"
    add_file(model.files, f"{root}/src/cycle_a.go", cycle_a)
    add_file(model.files, f"{root}/src/cycle_b.go", cycle_b)
    test_path = f"{root}/tests/test_core.go"
    add_file(model.files, test_path, f"package fixture_test\n\nimport _ \"example.invalid/sfbench/sfbench_fixture/go/src\"\n\nconst testMarker = \"{MARKERS['test']}\"\n\nfunc sfbench_test_entry() string {{\n\treturn testMarker\n}}\n")
    duplicate = "package duplicate\n\nfunc sfbench_duplicate() string {\n\treturn \"duplicate\"\n}\n"
    for directory in ("a", "b"):
        duplicate_path = f"{root}/duplicates/{directory}/duplicate.go"
        add_file(model.files, duplicate_path, duplicate)
        model.symbols.append(SymbolLocator(language, duplicate_path, "sfbench_duplicate", "fn", "func sfbench_duplicate() string {\n\treturn \"duplicate\"\n}"))
    model.symbols.extend(
        [
            SymbolLocator(language, protocol_path, "SfBenchProtocol", "interface", "type SfBenchProtocol interface {\n\trun() string\n}"),
            SymbolLocator(language, core_path, "sfbench_leaf", "fn", parts["leaf"]),
            SymbolLocator(language, core_path, "sfbench_mid", "fn", parts["mid"]),
            SymbolLocator(language, core_path, "sfbench_rename_me", "fn", parts["rename"]),
            SymbolLocator(language, core_path, "SfBenchWorker", "struct", "type SfBenchWorker struct{}"),
            SymbolLocator(language, core_path, "run", "method", "func (SfBenchWorker) run() string {\n\treturn sfbench_rename_me()\n}"),
            SymbolLocator(language, core_path, "sfbench_entry", "fn", parts["entry"]),
            SymbolLocator(language, core_path, "sfbench_unused", "fn", parts["unused"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_one", "fn", parts["user_one"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_two", "fn", parts["user_two"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_three", "fn", parts["user_three"]),
            SymbolLocator(language, core_path, "sfbench_mutable", "fn", parts["mutable"]),
            SymbolLocator(language, core_path, "sfbench_outside_literal", "fn", parts["outside"]),
            SymbolLocator(language, core_path, "sfbench_delete_me", "fn", parts["delete"]),
            SymbolLocator(language, core_path, "sfbench_nested", "method", "func (SfBenchInner) sfbench_nested() string {\n\treturn \"nested\"\n}"),
            SymbolLocator(language, f"{root}/src/cycle_a.go", "CycleA", "struct", "type CycleA struct {\n\tOther *CycleAliasB\n}"),
            SymbolLocator(language, f"{root}/src/cycle_b.go", "CycleB", "struct", "type CycleB struct {\n\tOther *CycleAliasA\n}"),
        ]
    )
    model.imports.extend(
        [
            ImportLocator(language, core_path, "stdlib:strings", "sfstrings", "import sfstrings \"strings\""),
            ImportLocator(language, test_path, core_path, None, "import _ \"example.invalid/sfbench/sfbench_fixture/go/src\""),
        ]
    )
    register_language_relations(model, language, core_path, protocol_path, test_path, parts)


def java_fixture(model: FixtureModel) -> None:
    language = "Java"
    root = "sfbench_fixture/java"
    protocol_path = f"{root}/src/SfBenchProtocol.java"
    core_path = f"{root}/src/SfBenchCore.java"
    protocol = "interface SfBenchProtocol {\n    String run();\n}"
    parts = {
        "leaf": "    static String sfbench_leaf() {\n        return \"leaf\";\n    }",
        "mid": "    static String sfbench_mid() {\n        return sfbench_leaf();\n    }",
        "rename": "    static String sfbench_rename_me() {\n        return \"rename\";\n    }",
        "worker": "    static final class SfBenchWorker implements SfBenchProtocol {\n        public String run() {\n            return sfbench_rename_me();\n        }\n    }",
        "entry": "    static String sfbench_entry() {\n        SfBenchWorker worker = new SfBenchWorker();\n        return sfbench_mid() + \":\" + worker.run();\n    }",
        "unused": "    static String sfbench_unused() {\n        return \"unused\";\n    }",
        "user_one": "    static String sfbench_rename_user_one() {\n        return sfbench_rename_me();\n    }",
        "user_two": "    static String sfbench_rename_user_two() {\n        return sfbench_rename_me();\n    }",
        "user_three": "    static String sfbench_rename_user_three() {\n        return sfbench_rename_me();\n    }",
        "mutable": f"    static String[] sfbench_mutable() {{\n        String first = \"{MARKERS['edit_old']}\";\n        String unicodeInside = \"žarek λ\";\n        String second = \"{MARKERS['edit_old']}\";\n        return new String[] {{first, unicodeInside, second}};\n    }}",
        "outside": f"    static String sfbench_outside_literal() {{\n        return \"{MARKERS['edit_old']}\";\n    }}",
        "delete": "    static String sfbench_delete_me() {\n        return \"delete\";\n    }",
        "nested": "    static final class SfBenchOuter {\n        static final class Inner {\n            String sfbench_nested() {\n                return \"nested\";\n            }\n        }\n    }",
    }
    header = f"final class SfBenchCore {{\n    static final String SF_BENCH_SOURCE_MARKER = \"{MARKERS['source']}\";\n    static final String SF_BENCH_UNICODE_BEFORE = \"naïve λ\";"
    footer = "    // Non-code mention: sfbench_rename_me\n    static final String SF_BENCH_RENAME_TEXT = \"sfbench_rename_me\";\n}"
    add_file(model.files, protocol_path, protocol + "\n")
    add_file(model.files, core_path, assemble(header, parts.values(), footer))
    cycle_a = "final class CycleA {\n    CycleB other;\n}\n"
    cycle_b = "final class CycleB {\n    CycleA other;\n}\n"
    add_file(model.files, f"{root}/src/CycleA.java", cycle_a)
    add_file(model.files, f"{root}/src/CycleB.java", cycle_b)
    test_path = f"{root}/tests/SfBenchCoreTest.java"
    add_file(model.files, test_path, f"final class SfBenchCoreTest {{\n    static final String TEST_MARKER = \"{MARKERS['test']}\";\n\n    static String exercise() {{\n        return SfBenchCore.sfbench_entry();\n    }}\n}}\n")
    duplicate = "final class Duplicate {\n    static String sfbench_duplicate() {\n        return \"duplicate\";\n    }\n}\n"
    for directory in ("a", "b"):
        duplicate_path = f"{root}/duplicates/{directory}/Duplicate.java"
        add_file(model.files, duplicate_path, duplicate)
        model.symbols.append(SymbolLocator(language, duplicate_path, "sfbench_duplicate", "method", "    static String sfbench_duplicate() {\n        return \"duplicate\";\n    }"))
    model.symbols.extend(
        [
            SymbolLocator(language, protocol_path, "SfBenchProtocol", "interface", protocol),
            SymbolLocator(language, core_path, "sfbench_leaf", "method", parts["leaf"]),
            SymbolLocator(language, core_path, "sfbench_mid", "method", parts["mid"]),
            SymbolLocator(language, core_path, "sfbench_rename_me", "method", parts["rename"]),
            SymbolLocator(language, core_path, "SfBenchWorker", "class", parts["worker"]),
            SymbolLocator(language, core_path, "run", "method", "        public String run() {\n            return sfbench_rename_me();\n        }"),
            SymbolLocator(language, core_path, "sfbench_entry", "method", parts["entry"]),
            SymbolLocator(language, core_path, "sfbench_unused", "method", parts["unused"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_one", "method", parts["user_one"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_two", "method", parts["user_two"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_three", "method", parts["user_three"]),
            SymbolLocator(language, core_path, "sfbench_mutable", "method", parts["mutable"]),
            SymbolLocator(language, core_path, "sfbench_outside_literal", "method", parts["outside"]),
            SymbolLocator(language, core_path, "sfbench_delete_me", "method", parts["delete"]),
            SymbolLocator(language, core_path, "sfbench_nested", "method", "            String sfbench_nested() {\n                return \"nested\";\n            }"),
            SymbolLocator(language, f"{root}/src/CycleA.java", "CycleA", "class", cycle_a.strip("\n")),
            SymbolLocator(language, f"{root}/src/CycleB.java", "CycleB", "class", cycle_b.strip("\n")),
        ]
    )
    register_language_relations(model, language, core_path, protocol_path, test_path, parts)


def cpp_fixture(model: FixtureModel) -> None:
    language = "C++"
    root = "sfbench_fixture/cpp"
    protocol_path = f"{root}/src/protocol.hpp"
    core_path = f"{root}/src/core.cpp"
    protocol = "struct SfBenchProtocol {\n  virtual ~SfBenchProtocol() = default;\n  virtual std::string run() const = 0;\n};"
    parts = {
        "leaf": "std::string sfbench_leaf() {\n  return \"leaf\";\n}",
        "mid": "std::string sfbench_mid() {\n  return sfbench_leaf();\n}",
        "rename": "std::string sfbench_rename_me() {\n  return \"rename\";\n}",
        "worker": "struct SfBenchWorker final : SfBenchProtocol {\n  std::string run() const override {\n    return sfbench_rename_me();\n  }\n};",
        "entry": "std::string sfbench_entry() {\n  SfBenchWorker worker;\n  return sfbench_mid() + \":\" + worker.run();\n}",
        "unused": "std::string sfbench_unused() {\n  return \"unused\";\n}",
        "user_one": "std::string sfbench_rename_user_one() {\n  return sfbench_rename_me();\n}",
        "user_two": "std::string sfbench_rename_user_two() {\n  return sfbench_rename_me();\n}",
        "user_three": "std::string sfbench_rename_user_three() {\n  return sfbench_rename_me();\n}",
        "mutable": f"std::array<std::string, 3> sfbench_mutable() {{\n  const std::string first = \"{MARKERS['edit_old']}\";\n  const std::string unicode_inside = \"žarek λ\";\n  const std::string second = \"{MARKERS['edit_old']}\";\n  return {{first, unicode_inside, second}};\n}}",
        "outside": f"std::string sfbench_outside_literal() {{\n  return \"{MARKERS['edit_old']}\";\n}}",
        "delete": "std::string sfbench_delete_me() {\n  return \"delete\";\n}",
        "nested": "struct SfBenchOuter {\n  struct Inner {\n    std::string sfbench_nested() const {\n      return \"nested\";\n    }\n  };\n};",
    }
    header = f"#include \"protocol.hpp\"\n\n#include <array>\n#include <string>\n\nconst std::string SF_BENCH_SOURCE_MARKER = \"{MARKERS['source']}\";\nconst std::string SF_BENCH_UNICODE_BEFORE = \"naïve λ\";"
    footer = "// Non-code mention: sfbench_rename_me\nconst std::string SF_BENCH_RENAME_TEXT = \"sfbench_rename_me\";"
    add_file(model.files, protocol_path, "#pragma once\n#include <string>\n\n" + protocol + "\n")
    add_file(model.files, core_path, assemble(header, parts.values(), footer))
    cycle_a = "#pragma once\n#include \"cycle_b.hpp\"\nusing OtherB = CycleB;\nstruct CycleA { OtherB* other; };\n"
    cycle_b = "#pragma once\n#include \"cycle_a.hpp\"\nusing OtherA = CycleA;\nstruct CycleB { OtherA* other; };\n"
    add_file(model.files, f"{root}/src/cycle_a.hpp", cycle_a)
    add_file(model.files, f"{root}/src/cycle_b.hpp", cycle_b)
    test_path = f"{root}/tests/test_core.cpp"
    add_file(model.files, test_path, f"#include \"../src/core.cpp\"\nconst char* TEST_MARKER = \"{MARKERS['test']}\";\n")
    duplicate = "std::string sfbench_duplicate() {\n  return \"duplicate\";\n}\n"
    for directory in ("a", "b"):
        duplicate_path = f"{root}/duplicates/{directory}/duplicate.cpp"
        add_file(model.files, duplicate_path, duplicate)
        model.symbols.append(SymbolLocator(language, duplicate_path, "sfbench_duplicate", "fn", duplicate.strip("\n")))
    model.symbols.extend(
        [
            SymbolLocator(language, protocol_path, "SfBenchProtocol", "struct", protocol),
            SymbolLocator(language, core_path, "sfbench_leaf", "fn", parts["leaf"]),
            SymbolLocator(language, core_path, "sfbench_mid", "fn", parts["mid"]),
            SymbolLocator(language, core_path, "sfbench_rename_me", "fn", parts["rename"]),
            SymbolLocator(language, core_path, "SfBenchWorker", "struct", parts["worker"]),
            SymbolLocator(language, core_path, "run", "method", "  std::string run() const override {\n    return sfbench_rename_me();\n  }"),
            SymbolLocator(language, core_path, "sfbench_entry", "fn", parts["entry"]),
            SymbolLocator(language, core_path, "sfbench_unused", "fn", parts["unused"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_one", "fn", parts["user_one"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_two", "fn", parts["user_two"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_three", "fn", parts["user_three"]),
            SymbolLocator(language, core_path, "sfbench_mutable", "fn", parts["mutable"]),
            SymbolLocator(language, core_path, "sfbench_outside_literal", "fn", parts["outside"]),
            SymbolLocator(language, core_path, "sfbench_delete_me", "fn", parts["delete"]),
            SymbolLocator(language, core_path, "sfbench_nested", "method", "    std::string sfbench_nested() const {\n      return \"nested\";\n    }"),
            SymbolLocator(language, f"{root}/src/cycle_a.hpp", "CycleA", "struct", "struct CycleA { OtherB* other; };"),
            SymbolLocator(language, f"{root}/src/cycle_b.hpp", "CycleB", "struct", "struct CycleB { OtherA* other; };"),
        ]
    )
    model.imports.extend(
        [
            ImportLocator(language, core_path, protocol_path, None, "#include \"protocol.hpp\""),
            ImportLocator(language, f"{root}/src/cycle_a.hpp", f"{root}/src/cycle_b.hpp", "OtherB", "#include \"cycle_b.hpp\""),
            ImportLocator(language, f"{root}/src/cycle_b.hpp", f"{root}/src/cycle_a.hpp", "OtherA", "#include \"cycle_a.hpp\""),
            ImportLocator(language, test_path, core_path, None, "#include \"../src/core.cpp\""),
        ]
    )
    register_language_relations(model, language, core_path, protocol_path, test_path, parts)


def perl_fixture(model: FixtureModel) -> None:
    language = "Perl"
    root = "sfbench_fixture/perl"
    protocol_path = f"{root}/src/SfBenchProtocol.pm"
    core_path = f"{root}/src/SfBenchCore.pm"
    protocol = "package SfBenchProtocol;\nuse strict;\nuse warnings;\n\nsub run { die \"abstract\" }\n\n1;"
    parts = {
        "leaf": "sub sfbench_leaf {\n    return \"leaf\";\n}",
        "mid": "sub sfbench_mid {\n    return sfbench_leaf();\n}",
        "rename": "sub sfbench_rename_me {\n    return \"rename\";\n}",
        "worker": "package SfBenchWorker;\nour @ISA = ('SfBenchProtocol');\n\nsub run {\n    return SfBenchCore::sfbench_rename_me();\n}\n\npackage SfBenchCore;",
        "entry": "sub sfbench_entry {\n    my $worker = bless {}, 'SfBenchWorker';\n    return sfbench_mid() . ':' . $worker->run();\n}",
        "unused": "sub sfbench_unused {\n    return \"unused\";\n}",
        "user_one": "sub sfbench_rename_user_one {\n    return sfbench_rename_me();\n}",
        "user_two": "sub sfbench_rename_user_two {\n    return sfbench_rename_me();\n}",
        "user_three": "sub sfbench_rename_user_three {\n    return sfbench_rename_me();\n}",
        "mutable": f"sub sfbench_mutable {{\n    my $first = \"{MARKERS['edit_old']}\";\n    my $unicode_inside = \"žarek λ\";\n    my $second = \"{MARKERS['edit_old']}\";\n    return ($first, $unicode_inside, $second);\n}}",
        "outside": f"sub sfbench_outside_literal {{\n    return \"{MARKERS['edit_old']}\";\n}}",
        "delete": "sub sfbench_delete_me {\n    return \"delete\";\n}",
        "nested": "package SfBenchOuter::Inner;\nsub sfbench_nested { return \"nested\"; }\npackage SfBenchCore;",
    }
    header = f"package SfBenchCore;\nuse strict;\nuse warnings;\nuse utf8;\nrequire './SfBenchProtocol.pm';\n\nour $SF_BENCH_SOURCE_MARKER = \"{MARKERS['source']}\";\nour $SF_BENCH_UNICODE_BEFORE = \"naïve λ\";"
    footer = "# Non-code mention: sfbench_rename_me\nour $SF_BENCH_RENAME_TEXT = \"sfbench_rename_me\";\n\n1;"
    add_file(model.files, protocol_path, protocol + "\n")
    add_file(model.files, core_path, assemble(header, parts.values(), footer))
    cycle_a = "package CycleA;\nuse CycleB ();\nsub new { bless { other => undef }, shift }\n1;\n"
    cycle_b = "package CycleB;\nuse CycleA ();\nsub new { bless { other => undef }, shift }\n1;\n"
    add_file(model.files, f"{root}/src/CycleA.pm", cycle_a)
    add_file(model.files, f"{root}/src/CycleB.pm", cycle_b)
    test_path = f"{root}/tests/test_core.t"
    add_file(model.files, test_path, f"use strict;\nuse warnings;\nrequire '../src/SfBenchCore.pm';\nmy $test_marker = \"{MARKERS['test']}\";\nprint $test_marker . SfBenchCore::sfbench_entry();\n")
    duplicate = "package Duplicate;\nsub sfbench_duplicate { return \"duplicate\"; }\n1;\n"
    for directory in ("a", "b"):
        duplicate_path = f"{root}/duplicates/{directory}/duplicate.pm"
        add_file(model.files, duplicate_path, duplicate)
        model.symbols.append(SymbolLocator(language, duplicate_path, "sfbench_duplicate", "fn", "sub sfbench_duplicate { return \"duplicate\"; }"))
    model.symbols.extend(
        [
            SymbolLocator(language, protocol_path, "SfBenchProtocol", "module", "package SfBenchProtocol;"),
            SymbolLocator(language, core_path, "sfbench_leaf", "fn", parts["leaf"]),
            SymbolLocator(language, core_path, "sfbench_mid", "fn", parts["mid"]),
            SymbolLocator(language, core_path, "sfbench_rename_me", "fn", parts["rename"]),
            SymbolLocator(language, core_path, "SfBenchWorker", "module", "package SfBenchWorker;"),
            SymbolLocator(language, core_path, "run", "fn", "sub run {\n    return SfBenchCore::sfbench_rename_me();\n}"),
            SymbolLocator(language, core_path, "sfbench_entry", "fn", parts["entry"]),
            SymbolLocator(language, core_path, "sfbench_unused", "fn", parts["unused"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_one", "fn", parts["user_one"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_two", "fn", parts["user_two"]),
            SymbolLocator(language, core_path, "sfbench_rename_user_three", "fn", parts["user_three"]),
            SymbolLocator(language, core_path, "sfbench_mutable", "fn", parts["mutable"]),
            SymbolLocator(language, core_path, "sfbench_outside_literal", "fn", parts["outside"]),
            SymbolLocator(language, core_path, "sfbench_delete_me", "fn", parts["delete"]),
            SymbolLocator(language, core_path, "sfbench_nested", "fn", "sub sfbench_nested { return \"nested\"; }"),
            SymbolLocator(language, f"{root}/src/CycleA.pm", "CycleA", "module", "package CycleA;"),
            SymbolLocator(language, f"{root}/src/CycleB.pm", "CycleB", "module", "package CycleB;"),
        ]
    )
    model.imports.extend(
        [
            ImportLocator(language, core_path, protocol_path, None, "require './SfBenchProtocol.pm';"),
            ImportLocator(language, f"{root}/src/CycleA.pm", f"{root}/src/CycleB.pm", None, "use CycleB ();"),
            ImportLocator(language, f"{root}/src/CycleB.pm", f"{root}/src/CycleA.pm", None, "use CycleA ();"),
            ImportLocator(language, test_path, core_path, None, "require '../src/SfBenchCore.pm';"),
        ]
    )
    register_language_relations(model, language, core_path, protocol_path, test_path, parts)


def register_language_relations(
    model: FixtureModel,
    language: str,
    core_path: str,
    protocol_path: str,
    test_path: str,
    parts: dict[str, str],
) -> None:
    model.call_edges[language] = [
        ("sfbench_entry", "sfbench_mid"),
        ("sfbench_entry", "SfBenchWorker.run"),
        ("sfbench_mid", "sfbench_leaf"),
        ("SfBenchWorker.run", "sfbench_rename_me"),
        ("sfbench_rename_user_one", "sfbench_rename_me"),
        ("sfbench_rename_user_two", "sfbench_rename_me"),
        ("sfbench_rename_user_three", "sfbench_rename_me"),
    ]
    model.reference_callers[language] = [
        "run",
        "sfbench_rename_user_one",
        "sfbench_rename_user_two",
        "sfbench_rename_user_three",
    ]
    model.implementors[language] = {
        "protocol": "SfBenchProtocol",
        "implementor": "SfBenchWorker",
        "protocol_path": protocol_path,
        "implementor_path": core_path,
    }
    model.dependents[language] = {
        protocol_path: [core_path],
        core_path: [test_path],
    }
    model.mutation_bodies[language] = {
        "leaf_original": parts["leaf"],
        "leaf_replacement": replacement_leaf(language),
        "insert_after_entry": inserted_symbol(language),
    }


def replacement_leaf(language: str) -> str:
    return {
        "Rust": "pub fn sfbench_leaf() -> String {\n    \"leaf-v2\".to_owned()\n}",
        "Python": "def sfbench_leaf() -> str:\n    return \"leaf-v2\"",
        "TypeScript": "export function sfbench_leaf(): string {\n  return \"leaf-v2\";\n}",
        "Go": "func sfbench_leaf() string {\n\treturn \"leaf-v2\"\n}",
        "Java": "    static String sfbench_leaf() {\n        return \"leaf-v2\";\n    }",
        "C++": "std::string sfbench_leaf() {\n  return \"leaf-v2\";\n}",
        "Perl": "sub sfbench_leaf {\n    return \"leaf-v2\";\n}",
    }[language]


def inserted_symbol(language: str) -> str:
    return {
        "Rust": "pub fn sfbench_inserted() -> &'static str {\n    \"inserted\"\n}",
        "Python": "def sfbench_inserted() -> str:\n    return \"inserted\"",
        "TypeScript": "export function sfbench_inserted(): string {\n  return \"inserted\";\n}",
        "Go": "func sfbench_inserted() string {\n\treturn \"inserted\"\n}",
        "Java": "    static String sfbench_inserted() {\n        return \"inserted\";\n    }",
        "C++": "std::string sfbench_inserted() {\n  return \"inserted\";\n}",
        "Perl": "sub sfbench_inserted {\n    return \"inserted\";\n}",
    }[language]


def common_fixture(model: FixtureModel) -> None:
    add_file(
        model.files,
        ".gitattributes",
        "sfbench_fixture/exact/** -text\nsfbench_fixture/generated/large_generated.py -text\nsfbench_fixture/configs/bom_crlf_unicode.json -text\n",
    )
    add_file(model.files, ".gitignore", ".symforge/\nsfbench_fixture/ignored/\n")
    add_file(model.files, "README.md", "# SFBENCH deterministic control repository\n")
    add_file(model.files, "go.mod", "module example.invalid/sfbench\n\ngo 1.23\n")
    add_file(model.files, "sfbench_fixture/filter_cases/source/marker.py", f"MARKER = \"{MARKERS['source']}\"\n")
    add_file(model.files, "sfbench_fixture/filter_cases/tests/marker.py", f"MARKER = \"{MARKERS['test']}\"\n")
    add_file(model.files, "sfbench_fixture/filter_cases/generated/marker.py", f"MARKER = \"{MARKERS['generated']}\"\n")
    add_file(model.files, "sfbench_fixture/filter_cases/vendor/marker.py", f"MARKER = \"{MARKERS['vendor']}\"\n")
    add_file(model.files, "sfbench_fixture/ignored/marker.py", "IGNORED_FIXTURE = True\n")
    add_file(model.files, ".claude/gsd-tools/marker.py", f"MARKER = \"{MARKERS['personal']}\"\n")
    add_file(model.files, "sfbench_fixture/configs/valid.json", "{\n  \"enabled\": true,\n  \"name\": \"žarek\"\n}\n")
    add_file(model.files, "sfbench_fixture/configs/malformed.json", "{\n  \"enabled\": true,\n  \"items\": [1, 2,]\n}\n")
    add_file(model.files, "sfbench_fixture/configs/valid.toml", "enabled = true\nname = \"žarek\"\n")
    add_file(model.files, "sfbench_fixture/configs/malformed.toml", "enabled = true\nname = \"unterminated\ncount = 3\n")
    add_file(model.files, "sfbench_fixture/configs/valid.yaml", "enabled: true\nname: žarek\nitems:\n  - one\n")
    add_file(model.files, "sfbench_fixture/configs/malformed.yaml", "enabled: true\nitems:\n  - one\n  - [two, three\n")
    add_file(model.files, "sfbench_fixture/configs/bom_crlf_unicode.json", b"\xef\xbb\xbf{\r\n  \"name\": \"\xc5\xbearek\"\r\n}\r\n")
    add_file(model.files, "sfbench_fixture/exact/lf_source.py", "LF_VALUE = \"naïve λ\"\n")
    add_file(model.files, "sfbench_fixture/exact/crlf_source.py", b"CRLF_VALUE = \"na\xc3\xafve \xce\xbb\"\r\n")
    add_file(model.files, "sfbench_fixture/exact/no_final_newline.py", "NO_FINAL_NEWLINE = \"žarek\"")
    binary = bytes(range(256)) * 4 + b"\x00SFBENCH-BINARY\x00" + bytes(reversed(range(256)))
    add_file(model.files, "sfbench_fixture/exact/deterministic.bin", binary)
    large_lines = ["# SF_BENCH_LARGE_START_9F31", f"# {MARKERS['generated']}"]
    for number in range(1400):
        if number == 700:
            large_lines.append("# SF_BENCH_LARGE_MIDDLE_9F31")
        large_lines.extend(
            [
                f"def sfbench_bulk_result_{number:04d}():",
                f"    return \"{MARKERS['ccr']}:{number:04d}\"",
                "",
            ]
        )
    large_lines.append("# SF_BENCH_LARGE_END_9F31")
    large_source = "\n".join(large_lines) + "\n"
    if len(utf8(large_source)) <= 60 * 1024:
        raise RuntimeError("large fixture did not exceed 60 KiB")
    add_file(model.files, "sfbench_fixture/generated/large_generated.py", large_source)
    history_source = block(
        """
        COCHANGE_EVENTS = []

        def sfbench_history_modified() -> str:
            return "v1"

        def sfbench_history_removed() -> str:
            return "remove-in-commit-six"
        """
    ) + "\n"
    history_test = "from sfbench_fixture.history.source import sfbench_history_modified\n\nCOCHANGE_TEST_EVENTS = []\n\ndef test_history_modified() -> None:\n    assert sfbench_history_modified() == \"v1\"\n"
    add_file(model.files, "sfbench_fixture/history/source.py", history_source)
    add_file(model.files, "sfbench_fixture/history/test_source.py", history_test)
    add_file(model.files, "docs/history.txt", "unrelated-history-base\n")
    worktree_files = {
        "staged.py": "STATE = \"base\"\n",
        "unstaged.py": "STATE = \"base\"\n",
        "rename_from.py": "STATE = \"rename-base\"\n",
        "deleted.py": "STATE = \"delete-base\"\n",
        "stale_target.py": "STALE_STATE = \"before-fetch\"\n",
    }
    for name, value in worktree_files.items():
        add_file(model.files, f"sfbench_fixture/worktree/{name}", value)


def build_model() -> FixtureModel:
    model = FixtureModel({}, [], [], {}, {}, {}, {}, {})
    rust_fixture(model)
    python_fixture(model)
    typescript_fixture(model)
    go_fixture(model)
    java_fixture(model)
    cpp_fixture(model)
    perl_fixture(model)
    common_fixture(model)
    return model


def write_file(root: Path, relative: str, data: bytes) -> None:
    path = root.joinpath(*relative.split("/"))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def safe_git_env(global_config: Path, timestamp: str | None = None) -> dict[str, str]:
    allowed = ("PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "COMSPEC", "TEMP", "TMP")
    env = {name: os.environ[name] for name in allowed if name in os.environ}
    env.update(
        {
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": str(global_config),
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_AUTHOR_NAME": AUTHOR_NAME,
            "GIT_AUTHOR_EMAIL": AUTHOR_EMAIL,
            "GIT_COMMITTER_NAME": AUTHOR_NAME,
            "GIT_COMMITTER_EMAIL": AUTHOR_EMAIL,
            "LC_ALL": "C",
            "TZ": "UTC",
        }
    )
    if timestamp is not None:
        env["GIT_AUTHOR_DATE"] = timestamp
        env["GIT_COMMITTER_DATE"] = timestamp
    return env


def git(repo: Path, global_config: Path, *arguments: str, timestamp: str | None = None) -> bytes:
    command = ["git", *arguments]
    result = subprocess.run(
        command,
        cwd=repo,
        env=safe_git_env(global_config, timestamp),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"git command failed ({' '.join(command)}): {detail}")
    return result.stdout


def replace_once(path: Path, old: bytes, new: bytes) -> None:
    source = path.read_bytes()
    if source.count(old) != 1:
        raise RuntimeError(f"expected one replacement target in {path.name}")
    path.write_bytes(source.replace(old, new, 1))


def append_bytes(path: Path, value: bytes) -> None:
    path.write_bytes(path.read_bytes() + value)


def commit_all(
    repo: Path,
    global_config: Path,
    message: str,
    timestamp: str,
    expected_paths: list[str],
    force_paths: tuple[str, ...] = (),
) -> str:
    git(repo, global_config, "add", "--all")
    if force_paths:
        git(repo, global_config, "add", "--force", "--", *force_paths)
    actual = git(repo, global_config, "diff", "--cached", "--name-only", "--format=").decode("utf-8").splitlines()
    if sorted(actual) != sorted(expected_paths):
        raise RuntimeError(f"staged path mismatch for commit {message!r}")
    git(repo, global_config, "commit", "--no-verify", "--no-gpg-sign", "-m", message, timestamp=timestamp)
    return git(repo, global_config, "rev-parse", "HEAD").decode("ascii").strip()


def history_timestamp(index: int) -> str:
    return f"2026-01-{index:02d}T00:00:00+00:00"


def build_repository(repo: Path, model: FixtureModel, global_config: Path, template_dir: Path) -> list[dict[str, object]]:
    repo.mkdir()
    git(repo, global_config, "init", f"--template={template_dir}", "--quiet")
    git(repo, global_config, "symbolic-ref", "HEAD", "refs/heads/trunk")
    settings = {
        "core.autocrlf": "false",
        "core.eol": "lf",
        "core.filemode": "false",
        "core.longpaths": "true",
        "commit.gpgSign": "false",
        "tag.gpgSign": "false",
        "user.useConfigOnly": "true",
    }
    for key, value in settings.items():
        git(repo, global_config, "config", "--local", key, value)
    for path, data in sorted(model.files.items()):
        write_file(repo, path, data)

    commits: list[dict[str, object]] = []
    commit_paths = sorted(model.files)
    commit_id = commit_all(
        repo,
        global_config,
        "fixture: initialize controlled graph",
        history_timestamp(1),
        commit_paths,
        ("sfbench_fixture/ignored/marker.py",),
    )
    commits.append(commit_record(1, commit_id, "fixture: initialize controlled graph", commit_paths, [], False))

    source = repo / "sfbench_fixture/history/source.py"
    test = repo / "sfbench_fixture/history/test_source.py"
    append_bytes(source, b"\nCOCHANGE_EVENTS.append(\"one\")\n")
    append_bytes(test, b"\nCOCHANGE_TEST_EVENTS.append(\"one\")\n")
    paths = ["sfbench_fixture/history/source.py", "sfbench_fixture/history/test_source.py"]
    commit_id = commit_all(repo, global_config, "history: source test cochange one", history_timestamp(2), paths)
    commits.append(commit_record(2, commit_id, "history: source test cochange one", paths, [], True))

    history_doc = repo / "docs/history.txt"
    append_bytes(history_doc, b"unrelated-change-one\n")
    paths = ["docs/history.txt"]
    commit_id = commit_all(repo, global_config, "docs: unrelated history change", history_timestamp(3), paths)
    commits.append(commit_record(3, commit_id, "docs: unrelated history change", paths, [], False))

    append_bytes(source, b"\ndef sfbench_history_added() -> str:\n    return \"added\"\n")
    append_bytes(test, b"\ndef test_history_added() -> None:\n    assert True\n")
    paths = ["sfbench_fixture/history/source.py", "sfbench_fixture/history/test_source.py"]
    commit_id = commit_all(repo, global_config, "history: add symbol with cochange two", history_timestamp(4), paths)
    commits.append(commit_record(4, commit_id, "history: add symbol with cochange two", paths, [{"name": "sfbench_history_added", "change": "added", "kind": "fn"}], True))

    replace_once(source, b'return "v1"', b'return "v2"')
    replace_once(test, b'== "v1"', b'== "v2"')
    paths = ["sfbench_fixture/history/source.py", "sfbench_fixture/history/test_source.py"]
    commit_id = commit_all(repo, global_config, "history: modify symbol with cochange three", history_timestamp(5), paths)
    commits.append(commit_record(5, commit_id, "history: modify symbol with cochange three", paths, [{"name": "sfbench_history_modified", "change": "modified", "kind": "fn"}], True))

    removed = b'\ndef sfbench_history_removed() -> str:\n    return "remove-in-commit-six"\n'
    replace_once(source, removed, b"")
    append_bytes(history_doc, b"unrelated-change-two\n")
    paths = ["docs/history.txt", "sfbench_fixture/history/source.py"]
    commit_id = commit_all(repo, global_config, "history: remove symbol and unrelated change", history_timestamp(6), paths)
    commits.append(commit_record(6, commit_id, "history: remove symbol and unrelated change", paths, [{"name": "sfbench_history_removed", "change": "removed", "kind": "fn"}], False))

    append_bytes(source, b"\nCOCHANGE_EVENTS.append(\"four\")\n")
    append_bytes(test, b"\nCOCHANGE_TEST_EVENTS.append(\"four\")\n")
    paths = ["sfbench_fixture/history/source.py", "sfbench_fixture/history/test_source.py"]
    commit_id = commit_all(repo, global_config, "history: source test cochange four", history_timestamp(7), paths)
    commits.append(commit_record(7, commit_id, "history: source test cochange four", paths, [], True))

    git(repo, global_config, "update-ref", "refs/bench/initial", str(commits[0]["commit"]))
    git(repo, global_config, "update-ref", "refs/bench/cochange-2", str(commits[3]["commit"]))
    git(repo, global_config, "update-ref", "refs/bench/pre-removal", str(commits[4]["commit"]))
    git(repo, global_config, "update-ref", "refs/tags/sfbench-v1", str(commits[6]["commit"]))
    head_refs = git(repo, global_config, "for-each-ref", "--format=%(refname)", "refs/heads").decode("utf-8").splitlines()
    if head_refs != ["refs/heads/trunk"]:
        raise RuntimeError(f"unexpected branch refs: {head_refs!r}")
    if git(repo, global_config, "status", "--porcelain=v1"):
        raise RuntimeError("clean control repository unexpectedly dirty")
    return commits


def commit_record(
    sequence: int,
    commit: str,
    message: str,
    changed_paths: list[str],
    symbol_changes: list[dict[str, str]],
    source_test_cochange: bool,
) -> dict[str, object]:
    return {
        "sequence": sequence,
        "commit": commit,
        "message": message,
        "timestamp": history_timestamp(sequence),
        "changed_paths": sorted(changed_paths),
        "symbol_changes": symbol_changes,
        "source_test_cochange": source_test_cochange,
    }


def prepare_dirty_repository(repo: Path, global_config: Path) -> dict[str, object]:
    fixture = repo / "sfbench_fixture/worktree"
    write_file(repo, "sfbench_fixture/worktree/staged.py", b'STATE = "staged"\n')
    write_file(repo, "sfbench_fixture/worktree/staged_added.py", b'STATE = "staged-added"\n')
    git(repo, global_config, "mv", "--", "sfbench_fixture/worktree/rename_from.py", "sfbench_fixture/worktree/rename_to.py")
    git(repo, global_config, "add", "--", "sfbench_fixture/worktree/staged.py", "sfbench_fixture/worktree/staged_added.py")
    write_file(repo, "sfbench_fixture/worktree/unstaged.py", b'STATE = "unstaged"\n')
    (fixture / "deleted.py").unlink()
    write_file(repo, "sfbench_fixture/worktree/untracked.py", b'STATE = "untracked"\n')
    expected_status = sorted(
        [
            "M  sfbench_fixture/worktree/staged.py",
            "A  sfbench_fixture/worktree/staged_added.py",
            "R  sfbench_fixture/worktree/rename_from.py -> sfbench_fixture/worktree/rename_to.py",
            " M sfbench_fixture/worktree/unstaged.py",
            " D sfbench_fixture/worktree/deleted.py",
            "?? sfbench_fixture/worktree/untracked.py",
        ]
    )
    actual_status = sorted(git(repo, global_config, "status", "--porcelain=v1", "--untracked-files=all").decode("utf-8").splitlines())
    if actual_status != expected_status:
        raise RuntimeError(f"dirty worktree status mismatch: {actual_status!r}")
    staged_diff = git(repo, global_config, "diff", "--cached", "--binary", "--no-ext-diff")
    unstaged_diff = git(repo, global_config, "diff", "--binary", "--no-ext-diff")
    return {
        "status_porcelain_v1": expected_status,
        "staged": {
            "added": ["sfbench_fixture/worktree/staged_added.py"],
            "modified": ["sfbench_fixture/worktree/staged.py"],
            "renamed": [{"from": "sfbench_fixture/worktree/rename_from.py", "to": "sfbench_fixture/worktree/rename_to.py"}],
            "diff_sha256": sha256(staged_diff),
            "diff_bytes": len(staged_diff),
        },
        "unstaged": {
            "modified": ["sfbench_fixture/worktree/unstaged.py"],
            "deleted": ["sfbench_fixture/worktree/deleted.py"],
            "diff_sha256": sha256(unstaged_diff),
            "diff_bytes": len(unstaged_diff),
        },
        "untracked": ["sfbench_fixture/worktree/untracked.py"],
    }


def relative_files(root: Path) -> list[Path]:
    result: list[Path] = []
    for directory, directories, filenames in os.walk(root, followlinks=False):
        directories[:] = sorted(name for name in directories if name != ".git" and not Path(directory, name).is_symlink())
        for filename in sorted(filenames):
            path = Path(directory, filename)
            if ".git" not in path.relative_to(root).parts and not path.is_symlink():
                result.append(path)
    return sorted(result, key=lambda path: path.relative_to(root).as_posix())


def line_ending(data: bytes) -> str:
    if b"\x00" in data:
        return "binary"
    crlf = data.count(b"\r\n")
    bare_lf = data.count(b"\n") - crlf
    if crlf and bare_lf:
        return "mixed"
    if crlf:
        return "CRLF"
    if bare_lf:
        return "LF"
    return "none"


def file_inventory(root: Path) -> dict[str, dict[str, object]]:
    result: dict[str, dict[str, object]] = {}
    for path in relative_files(root):
        data = path.read_bytes()
        relative = path.relative_to(root).as_posix()
        result[relative] = {
            "sha256": sha256(data),
            "bytes": len(data),
            "line_ending": line_ending(data),
            "final_newline": data.endswith(b"\n"),
            "binary": b"\x00" in data,
        }
    return result


def inventory_tree_hash(inventory: dict[str, dict[str, object]]) -> str:
    digest = hashlib.sha256()
    for path, facts in sorted(inventory.items()):
        digest.update(path.encode("utf-8"))
        digest.update(b"\0")
        digest.update(str(facts["sha256"]).encode("ascii"))
        digest.update(b"\0")
        digest.update(str(facts["bytes"]).encode("ascii"))
        digest.update(b"\n")
    return digest.hexdigest()


def locate_unique(data: bytes, needle: bytes, label: str) -> tuple[int, int, int, int]:
    if data.count(needle) != 1:
        raise RuntimeError(f"oracle locator is not unique: {label}")
    start = data.index(needle)
    end = start + len(needle)
    start_line = data.count(b"\n", 0, start) + 1
    end_line = data.count(b"\n", 0, max(start, end - 1)) + 1
    return start, end, start_line, end_line


def symbol_oracle(repo: Path, model: FixtureModel) -> dict[str, list[dict[str, object]]]:
    result: dict[str, list[dict[str, object]]] = {}
    for item in model.symbols:
        data = (repo / item.path).read_bytes()
        source = utf8(item.source)
        start, end, start_line, end_line = locate_unique(data, source, f"{item.path}::{item.name}")
        result.setdefault(item.language, []).append(
            {
                "name": item.name,
                "kind": item.kind,
                "path": item.path,
                "start_line": start_line,
                "end_line": end_line,
                "start_byte": start,
                "end_byte_exclusive": end,
                "source_sha256": sha256(source),
            }
        )
    for values in result.values():
        values.sort(key=lambda value: (str(value["path"]), int(value["start_byte"]), str(value["name"])))
    return result


def import_oracle(repo: Path, model: FixtureModel) -> dict[str, list[dict[str, object]]]:
    result: dict[str, list[dict[str, object]]] = {}
    for item in model.imports:
        data = (repo / item.from_path).read_bytes()
        source = utf8(item.source)
        start, end, start_line, end_line = locate_unique(data, source, f"import:{item.from_path}")
        result.setdefault(item.language, []).append(
            {
                "from_path": item.from_path,
                "to_path": item.to_path,
                "alias": item.alias,
                "start_line": start_line,
                "end_line": end_line,
                "start_byte": start,
                "end_byte_exclusive": end,
            }
        )
    for values in result.values():
        values.sort(key=lambda value: (str(value["from_path"]), int(value["start_byte"])))
    return result


def dependent_file_oracle(model: FixtureModel) -> dict[str, dict[str, list[str]]]:
    result = {
        language: {path: list(dependents) for path, dependents in paths.items()}
        for language, paths in model.dependents.items()
    }
    for item in model.imports:
        if item.to_path.startswith("stdlib:"):
            continue
        values = result.setdefault(item.language, {}).setdefault(item.to_path, [])
        if item.from_path not in values:
            values.append(item.from_path)
    for paths in result.values():
        for dependents in paths.values():
            dependents.sort()
    return result


def all_occurrences(data: bytes, needle: bytes) -> list[int]:
    offsets: list[int] = []
    cursor = 0
    while True:
        found = data.find(needle, cursor)
        if found < 0:
            return offsets
        offsets.append(found)
        cursor = found + len(needle)


def reference_oracle(
    repo: Path,
    model: FixtureModel,
    symbols: dict[str, list[dict[str, object]]],
) -> dict[str, dict[str, object]]:
    result: dict[str, dict[str, object]] = {}
    needle = utf8("sfbench_rename_me")
    for language, caller_names in model.reference_callers.items():
        language_symbols = symbols[language]
        definition = next(value for value in language_symbols if value["name"] == "sfbench_rename_me")
        path = str(definition["path"])
        data = (repo / path).read_bytes()
        code: list[dict[str, object]] = []
        consumed = {int(definition["start_byte"]) + data[int(definition["start_byte"]):int(definition["end_byte_exclusive"])].index(needle)}
        for caller in caller_names:
            symbol = next(value for value in language_symbols if value["name"] == caller and value["path"] == path)
            start = int(symbol["start_byte"])
            end = int(symbol["end_byte_exclusive"])
            span = data[start:end]
            matches = all_occurrences(span, needle)
            if len(matches) != 1:
                raise RuntimeError(f"expected one rename reference in {language}::{caller}")
            offset = start + matches[0]
            consumed.add(offset)
            code.append(
                {
                    "caller": "SfBenchWorker.run" if caller == "run" else caller,
                    "path": path,
                    "line": data.count(b"\n", 0, offset) + 1,
                    "start_byte": offset,
                    "end_byte_exclusive": offset + len(needle),
                }
            )
        mentions = []
        for offset in all_occurrences(data, needle):
            if offset in consumed:
                continue
            mentions.append(
                {
                    "path": path,
                    "line": data.count(b"\n", 0, offset) + 1,
                    "start_byte": offset,
                    "end_byte_exclusive": offset + len(needle),
                }
            )
        if len(code) != 4 or len(mentions) != 2 or len(all_occurrences(data, needle)) != 7:
            raise RuntimeError(f"rename reference invariant failed for {language}")
        result[language] = {
            "target": "sfbench_rename_me",
            "definition": definition,
            "code_references": sorted(code, key=lambda value: int(value["start_byte"])),
            "comment_or_string_mentions": mentions,
            "expected_code_reference_count": 4,
            "expected_non_code_mention_count": 2,
        }
    return result


def marker_oracle(repo: Path) -> dict[str, dict[str, object]]:
    inventory = file_inventory(repo)
    result: dict[str, dict[str, object]] = {}
    for name, marker in MARKERS.items():
        marker_bytes = utf8(marker)
        paths = []
        count = 0
        for path in inventory:
            data = (repo / path).read_bytes()
            occurrences = data.count(marker_bytes)
            if occurrences:
                paths.append(path)
                count += occurrences
        result[name] = {"value": marker, "paths": sorted(paths), "occurrences": count}
    return result


def mutation_oracle(
    repo: Path,
    model: FixtureModel,
    symbols: dict[str, list[dict[str, object]]],
    references: dict[str, dict[str, object]],
) -> dict[str, object]:
    cases: dict[str, object] = {}
    old = utf8(MARKERS["edit_old"])
    new = utf8(MARKERS["edit_new"])
    for language, bodies in model.mutation_bodies.items():
        language_symbols = symbols[language]
        mutable = next(value for value in language_symbols if value["name"] == "sfbench_mutable")
        leaf = next(value for value in language_symbols if value["name"] == "sfbench_leaf")
        delete = next(value for value in language_symbols if value["name"] == "sfbench_delete_me")
        path = str(mutable["path"])
        before = (repo / path).read_bytes()
        mutable_start = int(mutable["start_byte"])
        mutable_end = int(mutable["end_byte_exclusive"])
        mutable_bytes = before[mutable_start:mutable_end]
        if mutable_bytes.count(old) != 2 or before.count(old) != 3:
            raise RuntimeError(f"edit literal invariant failed for {language}")
        edited = before[:mutable_start] + mutable_bytes.replace(old, new) + before[mutable_end:]
        leaf_start = int(leaf["start_byte"])
        leaf_end = int(leaf["end_byte_exclusive"])
        replaced = before[:leaf_start] + utf8(bodies["leaf_replacement"]) + before[leaf_end:]
        delete_start = int(delete["start_byte"])
        delete_end = int(delete["end_byte_exclusive"])
        delete_prefix = before[:delete_start]
        delete_suffix = before[delete_end:]
        if not delete_prefix.endswith(b"\n\n") or not delete_suffix.startswith(b"\n\n"):
            raise RuntimeError(f"delete symbol spacing invariant failed for {language}")
        deleted = delete_prefix + delete_suffix[2:]
        rename_target = b"sfbench_rename_me"
        rename_replacement = b"sfbench_renamed"
        definition = references[language]["definition"]
        definition_start = int(definition["start_byte"])
        definition_end = int(definition["end_byte_exclusive"])
        definition_offset = definition_start + before[definition_start:definition_end].index(rename_target)
        rename_offsets = [definition_offset]
        rename_offsets.extend(int(value["start_byte"]) for value in references[language]["code_references"])
        renamed = before
        for offset in sorted(rename_offsets, reverse=True):
            if renamed[offset : offset + len(rename_target)] != rename_target:
                raise RuntimeError(f"rename offset invariant failed for {language}")
            renamed = renamed[:offset] + rename_replacement + renamed[offset + len(rename_target):]
        if renamed.count(rename_target) != 2 or renamed.count(rename_replacement) != 5:
            raise RuntimeError(f"rename occurrence invariant failed for {language}")
        cases[language] = {
            "path": path,
            "edit_within": {
                "symbol": "sfbench_mutable",
                "old_text": MARKERS["edit_old"],
                "new_text": MARKERS["edit_new"],
                "replace_all": True,
                "inside_occurrences": 2,
                "outside_occurrences_unchanged": 1,
                "before_sha256": sha256(before),
                "after_sha256": sha256(edited),
                "diff_sha256": unified_diff_hash(path, before, edited),
            },
            "replace_symbol": {
                "symbol": "sfbench_leaf",
                "new_body": bodies["leaf_replacement"],
                "before_sha256": sha256(before),
                "after_sha256": sha256(replaced),
                "diff_sha256": unified_diff_hash(path, before, replaced),
            },
            "delete_symbol": {
                "symbol": "sfbench_delete_me",
                "before_sha256": sha256(before),
                "after_sha256": sha256(deleted),
                "diff_sha256": unified_diff_hash(path, before, deleted),
            },
            "rename_symbol": {
                "name": "sfbench_rename_me",
                "new_name": "sfbench_renamed",
                "changed_code_occurrences": 5,
                "unchanged_comment_or_string_mentions": 2,
                "before_sha256": sha256(before),
                "after_sha256": sha256(renamed),
                "diff_sha256": unified_diff_hash(path, before, renamed),
            },
            "insert_symbol": {
                "anchor": "sfbench_entry",
                "position": "after",
                "body": bodies["insert_after_entry"],
            },
        }
    stale_path = "sfbench_fixture/worktree/stale_target.py"
    before = (repo / stale_path).read_bytes()
    after = b'STALE_STATE = "after-fetch"\n'
    cases["stale_index"] = {
        "path": stale_path,
        "before_sha256": sha256(before),
        "after_sha256": sha256(after),
        "after_bytes_utf8": after.decode("utf-8"),
        "sequence": ["fetch", "write_after_bytes", "query_without_refresh", "query_with_force_refresh"],
    }
    return cases


def unified_diff_hash(path: str, before: bytes, after: bytes) -> str:
    before_lines = before.decode("utf-8").splitlines(keepends=True)
    after_lines = after.decode("utf-8").splitlines(keepends=True)
    diff = "".join(difflib.unified_diff(before_lines, after_lines, fromfile=f"a/{path}", tofile=f"b/{path}", lineterm="\n"))
    return sha256(utf8(diff))


def config_oracle(repo: Path) -> dict[str, dict[str, object]]:
    expectations: dict[str, tuple[bool, int | None]] = {
        "sfbench_fixture/configs/valid.json": (True, None),
        "sfbench_fixture/configs/malformed.json": (False, 3),
        "sfbench_fixture/configs/valid.toml": (True, None),
        "sfbench_fixture/configs/malformed.toml": (False, 2),
        "sfbench_fixture/configs/valid.yaml": (True, None),
        "sfbench_fixture/configs/malformed.yaml": (False, 4),
        "sfbench_fixture/configs/bom_crlf_unicode.json": (True, None),
    }
    result = {}
    for path, (valid, diagnostic_line) in expectations.items():
        data = (repo / path).read_bytes()
        result[path] = {
            "valid": valid,
            "diagnostic_line": diagnostic_line,
            "sha256": sha256(data),
            "bytes": len(data),
            "line_ending": line_ending(data),
        }
    json.loads((repo / "sfbench_fixture/configs/valid.json").read_text(encoding="utf-8"))
    try:
        json.loads((repo / "sfbench_fixture/configs/malformed.json").read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        if error.lineno != 3:
            raise RuntimeError("malformed JSON diagnostic moved") from error
    else:
        raise RuntimeError("malformed JSON unexpectedly parsed")
    tomllib.loads((repo / "sfbench_fixture/configs/valid.toml").read_text(encoding="utf-8"))
    try:
        tomllib.loads((repo / "sfbench_fixture/configs/malformed.toml").read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError:
        pass
    else:
        raise RuntimeError("malformed TOML unexpectedly parsed")
    return result


def create_non_git(root: Path) -> dict[str, object]:
    root.mkdir()
    write_file(root, "plain.py", b'def sfbench_non_git():\n    return "plain"\n')
    write_file(root, "nested/config.json", b'{"non_git": true}\n')
    inventory = file_inventory(root)
    return {"path": "non-git", "tree_sha256": inventory_tree_hash(inventory), "files": inventory}


def create_symlink_cohort(root: Path) -> dict[str, object]:
    root.mkdir()
    write_file(root, "target.txt", b"symlink-target\n")
    (root / "loop-a").mkdir()
    (root / "loop-b").mkdir()
    links = [
        (root / "file-link", "target.txt", False),
        (root / "loop-a" / "to-b", "../loop-b", True),
        (root / "loop-b" / "to-a", "../loop-a", True),
    ]
    created: list[Path] = []
    try:
        for path, target, is_directory in links:
            os.symlink(target, path, target_is_directory=is_directory)
            created.append(path)
    except OSError:
        for path in reversed(created):
            path.unlink()
        return {"path": "symlink-cohort", "supported": False, "links": []}
    return {
        "path": "symlink-cohort",
        "supported": True,
        "links": [
            {"path": path.relative_to(root).as_posix(), "target": target, "directory": is_directory}
            for path, target, is_directory in links
        ],
    }


def repository_refs(repo: Path, global_config: Path) -> dict[str, str]:
    refs = [
        "refs/heads/trunk",
        "refs/bench/initial",
        "refs/bench/cochange-2",
        "refs/bench/pre-removal",
        "refs/tags/sfbench-v1",
    ]
    return {name: git(repo, global_config, "rev-parse", name).decode("ascii").strip() for name in refs}


def write_oracle(
    output: Path,
    model: FixtureModel,
    commits: list[dict[str, object]],
    dirty: dict[str, object],
    non_git: dict[str, object],
    symlinks: dict[str, object],
    global_config: Path,
) -> dict[str, object]:
    clean_repo = output / "control-repo"
    mutation_repo = output / "mutation-repo"
    clean_inventory = file_inventory(clean_repo)
    dirty_inventory = file_inventory(mutation_repo)
    symbols = symbol_oracle(clean_repo, model)
    refs = repository_refs(clean_repo, global_config)
    imports = import_oracle(clean_repo, model)
    references = reference_oracle(clean_repo, model, symbols)
    head = str(commits[-1]["commit"])
    mutation_head = git(mutation_repo, global_config, "rev-parse", "HEAD").decode("ascii").strip()
    if mutation_head != head:
        raise RuntimeError("independent repository histories are not identical")
    exact_paths = [
        "sfbench_fixture/exact/lf_source.py",
        "sfbench_fixture/exact/crlf_source.py",
        "sfbench_fixture/exact/no_final_newline.py",
        "sfbench_fixture/exact/deterministic.bin",
        "sfbench_fixture/configs/bom_crlf_unicode.json",
    ]
    oracle: dict[str, object] = {
        "protocol": PROTOCOL_ID,
        "fixture": {
            "version": FIXTURE_VERSION,
            "frozen_at": "2026-07-12",
            "generator_sha256": sha256(Path(__file__).read_bytes()),
            "encoding": "UTF-8 unless a file is explicitly binary",
            "path_style": "POSIX-relative",
            "oracle_source": "fixture templates and Python hashlib; never SymForge output",
        },
        "paths": {
            "clean_repository": "control-repo",
            "mutation_repository": "mutation-repo",
            "non_git_directory": "non-git",
            "symlink_cohort": "symlink-cohort",
        },
        "repositories": {
            "clean": {
                "path": "control-repo",
                "branch": "trunk",
                "head": head,
                "main_ref_absent": True,
                "clean": True,
                "tree_sha256": inventory_tree_hash(clean_inventory),
                "file_count": len(clean_inventory),
                "source_bytes": sum(int(value["bytes"]) for value in clean_inventory.values()),
            },
            "mutation": {
                "path": "mutation-repo",
                "branch": "trunk",
                "head": mutation_head,
                "independent_object_database": True,
                "clean": False,
                "tree_sha256": inventory_tree_hash(dirty_inventory),
                "file_count": len(dirty_inventory),
                "source_bytes": sum(int(value["bytes"]) for value in dirty_inventory.values()),
            },
        },
        "files": clean_inventory,
        "exact_byte_files": {path: clean_inventory[path] for path in exact_paths},
        "languages": {
            "Rust": {"root": "sfbench_fixture/rust", "extension": ".rs"},
            "Python": {"root": "sfbench_fixture/python", "extension": ".py"},
            "TypeScript": {"root": "sfbench_fixture/typescript", "extension": ".ts"},
            "Go": {"root": "sfbench_fixture/go", "extension": ".go"},
            "Java": {"root": "sfbench_fixture/java", "extension": ".java"},
            "C++": {"root": "sfbench_fixture/cpp", "extension": ".cpp/.hpp"},
            "Perl": {"root": "sfbench_fixture/perl", "extension": ".pm/.t"},
        },
        "symbols": symbols,
        "references": references,
        "graphs": {
            "call_edges": {key: [{"caller": caller, "callee": callee} for caller, callee in value] for key, value in model.call_edges.items()},
            "recursive_custom_type_cycles": {
                language: ["CycleA", "CycleB", "CycleA"] for language in model.call_edges
            },
            "import_cycles": {
                "Rust": ["cycle_a.rs", "cycle_b.rs", "cycle_a.rs"],
                "Python": ["cycle_a.py", "cycle_b.py", "cycle_a.py"],
                "TypeScript": ["cycle_a.ts", "cycle_b.ts", "cycle_a.ts"],
                "C++": ["cycle_a.hpp", "cycle_b.hpp", "cycle_a.hpp"],
                "Perl": ["CycleA.pm", "CycleB.pm", "CycleA.pm"],
            },
        },
        "imports": imports,
        "implementors": model.implementors,
        "dependent_files": dependent_file_oracle(model),
        "markers": marker_oracle(clean_repo),
        "configs": config_oracle(clean_repo),
        "large_file": {
            "path": "sfbench_fixture/generated/large_generated.py",
            "minimum_bytes_exclusive": 60 * 1024,
            "actual_bytes": clean_inventory["sfbench_fixture/generated/large_generated.py"]["bytes"],
            "sha256": clean_inventory["sfbench_fixture/generated/large_generated.py"]["sha256"],
            "result_symbol_count": 1400,
            "anchors": ["SF_BENCH_LARGE_START_9F31", "SF_BENCH_LARGE_MIDDLE_9F31", "SF_BENCH_LARGE_END_9F31"],
            "search_marker": MARKERS["ccr"],
        },
        "duplicate_basenames": {
            language: sorted(
                value["path"]
                for value in symbols[language]
                if value["name"] == "sfbench_duplicate"
            )
            for language in symbols
        },
        "history": {
            "commit_count": 7,
            "source_test_cochange_count": sum(bool(value["source_test_cochange"]) for value in commits),
            "commits": commits,
            "refs": refs,
            "default_comparison_ref": "trunk",
        },
        "worktree": dirty,
        "mutations": mutation_oracle(clean_repo, model, symbols, references),
        "non_git": non_git,
        "symlinks": symlinks,
    }
    encoded = utf8(json.dumps(oracle, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    (output / "oracle.json").write_bytes(encoded)
    return oracle


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_root", type=Path, help="new directory outside the SymForge checkout")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    output = args.output_root.expanduser().resolve(strict=False)
    project_root = Path(__file__).resolve().parents[2]
    if output == project_root or output.is_relative_to(project_root):
        raise SystemExit("refusing output inside the SymForge checkout")
    if os.path.lexists(output):
        raise SystemExit(f"refusing existing output root: {output}")
    output.mkdir(parents=True)
    global_config = output / "gitconfig.empty"
    global_config.write_bytes(b"")
    template_dir = output / "git-template.empty"
    template_dir.mkdir()
    model = build_model()
    commits = build_repository(output / "control-repo", model, global_config, template_dir)
    mutation_commits = build_repository(output / "mutation-repo", model, global_config, template_dir)
    if [value["commit"] for value in commits] != [value["commit"] for value in mutation_commits]:
        raise RuntimeError("independent commit graphs differ")
    dirty = prepare_dirty_repository(output / "mutation-repo", global_config)
    non_git = create_non_git(output / "non-git")
    symlinks = create_symlink_cohort(output / "symlink-cohort")
    oracle = write_oracle(output, model, commits, dirty, non_git, symlinks, global_config)
    summary = {
        "protocol": PROTOCOL_ID,
        "output_root": str(output),
        "oracle_sha256": sha256((output / "oracle.json").read_bytes()),
        "clean_tree_sha256": oracle["repositories"]["clean"]["tree_sha256"],
        "mutation_tree_sha256": oracle["repositories"]["mutation"]["tree_sha256"],
        "head": oracle["repositories"]["clean"]["head"],
    }
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
