# Risk Register — Program 015

| ID | Risk | L | I | Score | Trigger | Mitigation | Owner | Sprint |
|----|------|---|---|-------|---------|------------|-------|--------|
| R-01 | SQLite Soul Map creep | M | H | H | New query path reads `.db` not LiveIndex | Constitution check each PR; grep gate | Arch | all |
| R-02 | Snapshot v5 break | H | H | H | ResolvedCall in snapshot | Migration test + version bump spec | Engine | S3 |
| R-03 | Resolver scope creep | H | M | H | >4 languages in one sprint | Language milestones in S3 spec | Parsing | S3 |
| R-04 | Graph memory blowup | M | H | H | >500k edges in memory | Lazy build; depth caps; CCR | Engine | S2 |
| R-05 | Compact schema budget break | M | M | M | 4th default tool | STEL intents only | Protocol | S1+ |
| R-06 | Windows git porcelain diff | M | M | M | impact misses untracked | Port CBM #520 merge; Windows CI | Git | S1 |
| R-07 | zstd dependency rejection | L | M | L | No new deps policy | gzip fallback (R11 ponytail) | Engine | S1 |
| R-08 | Cypher scope creep | M | M | M | Full openCypher requests | Fail-closed subset; decision-log | Engine | S2 |
| R-09 | 012 cross-project conflict | M | M | M | project param divergence | Single-project first; 012 owns merge | Daemon | S2+ |
| R-10 | Perf regression index | M | H | H | Index 2x slower | Modes fast/standard/deep; perf smoke | Engine | S1 |
| R-11 | Hook latency >100ms | M | L | L | Sidecar cold | Inline index read fallback | CLI | S1 |
| R-12 | False resolver confidence | H | M | H | Wrong call edges | confidence + strategy disclosure | Parsing | S3 |
| R13 | Semantic false positives | M | M | M | Noisy related edges | threshold env; deep mode only | Engine | S4 |
| R-14 | Team artifact secret leak | L | H | M | Commit secrets in snapshot | index excludes .env; gitignore docs | Ops | S1 |
| R-15 | Program fatigue / skip planning | M | H | H | [C] before [P] | execution-model gates enforced | PM | all |

**L**=Likelihood **I**=Impact **Score**=combined judgment

## Escalation

- **H score**: blocks Planning Gate until mitigated or accepted in decision-log
- **M score**: document mitigation in sprint spec
- **L score**: monitor

## Review cadence

- End of each sprint `[V]` phase: refresh scores
- New risks → add row + decision-log if architectural
