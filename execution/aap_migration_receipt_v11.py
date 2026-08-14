#!/usr/bin/env python3
"""Feature 020 V11, T049 — the AAP migration receipt harness.

Runs the pre-activation consumer corpus (tests/fixtures/public-api-v11-consumer)
against the DARK API adapter and records what was actually observed, never what
activation would make true:

* the generated all-cfg inventory: every item of the synthetic completeness
  crate evaluated against all 26 target/feature cells of graph-cover.json,
  with the completeness sentinels checked against the union;
* the dependent-positive fixture, materialized with its `symforge::embed::*`
  paths mapped through the adapter (harness scaffolding, NOT the activation
  mapping) and compiled;
* every atomic compile-fail case, in two lanes: the REAL lane compiles the
  subject verbatim against the live crate and records the pre-activation
  truth (resolution failure, still-public path, or a genuine expected code),
  and the ADAPTER lane (embed-vector trait/impl groups only) maps the subject
  through the adapter so the case can fail for its CONTRACT reason.

The frozen corpus is read, never written. Output is one JSON document; the
human-readable receipt in docs/reviews/AAP-MIGRATION-RECEIPT-v11.md cites it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ADAPTER_NAME = "symforge-v11-dark-adapter"
ADAPTER_LIB = "symforge_v11_dark_adapter"

ADAPTER_SRC = """\
//! T049 dark API adapter — harness scaffolding ONLY.
//!
//! Maps the frozen contract's `symforge::embed::*` atom names onto the dark
//! Slice 3 boundary so the pre-activation consumer fixtures can compile and
//! fail for CONTRACT reasons instead of resolution reasons. This crate is not
//! the activation mapping and must never ship.

