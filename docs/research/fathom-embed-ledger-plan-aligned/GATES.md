# Gates: node 1, plan-aligned Fathom embed release integration

Scope: integrate leaves 1.1 to 1.7 (plan Tasks 1 to 7) into one verified release, checked out at the release tag, with the public surface changed additively only

Run from the SymForge repository root through Terminal Commander, checked out at
the release tag, with a longer outer timeout because N1 reruns every child:

    node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 43200 --reverify docs/research/fathom-embed-ledger-plan-aligned/GATES.md

No child claims ownership leases (tasks run one at a time on one branch), so the
template's lease-release gate is dropped.

- [ ] N0: every ledger in this package states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/GATES.md docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.1.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.2.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.3.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.4.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.5.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.6.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.7.md
  EXPECT: LINT OK
  EVIDENCE: pending

- [ ] N1: every child leaf is reverified from its exact ledger
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.1.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.2.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.3.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.4.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.5.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.6.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.7.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: pending

- [ ] N2: the checkout under verification is exactly the one release tag that carries every change request together
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs release --since v11.2.0 --cr CR-0 --cr CR-1 --cr CR-2 --cr CR-3 --cr F-1 --cr F-2 --together yes --head-is-tag yes
  EXPECT: /LEDGER-ORACLE RELEASE GREEN tag=v\d+\.\d+\.\d+ crs=CR-0,CR-1,CR-2,CR-3,F-1,F-2;/
  EVIDENCE: pending

- [ ] N3: the released public items changed additively only since v11.2.0
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: pending

- [ ] N4: the released contract keeps every v11.2.0 atom and records every census, progress and knowledge retrieval atom, with no other addition
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge --require-atom symforge::embed::IndexCensus --require-atom symforge::embed::CensusFile --require-atom symforge::embed::EmbeddedSourceHandle::index_census --require-atom symforge::embed::IndexProgress --require-atom symforge::embed::EmbeddedSourceHandle::index_progress --require-atom symforge::embed::KnowledgeSearchRequest --require-atom symforge::embed::KnowledgeSearchResult --require-atom symforge::embed::EmbeddedSourceHandle::search_knowledge
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: pending

- [ ] N5: the release adds no crate and changes no Cargo.toml dependency or patch table
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs deps --base v11.2.0 --allow-lock reqwest@0.13.5 --allow-lock rmcp@3.3.0 --allow-lock rmcp-macros@3.3.0 --allow-lock toml_edit@0.25.15+spec-1.1.0 --allow-lock toml_parser@1.1.3+spec-1.1.0
  EXPECT: LEDGER-ORACLE DEPS UNCHANGED base=v11.2.0
  EVIDENCE: pending

- [ ] N6: the public embed path still indexes, refreshes, queries and joins end to end at the release tag
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name public_embed_handle_indexes_refreshes_queries_and_joins -- test -j 8 --no-default-features --features embed --test embed_bound_index -- public_embed_handle_indexes_refreshes_queries_and_joins --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=public_embed_handle_indexes_refreshes_queries_and_joins
  EVIDENCE: pending

- [ ] N7: the binding verification cells pass at the release tag
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: pending

- [ ] N8: every contract inventory row in PLAN.md is reconciled against its owner and observation, and every manual record from the children was reviewed at node level
  RECORD: per C-row its disposition (ACTIVE met, ABANDONED with reason, REMOVED_BY_USER with the owner's words); the citation tables, red receipts, path-scope review, counter meanings review, Markdown-scan review and release record were read
  EVIDENCE: pending
