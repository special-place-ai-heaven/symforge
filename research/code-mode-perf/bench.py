#!/usr/bin/env python3
"""Exercise the real local Code Mode MCP surface on generated Git fixtures."""

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import platform
import queue
import re
import statistics
import subprocess
import tempfile
import threading
import time

from fixture import ENTRY, HIT_PATHS, HITS_PER_FILE, LARGE_FILES, MARKER, SMALL_FILES, TYPES, create

TIMEOUT = 45
MAP = 'repo_map(detail="compact")'
BUNDLE = f'symbol_context(name="choose_mode", path="{ENTRY}", bundle=True)'
WINDOW_CALLS = (
    f'file_content(path="{ENTRY}", start_line=3, end_line=5, show_line_numbers=False)',
    f'file_content(path="{TYPES}", start_line=1, end_line=4, show_line_numbers=False)',
)
WINDOWS = f'[{WINDOW_CALLS[0]}, {WINDOW_CALLS[1]}]'
WINDOWS_PROJECTED = f'{WINDOW_CALLS[0]}["content"] + "\\n" + {WINDOW_CALLS[1]}["content"]'
BUNDLE_PROJECTED = f'''b = {BUNDLE}
t = [d["body"] for d in b["dependencies"] if d["name"] == "FixtureConfig"][0]
b["body"] + "\\n" + "\\n".join(t.splitlines()[:4])'''
SEARCH = f'search_text(query="{MARKER}", limit=50'
HIT_SEARCH = f'{SEARCH}, max_per_file={HITS_PER_FILE}'
GROUP_CONTEXT = f'''rows = {HIT_SEARCH}, context=1)
groups = {{}}
for r in rows:
    path = r["path"]
    if path not in groups:
        groups[path] = []
    groups[path].append(str(r["line"]) + "\\t" + r["context"].rstrip())
"\\n".join([p + "\\n" + "\\n".join(groups[p]) for p in groups])'''


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def environment(home):
    keep = ("PATH", "PATHEXT", "SystemRoot", "WINDIR", "COMSPEC", "TEMP", "TMP", "TMPDIR")
    env = {key: os.environ[key] for key in keep if key in os.environ}
    return env | {
        "SYMFORGE_HOME": str(home / "symforge"), "HOME": str(home),
        "USERPROFILE": str(home), "LOCALAPPDATA": str(home), "APPDATA": str(home),
        "SYMFORGE_SURFACE": "code", "SYMFORGE_NO_DAEMON": "1", "SYMFORGE_AUTO_INDEX": "true",
        "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_ATTR_NOSYSTEM": "1", "NO_COLOR": "1",
    }


@dataclass
class Reply:
    result: dict
    text: str
    metrics: dict
    received: float


class Client:
    def __init__(self, binary, root, env):
        self.launch = time.perf_counter()
        self.proc = subprocess.Popen([str(binary)], cwd=root, env=env, stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.frames = queue.Queue()
        self.ident = 0
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()

    def read(self):
        for line in self.proc.stdout:
            self.frames.put((time.perf_counter(), line))
        self.frames.put((time.perf_counter(), b""))

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=5)
        self.reader.join(timeout=5)
        self.proc.stdout.close()

    def rpc(self, method, params, *, notify=False):
        packet = {"jsonrpc": "2.0", "method": method, "params": params}
        if not notify:
            self.ident += 1
            packet["id"] = self.ident
        wire = (json.dumps(packet, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
        start = time.perf_counter()
        self.proc.stdin.write(wire)
        self.proc.stdin.flush()
        if notify:
            return None
        deadline = start + TIMEOUT
        while True:
            try:
                received, line = self.frames.get(timeout=max(0, deadline - time.perf_counter()))
            except queue.Empty as exc:
                raise RuntimeError(f"MCP timed out: {method}") from exc
            require(line, f"MCP exited before responding: {method}")
            msg = json.loads(line)
            if "id" in msg:
                require(msg["id"] == self.ident and "result" in msg, f"MCP failed: {msg}")
                break
            require(time.perf_counter() < deadline, f"MCP timed out: {method}")
        result = msg["result"]
        blocks = result.get("content", [])
        text = "\n".join(item["text"] for item in blocks if item.get("type") == "text")
        return Reply(result, text, {
            "rpc_ms": ms(received - start), "request_bytes": len(wire),
            "response_bytes": len(line), "text_bytes": len(text.encode()),
        }, received)

    def initialize(self):
        reply = self.rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                       "clientInfo": {"name": "code-mode-perf", "version": "1"}})
        self.rpc("notifications/initialized", {}, notify=True)
        return reply

    def tool(self, name, args):
        return self.rpc("tools/call", {"name": name, "arguments": args})

    def code(self, script):
        return self.tool("code_execute", {"code": script})