pub mod embed {
    pub use symforge::embed::{EngineInfo, engine_info};
    pub use symforge::live_index::index_lifecycle::embedded::{
        EmbeddedSourceHandle, ReceiptWaitError, SourceCloseReceipt,
    };
    pub use symforge::live_index::index_lifecycle::public_api::{
        EmbedAtomicAuthority as AtomicAuthority, EmbedClaim as Claim,
        EmbedClaimProvenance as ClaimProvenance,
        EmbedEvaluationProvenance as EvaluationProvenance,
        EmbedOperationReceipt as OperationReceipt, EmbedRefreshTicket as RefreshTicket,
        EmbedShutdownReceipt as ShutdownReceipt, EmbedSourceRefusal as SourceRefusal,
        EmbeddedSourceSpec, OperationKind, ProcessRuntimeApi as ProcessIndexRuntime,
        RetryAdvice, ShutdownReport, SourceCloseReport, SourceRefusalKind,
        SourceRuntimePhase, SourceRuntimeView, SymbolMatch, SymbolSearchRequest,
        SymbolSearchResult, TextMatch, TextSearchRequest, TextSearchResult,
    };
}
"""

# C16: the positive sentinels only prove PRESENCE in the union, which an
# over-inclusive cfg evaluator satisfies trivially. These are closed expected
# ABSENCES, hand-derived from graph-cover.json's own target/feature facts —
# an evaluator that says yes to everything fails every row.
NEGATIVE_SENTINELS = [
    ("WindowsOnly", "x86_64-unknown-linux-gnu__embed"),
    ("Aarch64Only", "x86_64-pc-windows-msvc__server"),
    ("MacosOnly", "x86_64-unknown-linux-musl__embed"),
    ("MsvcOnly", "x86_64-unknown-linux-gnu__embed"),
    ("NotServer", "x86_64-pc-windows-msvc__server"),
    ("PureEmbed", "x86_64-unknown-linux-gnu__server-embed"),
    ("Atomic128", "x86_64-unknown-linux-gnu__embed"),
    ("EmbedEnabled", "x86_64-apple-darwin__server"),
    ("CbmSpikeEnabled", "x86_64-unknown-linux-gnu__embed"),
]

RESOLUTION_CODES = {"E0412", "E0425", "E0432", "E0433", "E0603"}


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


# ── all-cfg inventory ──────────────────────────────────────────────────────


class CfgExpr:
    """Tiny recursive-descent evaluator for the cfg predicates the all-cfg
    crate actually uses: key = "value" atoms, all(), any(), not()."""

    def __init__(self, text: str):
        self.tokens = re.findall(r'\w+|\(|\)|,|=|"[^"]*"', text)
        self.pos = 0

    def peek(self):
        return self.tokens[self.pos] if self.pos < len(self.tokens) else None

    def eat(self, expected=None):
        token = self.tokens[self.pos]
        if expected is not None and token != expected:
            raise ValueError(f"expected {expected!r}, got {token!r}")
        self.pos += 1
        return token

    def parse(self):
        node = self.expr()
        if self.pos != len(self.tokens):
            raise ValueError(f"trailing tokens in cfg: {self.tokens[self.pos:]}")
        return node

    def expr(self):
        head = self.eat()
        if head in ("all", "any", "not"):
            self.eat("(")
            args = []
            while self.peek() != ")":
                args.append(self.expr())
                if self.peek() == ",":
                    self.eat(",")
            self.eat(")")
            return (head, args)
        if self.peek() == "=":
            self.eat("=")
            value = self.eat()
            if not value.startswith('"'):
                raise ValueError(f"cfg value for {head} is not a string: {value!r}")
            return ("atom", head, value.strip('"'))
        raise ValueError(f"bare cfg ident unsupported: {head!r}")


def eval_cfg(node, facts) -> bool:
    kind = node[0]
    if kind == "atom":
        _, key, value = node
        have = facts.get(key)
        if have is None:
            return False
        if isinstance(have, list):
            return value in have
        return have == value
    if kind == "all":
        return all(eval_cfg(child, facts) for child in node[1])
    if kind == "any":
        return any(eval_cfg(child, facts) for child in node[1])
    if kind == "not":
        (child,) = node[1]
        return not eval_cfg(child, facts)
    raise ValueError(f"unknown cfg node {kind!r}")


def parse_all_cfg_items(source: str):
    """Yield (item_name, cfg_expr_text_or_None) plus trait-associated items."""
    items = []
    pending_cfg = None
    macro_exported = False
    lines = source.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i].strip()
        cfg_match = re.fullmatch(r"#\[cfg\((.*)\)\]", line)
        if cfg_match:
            pending_cfg = cfg_match.group(1)
            i += 1
            continue
        if line == "#[macro_export]":
            macro_exported = True
            i += 1
            continue
        struct_match = re.match(r"pub struct (\w+)", line)
        trait_match = re.match(r"pub trait (\w+)", line)
        impl_match = re.match(r"impl (\w+) for (\w+)", line)
        macro_match = re.match(r"macro_rules! (\w+)", line)
        if struct_match:
            items.append((struct_match.group(1), pending_cfg))
        elif trait_match:
            trait_name = trait_match.group(1)
            items.append((trait_name, pending_cfg))
            i += 1
            while i < len(lines) and lines[i].strip() != "}":
                body = lines[i].strip()
                assoc = re.match(r"(?:type|const|fn) (\w+)", body)
                if assoc:
                    items.append((f"{trait_name}::{assoc.group(1)}", pending_cfg))
                i += 1
        elif impl_match:
            items.append(
                (f"{impl_match.group(1)} for {impl_match.group(2)}", pending_cfg)
            )
        elif macro_match and macro_exported:
            items.append((macro_match.group(1), pending_cfg))
            macro_exported = False
        if not line.startswith("#["):
            pending_cfg = None
        i += 1
    return items


def build_inventory(fixture_dir: Path):
    source = (fixture_dir / "all-cfg" / "src" / "lib.rs").read_text(encoding="utf-8")
    cover = json.loads((fixture_dir / "graph-cover.json").read_text(encoding="utf-8"))
    items = parse_all_cfg_items(source)
    parsed = [
        (name, CfgExpr(cfg).parse() if cfg else None) for name, cfg in items
    ]
    targets = {t["id"]: t for t in cover["targets"]}
    vectors = {v["id"]: v for v in cover["feature_vectors"]}
    cells = {}
    for cell in cover["cells"]:
        target = targets[cell["target"]]
        vector = vectors[cell["feature_vector"]]
        facts = {
            "target_arch": target["arch"],
            "target_os": target["os"],
            "target_env": target["env"],
            "target_vendor": target["vendor"],
            "target_family": target["family"],
            "target_endian": target["endian"],
            "target_pointer_width": str(target["pointer_width"]),
            "target_has_atomic": target["atomic_widths"],
            "feature": vector["resolved"],
        }
        cells[cell["id"]] = sorted(
            name for name, node in parsed if node is None or eval_cfg(node, facts)
        )
    union = sorted(set().union(*cells.values()))
    sentinels = {}
    for sentinel in cover["completeness_sentinels"]:
        missing = [s for s in sentinel["required_symbols"] if s not in union]
        sentinels[sentinel["id"]] = {
            "required_symbols": sentinel["required_symbols"],
            "missing_from_union": missing,
            "satisfied": not missing,
        }
    inventory = {
        "kind": "symforge.t049_all_cfg_inventory",
        "schema_version": 1,
        "cell_count": len(cells),
        "cells": cells,
        "union": union,
        "sentinels": sentinels,
    }
    canonical = json.dumps(inventory, sort_keys=True, separators=(",", ":"))
    return inventory, sha256_text(canonical)


# ── crate materialization ──────────────────────────────────────────────────


def write_crate(root: Path, name: str, deps: str, source: str, bin_crate: bool, extra_manifest: str = ""):
    root.mkdir(parents=True, exist_ok=True)
    (root / "src").mkdir(exist_ok=True)
    (root / "Cargo.toml").write_text(
        "[package]\n"
        f'name = "{name}"\n'
        'version = "0.0.0"\n'
        'edition = "2021"\n'
        "publish = false\n\n"
        "[dependencies]\n" + deps + extra_manifest,
        encoding="utf-8",
    )
    target = root / "src" / ("main.rs" if bin_crate else "lib.rs")
    target.write_text(source, encoding="utf-8")


def symforge_dep(repo: Path, features) -> str:
    feature_list = ", ".join(f'"{f}"' for f in features)
    return (
        f'symforge = {{ path = "{repo.as_posix()}", default-features = false, '
        f"features = [{feature_list}] }}\n"
    )


def build_env():
    """MSVC cl.exe 14.51 rejects tree-sitter-scss's GCC-style
    `-Wno-unused-parameter`, so a COLD build of the symforge dependency fails
    under the default cc-rs toolchain on Windows (the repo's own gates only
    pass because every checkout's target/ carries a warm scanner object).
    clang-cl accepts both flag styles; pin it when present and record it."""
    env = dict(os.environ)
    if os.name == "nt" and shutil.which("clang-cl"):
        env.setdefault("CC", "clang-cl")
        env.setdefault("CXX", "clang-cl")
    return env


CARGO_ENV = None

# (needle, replacement) pairs applied to every recorded diagnostic so the
# committed artifact carries no machine-specific absolute paths.
SANITIZE = []


def sanitize(text: str) -> str:
    for needle, replacement in SANITIZE:
        text = text.replace(needle, replacement)
    return text


def cargo_check(ws_root: Path, package: str, target_dir: Path):
    global CARGO_ENV
    if CARGO_ENV is None:
        CARGO_ENV = build_env()
    proc = subprocess.run(
        [
            "cargo", "check", "-p", package,
            "--message-format=json",
            "--target-dir", str(target_dir),
        ],
        cwd=ws_root,
        env=CARGO_ENV,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    errors = []
    dependency_error = False
    for line in proc.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-message":
            continue
        message = msg.get("message", {})
        if message.get("level") != "error":
            continue
        # C15: an error raised while compiling a DEPENDENCY must not be
        # attributed to the case under test.
        if package not in str(msg.get("package_id", "")):
            dependency_error = True
            continue
        code = (message.get("code") or {}).get("code")
        errors.append({"code": code, "text": sanitize(message.get("message", ""))})
    primary = next((e["code"] for e in errors if e["code"]), None)
    return {
        "exit_code": proc.returncode,
        "primary_code": primary,
        "error_count": len(errors),
        "dependency_error": dependency_error,
        "first_errors": errors[:3],
        "stderr_tail": sanitize(proc.stderr[-1200:]) if proc.returncode != 0 else "",
    }


def classify(result, expected_codes):
    if result["exit_code"] == 0:
        return "compiles"
    if result["primary_code"] in expected_codes:
        return "expected-failure"
    if result["primary_code"] in RESOLUTION_CODES:
        return "resolution-failure"
    return "other-failure"


# ── case expansion ─────────────────────────────────────────────────────────


def expand_cases(fixture_dir: Path):
    cases_doc = json.loads(
        (fixture_dir / "compile-fail" / "cases.json").read_text(encoding="utf-8")
    )
    templates = {
        name: (fixture_dir / "compile-fail" / "templates" / f"{name}.rs.in").read_text(
            encoding="utf-8"
        )
        for name in ("trait_absent", "impl_family_absent", "path_absent")
    }
    cases = []
    for group in cases_doc["trait_absent_groups"] + cases_doc["impl_family_absent_groups"]:
        template = templates[Path(group["template"]).name.removesuffix(".rs.in")]
        for index, subject in enumerate(group["subjects"]):
            source = template.replace("{{BOUND}}", group["bound"]).replace(
                "{{SUBJECT}}", subject
            )
            cases.append({
                "assertion_id": group["assertion_id"],
                "index": index,
                "subject": subject,
                "feature_vector": group["feature_vector"],
                "expected_error_codes": group["expected_error_codes"],
                "source": source,
                "adapter_lane": group["feature_vector"] == "embed",
            })
    for group in cases_doc["path_absent_groups"]:
        template = templates["path_absent"]
        for index, path in enumerate(group["paths"]):
            cases.append({
                "assertion_id": group["assertion_id"],
                "index": index,
                "subject": path,
                "feature_vector": group["feature_vector"],
                "expected_error_codes": group["expected_error_codes"],
                "source": template.replace("{{PATH}}", path),
                "adapter_lane": False,
            })
    return cases


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--work", type=Path, default=None,
                        help="scratch root; defaults to <repo>/target/aap-harness")
    parser.add_argument("--stage", choices=("red", "full"), default="full")
    parser.add_argument("--json-out", type=Path, default=None,
                        help="defaults to <repo>/docs/reviews/AAP-MIGRATION-RECEIPT-v11.json")
    parser.add_argument("--check", action="store_true",
                        help="C12: exit nonzero unless every gated expectation held")
    args = parser.parse_args()

    repo = args.repo.resolve()
    work = (args.work or repo / "target" / "aap-harness").resolve()
    json_out = (args.json_out or repo / "docs" / "reviews" / "AAP-MIGRATION-RECEIPT-v11.json").resolve()
    SANITIZE.extend([
        (str(work), "<work>"),
        (work.as_posix(), "<work>"),
        (str(repo), "<repo>"),
        (repo.as_posix(), "<repo>"),
    ])
    fixture_dir = repo / "tests" / "fixtures" / "public-api-v11-consumer"
    commit = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    # C17: repo_commit names a commit; the tree must be OBSERVED to be that
    # commit, so cleanliness is recorded — and gated in check mode.
    dirty_lines = subprocess.run(
        ["git", "-C", str(repo), "status", "--porcelain"],
        capture_output=True, text=True, check=True,
    ).stdout.strip().splitlines()

    report = {
        "kind": "symforge.t049_aap_migration_receipt_run",
        "schema_version": 1,
        "stage": args.stage,
        "repo_commit": commit,
        "worktree_dirty": bool(dirty_lines),
        "worktree_dirty_paths": len(dirty_lines),
        "claims_v11_exports_live": False,
        "c_compiler": build_env().get("CC", "toolchain-default"),
        "rerun_command": "python execution/aap_migration_receipt_v11.py --stage full --check",
    }

    # The generated all-cfg inventory (no compiler required — pure cfg math
    # over the frozen corpus facts).
    inventory, inventory_sha = build_inventory(fixture_dir)
    report["all_cfg_inventory_sha256"] = inventory_sha
    report["all_cfg_sentinels"] = inventory["sentinels"]
    report["all_cfg_cell_count"] = inventory["cell_count"]
    report["all_cfg_negative_sentinels"] = [
        {
            "symbol": symbol,
            "cell": cell,
            "absent_as_expected": symbol not in inventory["cells"][cell],
        }
        for symbol, cell in NEGATIVE_SENTINELS
    ]
    (work / "all-cfg-inventory.json").parent.mkdir(parents=True, exist_ok=True)
    (work / "all-cfg-inventory.json").write_text(
        json.dumps(inventory, indent=2, sort_keys=True), encoding="utf-8"
    )

    # ── embed workspace: adapter + positive + embed-vector cases ──
    ws_embed = work / "ws-embed"
    target_embed = work / "target-embed"
    write_crate(
        ws_embed / "adapter", ADAPTER_NAME,
        symforge_dep(repo, ["embed"]), ADAPTER_SRC, bin_crate=False,
    )
    positive_source = (
        (fixture_dir / "dependent-positive" / "src" / "lib.rs")
        .read_text(encoding="utf-8")
        .replace("symforge::embed", f"{ADAPTER_LIB}::embed")
    )
    # The fixture gates its embed consumer behind `#[cfg(feature = "embed")]`,
    # so the materialized crate MUST define and default that feature — without
    # it the whole module is cfg'd out and "compiles" is a vacuous claim about
    # an empty lib. Found the honest way: mutation M22 removed a fixture-pinned
    # method and the check still passed, exposing the vacuity.
    write_crate(
        ws_embed / "positive", "symforge-public-api-v11-positive-dark",
        f'{ADAPTER_NAME} = {{ path = "../adapter" }}\n',
        positive_source, bin_crate=False,
        extra_manifest='\n[features]\ndefault = ["embed"]\nembed = []\n',
    )

    cases = expand_cases(fixture_dir)
    members = ["adapter", "positive"]
    ws_server = work / "ws-server"
    target_server = work / "target-server"
    server_members = []
    for case in cases:
        slug = f"{case['assertion_id']}-{case['index']}"
        if case["feature_vector"] == "embed":
            features, ws, member_list = ["embed"], ws_embed, members
        else:
            features, ws, member_list = ["server", "embed"], ws_server, server_members
        write_crate(
            ws / f"real-{slug}", f"real-{slug}",
            symforge_dep(repo, features)
            + 'serde = { version = "1", default-features = false }\n',
            case["source"], bin_crate=True,
        )
        member_list.append(f"real-{slug}")
        case["real_package"] = f"real-{slug}"
        if case["adapter_lane"]:
            write_crate(
                ws_embed / f"dark-{slug}", f"dark-{slug}",
                f'{ADAPTER_NAME} = {{ path = "../adapter" }}\n'
                + 'serde = { version = "1", default-features = false }\n',
                case["source"].replace("symforge::", f"{ADAPTER_LIB}::"),
                bin_crate=True,
            )
            members.append(f"dark-{slug}")
            case["dark_package"] = f"dark-{slug}"

    for ws, member_list in ((ws_embed, members), (ws_server, server_members)):
        if not member_list:
            continue
        ws.mkdir(parents=True, exist_ok=True)
        listed = ",\n    ".join(f'"{m}"' for m in sorted(member_list))
        (ws / "Cargo.toml").write_text(
            f'[workspace]\nresolver = "2"\nmembers = [\n    {listed},\n]\n',
            encoding="utf-8",
        )

    print(f"[t049] adapter check ({args.stage})", file=sys.stderr, flush=True)
    report["adapter"] = cargo_check(ws_embed, ADAPTER_NAME, target_embed)
    print(f"[t049] positive check", file=sys.stderr, flush=True)
    report["dependent_positive"] = cargo_check(
        ws_embed, "symforge-public-api-v11-positive-dark", target_embed
    )
    report["dependent_positive"]["outcome"] = classify(report["dependent_positive"], [])

    if args.stage == "red":
        report["check_failures"] = check_failures(report)
        json_out.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(f"[t049] RED stage written to {json_out}", file=sys.stderr)
        return finish(args, report)

    results = []
    for number, case in enumerate(cases, 1):
        for lane, package_key, ws, target_dir in (
            ("real", "real_package", ws_server if case["feature_vector"] != "embed" else ws_embed,
             target_server if case["feature_vector"] != "embed" else target_embed),
            ("adapter", "dark_package", ws_embed, target_embed),
        ):
            package = case.get(package_key)
            if not package:
                continue
            print(
                f"[t049] case {number}/{len(cases)} lane={lane} {case['assertion_id']}"
                f"[{case['index']}]",
                file=sys.stderr, flush=True,
            )
            result = cargo_check(ws, package, target_dir)
            results.append({
                "assertion_id": case["assertion_id"],
                "index": case["index"],
                "subject": case["subject"],
                "feature_vector": case["feature_vector"],
                "lane": lane,
                "expected_error_codes": case["expected_error_codes"],
                "outcome": classify(result, case["expected_error_codes"]),
                "primary_code": result["primary_code"],
                "error_count": result["error_count"],
                "first_error": result["first_errors"][0] if result["first_errors"] else None,
            })

    summary = {}
    for row in results:
        key = f"{row['lane']}:{row['outcome']}"
        summary[key] = summary.get(key, 0) + 1
    report["case_results"] = results
    report["case_summary"] = summary
    report["case_count"] = len(cases)
    report["check_failures"] = check_failures(report)
    json_out.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"[t049] full run written to {json_out}", file=sys.stderr)
    print(json.dumps(summary, indent=2))
    return finish(args, report)


def check_failures(report):
    """C12: the harness has explicit failure modes — every gated expectation
    that did not hold, so 'the receipt says so' is backed by an exit code."""
    failures = []
    if report.get("worktree_dirty"):
        failures.append("worktree dirty: repo_commit does not name the observed tree")
    for sentinel_id, sentinel in report.get("all_cfg_sentinels", {}).items():
        if not sentinel["satisfied"]:
            failures.append(f"sentinel {sentinel_id} unsatisfied")
    for row in report.get("all_cfg_negative_sentinels", []):
        if not row["absent_as_expected"]:
            failures.append(
                f"negative sentinel: {row['symbol']} present in {row['cell']}"
            )
    if report.get("adapter", {}).get("exit_code") != 0:
        failures.append("adapter failed to compile")
    if report.get("dependent_positive", {}).get("outcome") != "compiles":
        failures.append("dependent-positive did not compile")
    for row in report.get("case_results", []):
        if row["lane"] == "adapter" and row["outcome"] != "expected-failure":
            failures.append(
                f"adapter case {row['assertion_id']}[{row['index']}]: {row['outcome']}"
            )
        if row["lane"] == "real" and row["outcome"] == "other-failure":
            failures.append(
                f"real case {row['assertion_id']}[{row['index']}]: other-failure"
            )
    return failures


def finish(args, report) -> int:
    if args.check and report["check_failures"]:
        for failure in report["check_failures"]:
            print(f"[t049] CHECK FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
