#!/usr/bin/env bash
# Assay oracle harness for the SIFT WS0/WS1 claims.
#
# Two worlds, run sequentially in this isolated worktree (never the real repo):
#   world A = code as-is
#   world B = counterfactual mutant applied
#
# A claim is PROVEN only if the exit codes DIFFER. Same exit codes mean the
# harness did not discriminate and therefore proves nothing.
#
# Usage:  ./assay-harness.sh <claim> <world>
#   claim in {global-limit, ccr-blocksafe}
#   world in {asis, mutant}
set -u

CLAIM="${1:?claim}"
WORLD="${2:?world}"

restore() {
  git checkout -- src/protocol/knowledge_search.rs src/protocol/tools.rs 2>/dev/null || true
}
trap restore EXIT

mutate() {
  python - "$1" "$2" "$3" <<'PY'
import sys
path, needle_file, repl_file = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path, encoding='utf-8').read()
needle = open(needle_file, encoding='utf-8').read()
repl = open(repl_file, encoding='utf-8').read()
if needle not in src:
    sys.exit("mutation site not found -- harness is stale")
open(path, 'w', encoding='utf-8').write(src.replace(needle, repl, 1))
PY
}

case "$CLAIM:$WORLD" in
  global-limit:asis|ccr-blocksafe:asis) ;;

  global-limit:mutant)
    # MUTANT: apply `limit` PER SOURCE before flattening -- the pre-WS0
    # behaviour. If the global-limit claim is real, the test must flip.
    cat > ./.assay-n.txt <<'EOF'
    hits.sort_by(rank_hits);
    let overflow = hits.len().saturating_sub(query.limit);
    hits.truncate(query.limit);
EOF
    cat > ./.assay-r.txt <<'EOF'
    hits.sort_by(rank_hits);
    let overflow = hits.len().saturating_sub(query.limit);
    // MUTANT: no GLOBAL truncation. Pre-WS0 applied `limit` per source inside
    // search_current and concatenated, so a 2-source scope returned 2 x limit.
EOF
    mutate src/protocol/knowledge_search.rs ./.assay-n.txt ./.assay-r.txt || { echo "MUTATION_FAILED"; exit 99; }
    ;;

  ccr-blocksafe:mutant)
    # MUTANT: revert to the generic line-boundary budget helper. If the
    # block-safe claim is real, the budget sweep must flip.
    cat > ./.assay-n.txt <<'EOF'
        self.apply_ccr_budget_with_summary(
            "search_knowledge",
            output.rendered,
            output.budget_rendered,
            params.0.max_tokens,
        )
EOF
    cat > ./.assay-r.txt <<'EOF'
        self.apply_ccr_budget("search_knowledge", output.rendered, params.0.max_tokens)
EOF
    mutate src/protocol/tools.rs ./.assay-n.txt ./.assay-r.txt || { echo "MUTATION_FAILED"; exit 99; }
    ;;

  *) echo "unknown claim/world"; exit 98 ;;
esac

case "$CLAIM" in
  global-limit)
    cargo test --lib \
      protocol::knowledge_search::tests::global_limit_and_counts_apply_once_across_sources \
      -- --exact >/dev/null 2>&1
    ;;
  ccr-blocksafe)
    cargo test --test search_knowledge \
      ccr_truncation_withholds_partial_hits_and_round_trips_full_safe_output \
      -- --exact >/dev/null 2>&1
    ;;
esac
echo "EXIT=$?"
