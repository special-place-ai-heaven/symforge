# Live evidence — the unevidenced lifecycle reaches users

Observed by execution against a binary built from `main` at `00700698`
(`target/debug/symforge.exe`, driven over MCP by
`scripts/live-probe-lifecycle.cjs`). Not inferred from source.

## What was indexed

Two knowledge units, identical except for one line:

- `docs/declared.md` — contains `status: active`
- `docs/undeclared.md` — contains **no status line at all**

Both sit under a normal (non-archive) path, so neither hits the archive-path
rule.

## What `search_knowledge("widget")` returned

```
1. docs/declared.md:6   | ... | authority: lifecycle=active domain=current_implementation code=review_due voice=needs_review coverage=complete
2. docs/undeclared.md:5 | ... | authority: lifecycle=active domain=current_implementation code=review_due voice=needs_review coverage=complete
```

## Why this matters more than the code reading suggested

grok 4.6 found `derive_native_lifecycle` returning `(Active, LifecycleEvidence::None)`
for the undeclared case and proved it with a unit test. That establishes the defect
exists. This establishes that it **reaches users**, and how badly:

The fabricated value is rendered in **exactly the same syntax** as the evidenced
one. There is no marker, no hedge, no separate field — a reader of this output
cannot tell that unit 1 declared its lifecycle and unit 2 had one invented for
it. The evidence discriminator exists internally (`LifecycleEvidence::DeclaredSpan`
vs `LifecycleEvidence::None`) and is discarded before rendering.

That is the repository's binding reporting invariant violated at the surface a
user actually reads: *a component may not report success for an operation whose
completion it did not observe*. Here the component reports a **fact** it did not
observe, in a form indistinguishable from facts it did.

The knowledge-authority hygiene contract states the rule directly
(`contracts/knowledge-authority-hygiene.md:96-98`): "Lifecycle always cites
hash-valid policy or exact declared evidence. Code does not assign lifecycle."
`unknown` is already a legal value (`:87`).

## Consequence for the fix

`lifecycle=unknown` for the undeclared case is not a cosmetic change — it is the
difference between a user trusting a declaration and a user trusting a guess.
The fix should land on `main` independently of the rest of the sift work, which
is what grok's own triage recommended.

## Reproducing

```
cargo build --bin symforge
node scripts/live-probe-lifecycle.cjs target/debug/symforge.exe
```

The probe prints every line mentioning lifecycle and states plainly whether a
user-visible `lifecycle=active` was present. It contrasts a declared unit with an
undeclared one in the same index, so a change that made *everything* unknown
would be visible as the declared unit losing its evidenced `active`.
