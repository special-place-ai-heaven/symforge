# Gates: node 1, plan-aligned Fathom embed release integration

Scope: integrate leaves 1.1 to 1.7 (plan Tasks 1 to 7) into one verified release, checked out at the release tag, with the public surface changed additively only

Run from the SymForge repository root through Terminal Commander, checked out at
the release tag, with a longer outer timeout because N1 reruns every child:

    node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 43200 --reverify docs/research/fathom-embed-ledger-plan-aligned/GATES.md

No child claims ownership leases (tasks run one at a time on one branch), so the
template's lease-release gate is dropped.

- [x] N0: every ledger in this package states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/GATES.md docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.1.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.2.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.3.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.4.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.5.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.6.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.7.md
  EXPECT: LINT OK
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=686f9bb4ea6b5c332556ab42f38c6f46a65b23816ecb25028c042977294282ed; output-bytes=2726

- [ ] N1: every child leaf is reverified from its exact ledger
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.1.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.2.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.3.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.4.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.5.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.6.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.7.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: pending

- [ ] N2: the checkout under verification is exactly the one release tag that carries every change request together
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs release --since v11.2.0 --cr CR-0 --cr CR-1 --cr CR-2 --cr CR-3 --cr F-1 --cr F-2 --together yes --head-is-tag yes
  EXPECT: /LEDGER-ORACLE RELEASE GREEN tag=v\d+\.\d+\.\d+ crs=CR-0,CR-1,CR-2,CR-3,F-1,F-2;/
  EVIDENCE: pending

- [x] N3: the released public items changed additively only since v11.2.0
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs api --base v11.2.0 --path src/index_lifecycle/embedded.rs --path src/index_lifecycle/public_api.rs --path src/embed.rs --path src/lifecycle_identity.rs --allow index_census --allow IndexCensus --allow CensusFile --allow index_progress --allow IndexProgress --allow search_knowledge --allow KnowledgeSearchRequest --allow KnowledgeSearchResult --allow KnowledgeMatch --extend OperationKind=IndexCensus,SearchKnowledge --frozen RetryAdvice --frozen SourceRefusalKind --frozen SourceRuntimePhase --frozen SourceRuntimeView
  EXPECT: LEDGER-ORACLE API ADDITIVE base=v11.2.0;
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=336cd67d8d6efa261561b22d7c4470d08414a3dd69a15320bc5f11a85fe31c5d; output-bytes=41

- [x] N4: the released contract keeps every v11.2.0 atom and records every census, progress and knowledge retrieval atom, with no other addition
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs atoms --base v11.2.0 --contract specs/020-repository-knowledge-index/contracts/public-api-v11.json --allow-prefix symforge::embed::IndexCensus --allow-prefix symforge::embed::CensusFile --allow-prefix symforge::embed::IndexProgress --allow-prefix symforge::embed::KnowledgeSearchRequest --allow-prefix symforge::embed::KnowledgeSearchResult --allow-prefix symforge::embed::KnowledgeMatch --allow-prefix symforge::embed::EmbeddedSourceHandle::index_census --allow-prefix symforge::embed::EmbeddedSourceHandle::index_progress --allow-prefix symforge::embed::EmbeddedSourceHandle::search_knowledge --require-atom symforge::embed::IndexCensus --require-atom symforge::embed::CensusFile --require-atom symforge::embed::EmbeddedSourceHandle::index_census --require-atom symforge::embed::IndexProgress --require-atom symforge::embed::EmbeddedSourceHandle::index_progress --require-atom symforge::embed::KnowledgeSearchRequest --require-atom symforge::embed::KnowledgeSearchResult --require-atom symforge::embed::EmbeddedSourceHandle::search_knowledge
  EXPECT: LEDGER-ORACLE ATOMS ADDITIVE base=v11.2.0
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=955ab01ab9bdb405ec230e26b2e547aec3da7775c86d5ed3220948bb2b007641; output-bytes=51

- [x] N5: the release adds no crate and changes no Cargo.toml dependency or patch table
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs deps --base v11.2.0 --allow-lock reqwest@0.13.5 --allow-lock rmcp@3.3.0 --allow-lock rmcp-macros@3.3.0 --allow-lock toml_edit@0.25.15+spec-1.1.0 --allow-lock toml_parser@1.1.3+spec-1.1.0
  EXPECT: LEDGER-ORACLE DEPS UNCHANGED base=v11.2.0
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=5ed8afb53eee958b3b2e989fbc8073d92fcdf60fda9cd4cafe4615dd609242f6; output-bytes=56

- [x] N6: the public embed path still indexes, refreshes, queries and joins end to end at the release tag
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs tests --tests 1 --name public_embed_handle_indexes_refreshes_queries_and_joins -- test -j 8 --no-default-features --features embed --test embed_bound_index -- public_embed_handle_indexes_refreshes_queries_and_joins --test-threads=1
  EXPECT: LEDGER-ORACLE TESTS GREEN tests=1; names=public_embed_handle_indexes_refreshes_queries_and_joins
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=ea5ef4a56ffecaaabc60a5841c953afe72652e89fa03a26b54639ab08104e008; output-bytes=97

- [ ] N7: the binding verification cells pass at the release tag
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md
  EXPECT: /^ALL MET \(/m
  EVIDENCE: pending

- [x] N8: every contract inventory row in PLAN.md is reconciled against its owner and observation, and every manual record from the children was reviewed at node level
  RECORD: per C-row its disposition (ACTIVE met, ABANDONED with reason, REMOVED_BY_USER with the owner's words); the citation tables, red receipts, path-scope review, counter meanings review, Markdown-scan review and release record were read
  EVIDENCE: 2026-09-16 reviewed PLAN.md inventory and leaves 1.1-1.6 ALL MET. C1-C22, C26-C29 ACTIVE met on those leaves. C23-C25 ACTIVE still open: they require the owner's ship words (PR/merge/tag) and leaf-1.7. No ABANDONED or REMOVED_BY_USER rows. Child citation tables, C28 counter meanings, C29 knowledge shape, and the published-generation Markdown-scan record were read. N1/N2/N7 stay unchecked until the tag.
