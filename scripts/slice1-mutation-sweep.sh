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

AUTHORITY=src/live_index/index_lifecycle/authority.rs
MUTATION=src/live_index/index_lifecycle/mutation.rs
TRANSITION=src/live_index/index_lifecycle/transition.rs
PHYSICAL=src/live_index/index_lifecycle/physical_root.rs

# id | file | literal to replace | replacement | guard description
MUTATIONS=(
  "publication-identity|$AUTHORITY|if presented != live.publication() {|if false {|grant validates the exact live publication identity"
  "root-pairing|$MUTATION|if authority.binding().physical_root() != lease.identity() {|if false {|permit pairs a grant only with its own root lease"
  # These two keep their binding in use on purpose. Replacing them with a bare
  # `if false` or an empty statement leaves an unused parameter, and this crate
  # denies warnings, so the build fails for a reason unrelated to the guard --
  # which reads as "uncompilable" and proves nothing.
  # Both drain checks are reverted together (expect=2). Reverting only the
  # precondition leaves the post-freeze re-observation to catch it, and vice
  # versa, so a single-site mutation would report "caught" while proving only
  # that one of two redundant checks fires.
  "transition-drain|$TRANSITION|if !outstanding.has_ended() {|if outstanding.has_ended() {|transition refuses to install over a live permit|2"
  "install-revokes|$TRANSITION|outgoing.revoke();|let _ = &outgoing;|install revokes the outgoing root lease"
  "permit-terminality|$MUTATION|if matches!(self.state, PermitState::Terminal(_)) {|if false {|a terminal permit refuses a second termination"
  "drop-drains|$MUTATION|self.drain.record(Termination::Drained);||a dropped permit reports Drained"
  "lease-revoked|$PHYSICAL|if !self.is_live() {|if false {|a revoked lease resolves nothing"
  # Reviewer grok-4-5 is right that this proves less than the others: it reverts
  # the RECEIPT LABEL, so what it demonstrates is that the receipt's recorded
  # order is load-bearing -- not that the underlying write/rename order is. A
  # build that renamed first while pushing the labels in order would stay green
  # here. Closing that needs an oracle able to observe the target mid-flight,
  # which needs a seam this slice does not have; it is recorded as an open limit
  # in the evidence document rather than papered over with a mutation whose
  # effect no test can see. The label is described as what it is.
  "temp-first-label|$PHYSICAL|steps.push(ReplacementStep::TempCreated);|steps.push(ReplacementStep::Replaced);|the receipt's recorded temp-before-replace order is load-bearing"
  "epoch-monotonic|$AUTHORITY|self.mutation_epoch = self.mutation_epoch.advanced();|let _ = self.mutation_epoch.advanced();|the mutation epoch is monotonic across freeze"
  "proof-names-stored|$AUTHORITY|.expect(\"a Current source always has a publication to freeze\")|.and(Some(PublicationIdentity::fresh())).expect(\"mutated\")|the non-Current proof names the stored publication"
  # The two defects three independent reviews found. Both were live while every
  # gate this slice had was green, so both earn a permanent guard.
  "commit-receipt-lease|$MUTATION|if receipt.lease() != self.lease.identity() {|if receipt.lease() != receipt.lease() {|commit refuses a receipt from another lease"
  "drain-arms-on-grant|$MUTATION|drain.arm();|let _ = &drain;|a signal reports outstanding once a permit is attached"
)

# Batches of four keep each run inside the tool's wall-clock ceiling.
BATCH="${1:-1}"
OFFSET=$(( (BATCH - 1) * 4 ))
SELECTED=("${MUTATIONS[@]:$OFFSET:4}")
if [[ ${#SELECTED[@]} -eq 0 ]]; then
  echo "SWEEP: batch $BATCH is empty (${#MUTATIONS[@]} mutations defined)"
  exit 1
fi

# The sweep restores by discarding working-tree changes to these files, so any
# uncommitted work in them would be destroyed. Refuse rather than eat it. This
# is not hypothetical: a run with three uncommitted fixes in the tree reverted
# all three, and the only surviving evidence was that the tests stopped
# compiling.
DIRTY="$(git status --porcelain -- "$AUTHORITY" "$MUTATION" "$TRANSITION" "$PHYSICAL")"
if [[ -n "$DIRTY" ]]; then
  echo "SWEEP: refusing to run with uncommitted changes in the files it mutates:"
  echo "$DIRTY"
  echo "SWEEP: commit or stash them first -- the restore step discards them."
  exit 1
fi

STALE=0

restore() {
  git checkout -- "$AUTHORITY" "$MUTATION" "$TRANSITION" "$PHYSICAL" 2>/dev/null
}
trap restore EXIT

for entry in "${SELECTED[@]}"; do
  IFS='|' read -r id file needle replacement description expect <<<"$entry"
  expect="${expect:-1}"

  # A literal that no longer matches means the guard silently lost its coverage,
  # which is the very failure this sweep exists to detect. Report it as a
  # failure, not as a skip: a refactor that renames a guard must not quietly
  # remove it from the evidence.
  if ! grep -qF -- "$needle" "$file"; then
    echo "SWEEP $id: *** LITERAL NOT FOUND *** in $file -- this guard is UNVERIFIED"
    STALE=1
    continue
  fi

  python -c "
import sys
path, needle, replacement, expect = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
with open(path, encoding='utf-8') as handle:
    source = handle.read()
found = source.count(needle)
if found != expect:
    raise SystemExit(f'guard literal appears {found} times in {path}, expected {expect}')
with open(path, 'w', encoding='utf-8', newline='') as handle:
    handle.write(source.replace(needle, replacement))
" "$file" "$needle" "$replacement" "$expect" || { echo "SWEEP $id: SETUP FAILED"; restore; STALE=1; continue; }

  output="$(cargo test --test project_index_authority_v11 --test physical_root_lease_v11 \
    -- --test-threads=1 2>&1)"

  # Observed test failures are checked FIRST and outrank everything else.
  # `cargo test` prints "error: test failed" on a failing run, so matching a
  # leading "error:" and calling it a compile failure reports a conclusion the
  # script never observed -- which is how the first version of this sweep
  # mislabelled four caught guards as uncompilable.
  failed="$(grep -E "^test .* \.\.\. FAILED$" <<<"$output" | sed 's/^test //; s/ \.\.\. FAILED$//' | paste -sd, -)"
  if [[ -n "$failed" ]]; then
    echo "SWEEP $id: caught by [$failed] -- $description"
  elif grep -qE "could not compile|^error\[E" <<<"$output"; then
    # Show WHY. A bare "did not compile" is unfalsifiable: it reads as evidence
    # about the guard when it is usually evidence about the mutation.
    reason="$(grep -E "^error(\[E[0-9]+\])?:" <<<"$output" | head -2 | paste -sd'; ' -)"
    echo "SWEEP $id: DID NOT COMPILE -- $reason"
  else
    echo "SWEEP $id: *** NO TEST FAILED *** guard is not covered: $description"
  fi

  restore
done

# Exit non-zero when any guard went unverified, so a stale literal cannot be
# mistaken for a clean run by anything reading the exit status.
exit "$STALE"
