#!/usr/bin/env python3
"""SCORING FILE — LOCKED. Read-only for the AI. Never edit to make a round score higher.

Defines the objective measuring stick for the token-cost research loop:
- the frozen fixture set (tool + args) every round is scored against
- the real-tokenizer counter (tiktoken cl100k_base, NOT the codebase's own
  chars/4 estimator in handler.rs, which is a heuristic and not truth)

Usage (per round):
    1. Run each fixture's MCP tool call live against the current build.
    2. Paste/pipe the raw response text into count_tokens().
    3. Sum across fixtures -> the round's single score.

This file defines WHAT to measure. It does not itself drive the MCP
connection (there is no local build to shell out to in this environment;
see research/token-cost/instructions.md for why). The loop runner executes
the fixture calls via the connected MCP session and feeds output here.
"""

import sys

try:
    import tiktoken
    _ENC = tiktoken.get_encoding("cl100k_base")
except ImportError:
    print("tiktoken required: pip install tiktoken", file=sys.stderr)
    raise


def count_tokens(text: str) -> int:
    """The single objective number for a piece of tool output. Real tokenizer, not a heuristic."""
    return len(_ENC.encode(text))


# Frozen fixtures. Do not add/remove/reword without human sign-off
# (instructions.md rule 7) — changing these mid-run changes what "biggest
# win" means and breaks round-to-round comparability.
FIXTURES = [
    {
        "id": "F1-get_symbol-large-fn",
        "tool": "get_symbol",
        "args": {"name": "search_text_result_view", "path": "src/protocol/format.rs"},
        "why": "largest suspected single payload: full verbatim body dump (format.rs)",
    },
    {
        "id": "F2-search_text-default",
        "tool": "search_text",
        "args": {"query": "estimate_tokens", "path": "src/protocol"},
        "why": "default (non-symbol-grouped) view repeats per-match header block",
    },
    {
        "id": "F3-repo_map-compact",
        "tool": "get_repo_map",
        "args": {"detail": "compact"},
        "why": "one padded row per directory; scales with directory/file count (path scoping only applies to detail=tree, not compact/full)",
    },
    {
        "id": "F4-symbol_context-callers",
        "tool": "get_symbol_context",
        "args": {"name": "estimate_tokens", "project": "symforge"},
        "why": "fixed-width padded caller/callee/type-dep rows repeat file paths",
    },
    {
        "id": "F5-find_references-verbose",
        "tool": "find_references",
        "args": {"name": "OutputLimits", "project": "symforge"},
        "why": "default verbose per-hit view vs the existing compact variant",
    },
]


def score_round(fixture_id: str, raw_output: str) -> int:
    """Score one fixture's output for one round. This IS the objective number."""
    return count_tokens(raw_output)


if __name__ == "__main__":
    # Smoke test: confirm the counter works. Does not run fixtures (needs a live MCP call).
    sample = "hello world, this is a token counting smoke test"
    print(f"tiktoken smoke test: {count_tokens(sample)} tokens for {len(sample)} chars")
    print(f"Fixtures defined: {len(FIXTURES)}")
    for f in FIXTURES:
        print(f"  {f['id']}: {f['tool']}({f['args']}) — {f['why']}")
