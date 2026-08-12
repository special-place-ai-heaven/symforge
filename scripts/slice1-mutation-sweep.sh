#!/usr/bin/env bash
# Slice 1 guard mutation sweep (T029 evidence).
#
# Each Slice 1 test pairs its negative with the accepting case, so no test can
# pass by refusing everything. This sweep proves the other half: that each guard
# is load-bearing. It reverts one guard at a time and records which tests fail.
# A guard whose removal breaks nothing is not a guard.
#
# Usage: slice1-mutation-sweep.sh <batch>   (batch = 1 or 2)
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

AUTHORITY=src/index_lifecycle/authority.rs
MUTATION=src/index_lifecycle/mutation.rs
TRANSITION=src/index_lifecycle/transition.rs
PHYSICAL=src/index_lifecycle/physical_root.rs

# id | file | literal to replace | replacement | guard description
MUTATIONS=(
  "publication-identity|$AUTHORITY|if presented != live.publication() {|if false {|grant validates the exact live publication identity"
  "root-pairing|$MUTATION|if authority.binding().physical_root() != lease.identity() {|if false {|permit pairs a grant only with its own root lease"
  "transition-drain|$TRANSITION|if !signal.has_ended() {|if false {|transition refuses to install over a live permit"
  "install-revokes|$TRANSITION|outgoing.revoke();||install revokes the outgoing root lease"
  "permit-terminality|$MUTATION|if matches!(self.state, PermitState::Terminal(_)) {|if false {|a terminal permit refuses a second termination"
  "drop-drains|$MUTATION|self.drain.record(Termination::Drained);||a dropped permit reports Drained"
  "lease-revoked|$PHYSICAL|if !self.is_live() {|if false {|a revoked lease resolves nothing"
  "temp-first|$PHYSICAL|steps.push(ReplacementStep::TempCreated);|steps.push(ReplacementStep::Replaced);|replacement creates its temporary before replacing"
)

BATCH="${1:-1}"
if [[ "$BATCH" == "1" ]]; then
  SELECTED=("${MUTATIONS[@]:0:4}")
else
  SELECTED=("${MUTATIONS[@]:4:4}")
fi

restore() {
  git checkout -- "$AUTHORITY" "$MUTATION" "$TRANSITION" "$PHYSICAL" 2>/dev/null
}
trap restore EXIT

for entry in "${SELECTED[@]}"; do
  IFS='|' read -r id file needle replacement description <<<"$entry"

  if ! grep -qF -- "$needle" "$file"; then
    echo "SWEEP $id: SKIPPED (guard literal not found in $file)"
    continue
  fi

  python -c "
import sys
path, needle, replacement = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding='utf-8') as handle:
    source = handle.read()
if source.count(needle) != 1:
    raise SystemExit(f'guard literal is not unique in {path} ({source.count(needle)} matches)')
with open(path, 'w', encoding='utf-8', newline='') as handle:
    handle.write(source.replace(needle, replacement))
" "$file" "$needle" "$replacement" || { echo "SWEEP $id: SETUP FAILED"; restore; continue; }

  output="$(cargo test --test project_index_authority_v11 --test physical_root_lease_v11 \
    -- --test-threads=1 2>&1)"

  if grep -qE "^error(\[|:)" <<<"$output"; then
    echo "SWEEP $id: DID NOT COMPILE (guard removal is prevented by the type system)"
  else
    failed="$(grep -E "^test .* \.\.\. FAILED$" <<<"$output" | sed 's/^test //; s/ \.\.\. FAILED$//' | paste -sd, -)"
    if [[ -z "$failed" ]]; then
      echo "SWEEP $id: *** NO TEST FAILED *** guard is not covered: $description"
    else
      echo "SWEEP $id: caught by [$failed] -- $description"
    fi
  fi

  restore
done
