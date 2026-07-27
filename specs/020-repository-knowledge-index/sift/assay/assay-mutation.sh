#!/usr/bin/env bash
# Assay: are the NEW WS1 tests capable of failing?
#
# A test that cannot fail is not evidence. Each mutant below breaks exactly the
# property one new test claims to protect. The test MUST flip (exit != 0). If it
# stays green, that test proves nothing and must be reported as such.
#
# Usage: ./assay-mutation.sh <mutant>|all
set -u

KS=src/protocol/knowledge_search.rs
restore() { git checkout -- "$KS" src/live_index/knowledge_bridge.rs 2>/dev/null || true; }
trap restore EXIT

# sanity: a filter that matches nothing exits 0. Never trust a bare exit code.
count_match() { # <target-args...>
  "$@" --list 2>/dev/null | grep -c ": test$" || true
}

py() { python -c "$1"; }

mutate() {
  python - "$1" "$2" "$3" <<'PY'
import sys
path, nf, rf = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path, encoding='utf-8').read()
needle = open(nf, encoding='utf-8').read()
repl = open(rf, encoding='utf-8').read()
if needle not in src:
    sys.exit("mutation site not found -- harness is stale")
open(path, 'w', encoding='utf-8').write(src.replace(needle, repl, 1))
PY
}


apply() {
  case "$1" in
    # 1. Answer-first ordering: put provenance BEFORE the excerpt.
    #    -> answer_arrives_before_provenance_and_the_envelope_stays_bounded
    order)
      printf '%s' '
   {heading}
   \"{}\"
   source=' > ./.n.txt
      printf '%s' '
   {heading}
   source=' > ./.r.txt
      mutate src/protocol/knowledge_search.rs ./.n.txt ./.r.txt || return 1
      # excerpt now missing from the format string -> arg count mismatch -> the
      # block no longer renders the excerpt third. Re-append it last so the
      # block still CONTAINS the excerpt, just after the provenance.
      printf '%s' '   bridge_previews=[{}] omitted={}"' > ./.n.txt
      printf '%s' '   bridge_previews=[{}] omitted={}
   \"{}\""' > ./.r.txt
      mutate src/protocol/knowledge_search.rs ./.n.txt ./.r.txt || return 1
      ;;
    # 2. Bounded envelope: stop abbreviating source_id / manifest_digest.
    #    -> answer_arrives_before_provenance_and_the_envelope_stays_bounded
    envelope)
      printf '%s' 'ids.render(envelope.source.source_id.as_str()),' > ./.n.txt
      printf '%s' 'envelope.source.source_id.as_str().to_string(),' > ./.r.txt
      mutate src/protocol/knowledge_search.rs ./.n.txt ./.r.txt || return 1
      printf '%s' 'ids.render(&envelope.manifest_digest),' > ./.n.txt
      printf '%s' 'envelope.manifest_digest.clone(),' > ./.r.txt
      mutate src/protocol/knowledge_search.rs ./.n.txt ./.r.txt || return 1
      ;;
    # 3. No-match seam: rename the classifier's literal.
    #    -> no_match_seam_keeps_its_exact_prefix_and_position
    seam)
      py "
p='$KS'; s=open(p,encoding='utf-8').read()
old='output.push_str(&format!(\"\\\\nNo match: {no_match}\"));'
assert old in s, 'seam mutation site not found'
s=s.replace(old,'output.push_str(&format!(\"\\\\nNoMatch- {no_match}\"));',1)
open(p,'w',encoding='utf-8').write(s)
"
      ;;
    # 4. Excerpt bounding: return the raw matched line, uncapped.
    #    -> excerpt_is_bounded_* / excerpt_cuts_on_character_boundaries_*
    excerpt)
      py "
p='$KS'; s=open(p,encoding='utf-8').read()
old='    let chars: Vec<char> = line.chars().collect();'
assert old in s, 'excerpt mutation site not found'
s=s.replace(old,'    return line.to_string();\n    #[allow(unreachable_code)]\n    let chars: Vec<char> = line.chars().collect();',1)
open(p,'w',encoding='utf-8').write(s)
"
      ;;
    # 5. Digest abbreviation: freeze at 12, never extend on collision.
    #    -> forced_digest_prefix_collision_extends_until_unique
    digest)
      py "