def ms(seconds):
    return round(seconds * 1000, 3)


def ready(client, files):
    deadline = time.perf_counter() + TIMEOUT
    polls = 0
    while time.perf_counter() < deadline:
        reply = client.tool("status", {})
        polls += 1
        rows = reply.text.splitlines()
        if "index_ready: true" in rows:
            require(f"index_files: {files}" in rows, "ready index did not cover the fixture")
            return {"launch_to_ready_ms": ms(reply.received - client.launch), "polls": polls}
        time.sleep(0.02)
    raise RuntimeError("index did not become ready")


def success(reply, calls):
    require(not reply.result.get("isError"), f"Code Mode failed: {reply.text}")
    meta = reply.result.get("_meta", {}).get("symforge/code_execute", {})
    require(meta.get("calls") == calls, f"unexpected Code Mode counter: {reply.text}")
    # Success must not be a spill, truncated JSON, or an error disguised as a short payload.
    require(reply.text and '"truncated":true' not in reply.text, "empty/truncated payload")
    return reply.metrics | {"host_calls": calls, "runner_ms": meta["duration_ms"]}


def body_lines(text):
    return [row.strip() for row in text.splitlines()]


def measure_small(client, root, runs):
    cases = {
        "f11_bundle": (BUNDLE, 1), "f11_windows": (WINDOWS, 2),
        "f11_bundle_projected": (BUNDLE_PROJECTED, 1),
        "f11_windows_projected": (WINDOWS_PROJECTED, 2),
        "f13_hits": (HIT_SEARCH + ")", 1), "f13_context_hits": (HIT_SEARCH + ", context=1)", 1),
        "f13_grouped_context": (GROUP_CONTEXT, 1), "f13_counts": (SEARCH + ', group_by="count")', 1),
    }
    expected_source = [line.strip() for path, start, end in ((ENTRY, 2, 5), (TYPES, 0, 4))
                       for line in (root / path).read_text(encoding="utf-8").splitlines()[start:end]]
    metrics = {name: [] for name in cases}
    for sample in range(runs + 1):  # one unrecorded pass to prime all cases
        replies = {}
        names = list(cases)
        offset = sample % len(names)
        for name in names[offset:] + names[:offset]:
            code, calls = cases[name]
            reply = client.code(code)
            row = success(reply, calls)
            replies[name] = reply.text
            if sample:
                if name.startswith("f13"):
                    row["path_mentions"] = sum(reply.text.count(p) for p in HIT_PATHS)
                metrics[name].append(row)
        for name in ("f11_bundle", "f11_windows"):
            require(all(line in body_lines(replies[name]) for line in expected_source),
                    f"{name} missed a required source line: {replies[name]}")
        for name in ("f11_bundle_projected", "f11_windows_projected"):
            lines = [line for line in body_lines(replies[name]) if line]
            require(lines == expected_source, f"{name} projected unexpected lines: {replies[name]}")
        require(replies["f11_bundle_projected"] == replies["f11_windows_projected"],
                "bundle and windows projections did not return identical source text")
        validate_search(replies, root)
    return metrics


def validate_search(replies, root):
    plain = replies["f13_hits"]
    ctx = replies["f13_context_hits"]
    grouped = replies["f13_grouped_context"]
    counts = replies["f13_counts"]
    plain_hits, raw_hits, grouped_hits = [], [], []
    path = None
    header, *plain_rows = plain.splitlines()
    line_column = header.split("\t").index("line")
    for row in plain_rows:
        if row.startswith("@ "):
            path = row[2:]
        else:
            fields = row.split("\t")
            plain_hits.append((path, int(fields[line_column])))
    expected_context = {}
    for path in HIT_PATHS:
        lines = (root / path).read_text(encoding="utf-8").splitlines()
        for i, line in enumerate(lines):
            if MARKER in line:
                expected_context[path, i + 1] = [f"{j + 1}: {lines[j]}" for j in range(i - 1, i + 2)]
    for record in ctx.split("@ ")[1:]:
        header, body = record.split("\ncontext:\n", 1)
        keys = dict(field.split("=", 1) for field in header.split("\t"))
        hit = (keys["path"], int(keys["line"]))
        raw_hits.append(hit)
        require(body.rstrip().splitlines() == expected_context[hit], "raw context differs from source")
    path = None
    rows = iter(grouped.splitlines())
    for row in rows:
        if row in HIT_PATHS:
            path = row
        else:
            line, first = row.split("\t", 1)
            hit = (path, int(line))
            grouped_hits.append(hit)
            require([first, next(rows), next(rows)] == expected_context[hit], "grouped context differs from source")
    for form, hits in (("plain", plain_hits), ("raw context", raw_hits), ("grouped context", grouped_hits)):
        require(sorted(hits) == sorted(expected_context),
                f"{form} did not return all fixture locations once: {hits}")
    require(len(counts.splitlines()) == 1 + 2 * len(HIT_PATHS), "unexpected count table length")
    for path in HIT_PATHS:
        require(f"@ {path}\n{HITS_PER_FILE}" in counts, f"per-file count differs from fixture: {counts}")


