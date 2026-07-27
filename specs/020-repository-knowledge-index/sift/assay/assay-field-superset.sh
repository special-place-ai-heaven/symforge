#!/usr/bin/env bash
# Claim 1 receipt: the three functions removed in WS0 were each replaced by
# something that emits AT LEAST the same field tokens.
#
# Method: extract every `key=` token from the format strings of the OLD
# functions at the pre-WS0 commit, then assert each appears in the NEW render
# path. Exit 0 = no field lost. Exit 1 = a field was dropped (names printed).
#
# Honest limit: this proves each token is EMITTED SOMEWHERE in the new render
# path. It does not prove it is emitted in every branch. The behavioural half of
# that is carried by the unit tests (withheld-provenance, readiness, contract 11).
set -u
BASE="${1:-ff4302c}"   # last commit before the WS0 refactor

extract_tokens() { grep -oE '[a-z_]+=' | sort -u; }

echo "=== OLD tokens: render_source_scope_identity + render_source_withheld_response @ $BASE ==="
git show "$BASE:src/protocol/knowledge_search.rs" \
  | awk '/fn render_source_scope_identity/,/^}/' \
  | extract_tokens > ./.assay-old-identity.txt
git show "$BASE:src/protocol/knowledge_search.rs" \
  | awk '/fn render_source_withheld_response/,/^}/' \
  | extract_tokens > ./.assay-old-withheld.txt
cat ./.assay-old-identity.txt ./.assay-old-withheld.txt | sort -u > ./.assay-old-all.txt
wc -l < ./.assay-old-all.txt | xargs echo "  old token count:"

echo "=== NEW tokens: render_source_line + render_response + render_hit_block @ HEAD ==="
{
  awk '/fn render_source_line/,/^}/'  src/protocol/knowledge_search.rs
  awk '/fn render_response/,/^}/'     src/protocol/knowledge_search.rs
  awk '/fn render_hit_block/,/^}/'    src/protocol/knowledge_search.rs
} | extract_tokens > ./.assay-new-all.txt
wc -l < ./.assay-new-all.txt | xargs echo "  new token count:"

# Declared renames: same VALUE, different spelling. WS0 unified the multi-source
# per-source line onto the single-source `Source:` spelling, which has always
# used publication=/content=. Verified: the only consumers of the old spelling
# are knowledge_curation.rs / curate_knowledge.rs -- a DIFFERENT tool's output
# surface, untouched by this slice.
#   publication_generation=  ->  publication=
#   content_generation=      ->  content=
sed -e 's/^publication_generation=$/publication=/' \
    -e 's/^content_generation=$/content=/' \
    ./.assay-old-all.txt | sort -u > ./.assay-old-renamed.txt
mv ./.assay-old-renamed.txt ./.assay-old-all.txt

echo "=== fields present in OLD but absent from NEW (must be empty) ==="
missing=$(comm -23 ./.assay-old-all.txt ./.assay-new-all.txt)
if [ -n "$missing" ]; then
  echo "$missing" | sed 's/^/  DROPPED: /'
  exit 1
fi
echo "  (none)"

echo "=== fields NEW adds over OLD ==="
comm -13 ./.assay-old-all.txt ./.assay-new-all.txt | sed 's/^/  ADDED: /'
exit 0