p='$KS'; s=open(p,encoding='utf-8').read()
old='        let mut digest_len = DIGEST_PREFIX_MIN;'
assert old in s, 'digest mutation site not found'
s=s.replace(old,'        let digest_len = DIGEST_PREFIX_MIN;\n        #[allow(unused_mut)] let mut _unused = 0usize;\n        if false { let mut digest_len = digest_len; digest_len += 1; let _ = digest_len; }',1)
# neuter the extend loop
old2='        while digest_len < longest {'
assert old2 in s, 'digest loop site not found'
s=s.replace(old2,'        while false {',1)
open(p,'w',encoding='utf-8').write(s)
"
      ;;
    # 6. Anchor rendering: restore the Rust-debug leak.
    #    -> code_anchor_label_is_agent_readable_and_never_rust_debug
    anchor)
      py "
p='src/live_index/knowledge_bridge.rs'; s=open(p,encoding='utf-8').read()
old='format!(\"symbol:{}#{}:{start_line}\", symbol.path, symbol.name)'
assert old in s, 'anchor mutation site not found'
s=s.replace(old,'format!(\"symbol:{symbol:?}:{start_line}\")',1)
open(p,'w',encoding='utf-8').write(s)
"
      ;;
    # 7. ID typing: treat EVERY id as a digest, corrupting semantic rule IDs.
    #    -> semantic_ids_render_verbatim_and_only_digests_abbreviate
    semantic)
      py "
p='$KS'; s=open(p,encoding='utf-8').read()
old='        id.len() >= DIGEST_PREFIX_MIN'
assert old in s, 'semantic mutation site not found'
s=s.replace(old,'        id.len() >= DIGEST_PREFIX_MIN || true',1)
open(p,'w',encoding='utf-8').write(s)
"
      ;;
    # 8. Source labelling: hardcode every hit back to `current`.
    #    -> hits_carry_real_source_labels_not_hardcoded_current
    labels)
      py "
p='$KS'; s=open(p,encoding='utf-8').read()
old='            source_label: label.clone(),'
assert old in s, 'labels mutation site not found'
s=s.replace(old,'            source_label: \"current\".to_string(),',1)
open(p,'w',encoding='utf-8').write(s)
"
      ;;
    *) echo "unknown mutant: $1"; exit 98 ;;
  esac
}

run_test() { # <kind> <name>
  case "$1" in
    lib)  cargo test --lib "$2" -- --exact >/dev/null 2>&1 ;;
    int)  cargo test --test search_knowledge "$2" -- --exact >/dev/null 2>&1 ;;
  esac
  echo $?
}

declare -a CASES=(
  "order|int|answer_arrives_before_provenance_and_the_envelope_stays_bounded"
  "envelope|int|answer_arrives_before_provenance_and_the_envelope_stays_bounded"
  "seam|int|no_match_seam_keeps_its_exact_prefix_and_position"
  "excerpt|lib|protocol::knowledge_search::tests::excerpt_is_bounded_and_keeps_the_match_in_window"
  "digest|lib|protocol::knowledge_search::tests::forced_digest_prefix_collision_extends_until_unique"
  "anchor|lib|live_index::knowledge_bridge::tests::code_anchor_label_is_agent_readable_and_never_rust_debug"
  "semantic|lib|protocol::knowledge_search::tests::semantic_ids_render_verbatim_and_only_digests_abbreviate"
  "labels|lib|protocol::knowledge_search::tests::hits_carry_real_source_labels_not_hardcoded_current"
)

WANT="${1:-all}"
printf "%-10s %-64s %-8s %-8s %s\n" MUTANT TEST BASELINE MUTATED VERDICT
for c in "${CASES[@]}"; do
  IFS='|' read -r mut kind name <<< "$c"
  [ "$WANT" != "all" ] && [ "$WANT" != "$mut" ] && continue

  restore
  # Guard against the zero-match trap: the filter must select exactly 1 test.
  if [ "$kind" = lib ]; then n=$(count_match cargo test --lib "$name" --); else n=$(count_match cargo test --test search_knowledge "$name" --); fi
  if [ "${n:-0}" -ne 1 ]; then
    printf "%-10s %-64s %-8s %-8s %s\n" "$mut" "$name" "-" "-" "VOID(filter matched ${n:-0} tests)"
    continue
  fi

  base=$(run_test "$kind" "$name")
  apply "$mut" || { printf "%-10s %-64s %s\n" "$mut" "$name" "VOID(mutation failed)"; continue; }
  mutated=$(run_test "$kind" "$name")
  restore

  if [ "$base" = "0" ] && [ "$mutated" != "0" ]; then v="PROVEN(can fail)"; else v="INERT(proves nothing)"; fi
  printf "%-10s %-64s %-8s %-8s %s\n" "$mut" "$name" "$base" "$mutated" "$v"
done