def valid_map(reply):
    return f"total_files: {LARGE_FILES}" in reply.text.splitlines() and \
        f"Index: {LARGE_FILES} files," in reply.text


def map_sample(binary, root, env, *, cold, warm_runs):
    with Client(binary, root, env) as client:
        init = client.initialize()
        report = {"launch_to_initialize_ms": ms(init.received - client.launch)}
        if cold:
            first = client.code(MAP)
            report["first_rpc"] = first.metrics
            # A deferred-front loading receipt is not a successful map. Retain
            # its timing separately and retry only that documented condition.
            retries = 0
            reply = first
            deadline = time.perf_counter() + TIMEOUT
            while not valid_map(reply) and "retry" in reply.text.lower() and \
                    "indexing" in reply.text.lower() and time.perf_counter() < deadline:
                retries += 1
                reply = client.code(MAP)
            report["loading_retries"] = retries
        else:
            report.update(ready(client, LARGE_FILES))
            reply = client.code(MAP)
        require(valid_map(reply), f"did not get the complete compact map: {reply.text}")
        report["first_map"] = success(reply, 1)
        report["launch_to_first_map_ms"] = ms(reply.received - client.launch)
        report["warm_maps"] = []
        for _ in range(warm_runs if not cold else 0):
            repeat = client.code(MAP)
            require(repeat.text == reply.text, "warm map changed on an unmodified repository")
            report["warm_maps"].append(success(repeat, 1))
        return report, hashlib.sha256(reply.text.encode()).hexdigest()


def stats(rows):
    return {key: {"median": statistics.median(values), "min": min(values), "max": max(values)}
            for key in rows[0] for values in [[row[key] for row in rows]]}


def run(binary, runs, warm_runs, scratch):
    env = environment(scratch / "home")
    Path(env["HOME"]).mkdir()
    (scratch / "home" / "symforge").mkdir()
    version = subprocess.check_output([str(binary), "--version"], cwd=scratch, env=env,
                                      timeout=10).decode().strip()
    digest = hashlib.sha256()
    with binary.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    small = scratch / "small"
    fixtures = {"small": create(small, SMALL_FILES, env)}
    with Client(binary, small, env) as client:
        client.initialize()
        ready(client, SMALL_FILES)
        small_metrics = measure_small(client, small, runs)
    phases = {"cold": [], "ready": []}
    hashes = set()
    for trial in range(runs):
        for phase in (("cold", "ready") if trial % 2 == 0 else ("ready", "cold")):
            root = scratch / f"{phase}-{trial}"
            fixtures["large"] = create(root, LARGE_FILES, env)
            sample, map_hash = map_sample(binary, root, env, cold=phase == "cold", warm_runs=warm_runs)
            phases[phase].append(sample)
            hashes.add(map_hash)
    require(len(hashes) == 1, "compact map differs across identical fixtures/phases")
    return {
        "binary": {"version": version, "sha256": digest.hexdigest()},
        "environment": {"system": platform.system(), "release": platform.release(),
                        "machine": platform.machine(), "visible_logical_cpus": os.cpu_count()},
        "runs": runs, "warm_runs_per_ready_process": warm_runs,
        "fixtures": fixtures, "f11_f13_samples": small_metrics, "f12_samples": phases,
        "compact_map_text_sha256": hashes.pop(),
        "summary": {
            **{key: stats(rows) for key, rows in small_metrics.items()},
            **{f"f12_{phase}_first_map": stats([sample["first_map"] for sample in samples])
               for phase, samples in phases.items()},
            "f12_warm": stats([row for sample in phases["ready"] for row in sample["warm_maps"]]),
            "f12_cold_launch_to_map": stats([{"ms": s["launch_to_first_map_ms"]} for s in phases["cold"]]),
            "f12_ready_launch_to_status": stats([{"ms": s["launch_to_ready_ms"]} for s in phases["ready"]]),
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--warm-runs", type=int, default=7)
    args = parser.parse_args()
    if min(args.runs, args.warm_runs) < 1:
        parser.error("run counts must be positive")
    binary = args.binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="symforge-code-perf-") as directory:
        print(json.dumps(run(binary, args.runs, args.warm_runs, Path(directory)), indent=2))


if __name__ == "__main__":
    main()
