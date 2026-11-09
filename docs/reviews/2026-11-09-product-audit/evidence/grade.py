"""Grade benchmark answers against fixed ground-truth facts and print paired summaries.

Usage: python -I grade.py all.json
Each task has a list of required facts; a fact is a list of alternative regexes (any one matches).
"""
import json
import re
import statistics
import sys

rows = json.load(open(sys.argv[1], encoding="utf-8"))

EXTS = [".txt", ".xml", ".csv", ".lock", ".gitignore", ".dockerignore", ".editorconfig",
        ".prettierrc", ".eslintrc", ".ini", ".cfg", ".conf", ".html", ".css", ".svg", ".png",
        ".jpg", ".jpeg", ".gif", ".ico"]
DIRS = ["/docs/", "/doc/", "/.github/", "/.planning/", "/.claude/", "/.gemini/", "/.codex/",
        "/node_modules/", "/.git/"]

FACTS = {
    "T1": [[r"tools\.rs"], [r"1936"], [r"compact"], [r"\bfull\b"], [r"\btree\b"],
           [r"\b200\b"], [r"\b5\b"], [r"\*\s*30"], [r"\*\s*10"], [r"\b500\b"],
           [r"knowledge"], [r"topology|code tree|symbol map|outline|code map|file tree|the map"]],
    "T2": [[r"7005"], [r"2098"], [r"3342"], [r"mod\.rs:924"], [r"execute_cross_project_review"],
           [r"get_repo_map"], [r"review_knowledge"]],
    "T3": [[r"\"?routed\"?"], [r"no-sidecar"], [r"sidecar-error"], [r"daemon-fallback"],
           [r"\b50\s*ms|from_millis\(50\)|\b50\b"], [r"\b500\s*ms|from_millis\(500\)|\b500\b"],
           [r"index_not_ready"], [r"root_conflict"], [r"http_failure"]],
    "T4": [[r"src/cli/hook\.rs`?[^\n]{0,12}\b12\b"], [r"src/protocol/format\.rs`?[^\n]{0,12}\b11\b"],
           [r"format/tests\.rs`?[^\n]{0,12}\b10\b"], [r"hook_subprocess_integration\.rs`?[^\n]{0,12}\b11\b"]],
    "T5": [[r"is_non_source_path"], [r"hook\.rs:914"]] + [[re.escape(e) + r"\b"] for e in EXTS]
          + [[re.escape(d)] for d in DIRS],
    "T6": [[r"173[0-5]"], [r"\bNone\b"], [r"append"], [r"read_session_id_for_repo_at|1700|1766"],
           # bonus fact: session ids are reused across daemon restarts
           [r"restart|next_session_id|daemon\.rs:(1172|1522)|reused|not unique"]],
}

for r in rows:
    facts = FACTS[r["task"]]
    text = r["final"].replace("\\\\", "\\")
    hit = sum(1 for alts in facts if any(re.search(a, text) for a in alts))
    r["score"] = round(hit / len(facts), 2)
    r["missed"] = [alts[0] for alts in facts if not any(re.search(a, text) for a in alts)]
    r["rule_break"] = (
        (r["arm"] == "A" and any(k.startswith("mcp__symforge") for k in r["tools"]))
        or (r["arm"] == "B" and any(k in ("Grep", "Glob", "Read", "Bash") for k in r["tools"]))
    )

print("name            score calls growth toolchars turns  out  rule_break missed")
for r in rows:
    print(f"{r['name']:<15} {r['score']:>5} {r['tool_calls']:>5} {r['growth_ctx']:>6} "
          f"{r['tool_result_chars']:>9} {r['api_turns']:>5} {r['output']:>5} {str(r['rule_break']):>10} "
          f"{r['missed'][:4]}")


def cohort(pred, label):
    print(f"\n== {label}: paired by task (A = conventional, B = SymForge) ==")
    print("task  n  A_growth B_growth  delta%   A_calls B_calls  A_turns B_turns  A_chars B_chars  A_score B_score  A_dur B_dur")
    deltas = []
    tot = {"A": [0, 0, 0, 0], "B": [0, 0, 0, 0]}
    for task in sorted(FACTS):
        a = [r for r in rows if r["task"] == task and r["arm"] == "A" and pred(r)]
        b = [r for r in rows if r["task"] == task and r["arm"] == "B" and pred(r)]
        if not a or not b:
            continue
        m = lambda xs, k: statistics.mean(x[k] for x in xs)
        ag, bg = m(a, "growth_ctx"), m(b, "growth_ctx")
        d = (bg - ag) / ag * 100
        deltas.append(d)
        for arm, xs in (("A", a), ("B", b)):
            tot[arm][0] += m(xs, "growth_ctx")
            tot[arm][1] += m(xs, "tool_calls")
            tot[arm][2] += m(xs, "api_turns")
            tot[arm][3] += m(xs, "tool_result_chars")
        print(f"{task}   {len(a)}  {ag:>8.0f} {bg:>8.0f} {d:>+7.0f}%  {m(a,'tool_calls'):>7.1f} {m(b,'tool_calls'):>7.1f} "
              f"{m(a,'api_turns'):>8.1f} {m(b,'api_turns'):>7.1f}  {m(a,'tool_result_chars'):>7.0f} {m(b,'tool_result_chars'):>7.0f} "
              f"{m(a,'score'):>8.2f} {m(b,'score'):>7.2f}  {m(a,'duration_s'):>5.0f} {m(b,'duration_s'):>5.0f}")
    if deltas:
        print(f"sum over tasks: A growth {tot['A'][0]:.0f} vs B {tot['B'][0]:.0f} ({(tot['B'][0]-tot['A'][0])/tot['A'][0]*100:+.0f}%); "
              f"calls {tot['A'][1]:.1f} vs {tot['B'][1]:.1f}; turns {tot['A'][2]:.1f} vs {tot['B'][2]:.1f}; "
              f"tool chars {tot['A'][3]:.0f} vs {tot['B'][3]:.0f} ({(tot['B'][3]-tot['A'][3])/tot['A'][3]*100:+.0f}%)")
        print(f"per-task delta% in context growth: median {statistics.median(deltas):+.0f}%, min {min(deltas):+.0f}%, max {max(deltas):+.0f}%; "
              f"tasks where SymForge added less context: {sum(1 for d in deltas if d < 0)}/{len(deltas)}")


cohort(lambda r: "sonnet" in (r["model"] or ""), "Sonnet, 2 reps")
cohort(lambda r: "haiku" in (r["model"] or ""), "Haiku, 1 rep")
models = sorted({r["model"] for r in rows})
print("\nmodels:", models)
print("baseline first-turn context (tokens): min", min(r["first_ctx"] for r in rows), "max", max(r["first_ctx"] for r in rows))
json.dump(rows, open(sys.argv[1].replace(".json", "-graded.json"), "w", encoding="utf-8"), indent=1)
