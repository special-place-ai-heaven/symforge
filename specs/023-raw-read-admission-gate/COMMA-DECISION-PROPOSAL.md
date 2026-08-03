# Proposal v2 — the depth-0 comma, after adversarial attack

Date: 2026-07-31 · Branch `fix/raw-read-admission-gate` @ `20b51c8` · nothing committed

**v1 of this document proposed D1–D4. An adversarial pass killed D2 and D3.
This version supersedes it. Ship D1 only.**

Inputs: two independent reviews, a four-lens adversarial attack on v1 (9 findings
survived verification), and my own measurements confirming or correcting each
load-bearing claim.

---

## 1. Verdict

| Decision | Status |
|---|---|
| **D1** — on code paths a depth-0 comma is always a continuation, never a terminator | **SHIP.** Proven safe; fixes a live leak. |
| **D2** — on non-code paths a depth-0 comma terminates the walk | **DEAD.** Four independent falsifications, three measured. |
| **D3** — resume the walk on a swallowed structural comma | **DEAD, and worse:** it manufactures the exemption D2 then grants. |
| **D4** — require the quote mate before stepping | **Defer to its own commit**, with its primitive reused (see §5). |

---

## 2. D1 survives, with a proof rather than an intuition

D1 is **purely additive** to `CONTINUATION` and adds no terminator. So pre- and
post-D1 walks are byte-identical until the first newline where pre-D1 declines to
continue. That stop is necessarily at depth 0 and untruncated, so pre-D1 returns
FALSE (consumed) there. Continuing past a FALSE stop can only reach another FALSE
arm (same verdict) or a TRUE arm / truncation (strictly more SENSITIVE).
**TRUE → FALSE is unreachable, so D1 alone cannot manufacture a false negative.**

That is a monotonicity argument, not a sampled test, and it is the only claim in
this campaign backed by something stronger than rows.

- **Fixes C5**, measured CLEAN at `20b51c8` in JS and Rust multi-line declarator
  shapes, with the one-line control SENSITIVE.
- **Costs** the struct-literal false positive; the `,` exclusion is reversed
  because it bought precision with a leak.

One caveat the verifier raised and I could not discharge: the proof assumes a
consumed walk does not advance the regex scanner's resume position. Nothing in
the code suggests it does, but the pinning matrix should include a row where a
long post-D1 walk passes over a *later* independent match.

---

## 3. Why D2 is dead

**The premise was false.** "Non-code formats have no tuple-bound-to-one-key" is
simply wrong. Comma-joined lists bound to one key are idiomatic: `NO_PROXY=`,
`KAFKA_BROKERS=`, `SPRING_PROFILES_ACTIVE=`, JDBC failover host lists, Cassandra
contact points, rotation key pairs. Measured: an `.env` comma-joined list is
SENSITIVE today with **one** finding — i.e. caught *by the walk*, which D2 blinds.

**It removes coverage exactly where the keyword set is weakest.** Measured, a
flow-map sibling named `bearer`, `credential`, or `accesskey` is SENSITIVE today
with **one** finding — the walk is the only thing catching it. D2 makes all three
CLEAN. Credential-named keys outside the alternation are common.

**Unknown extensions default to non-code.** The code list is only Rust, Python,
JS, TS, Java, Go, C#, Ruby, PHP. Everything else — C, C++, Kotlin, Scala, Swift,
shell, PowerShell, Perl, Lua, Dart, Elixir, Terraform/HCL, Groovy, and every
extensionless script (`bin/*`, Dockerfile, git hooks) — is non-code by default.
D2 would apply its terminator to real code there, reintroducing the exact tuple
leak D1 exists to prevent.

**Correction to the attack report:** the finding that named this used `apikey` as
its non-keyword sibling. Measured, `auth: {password: "${…}", apikey: "…"}` is
SENSITIVE with **two** findings — `api[_-]?key` has an optional separator, so
`apikey` *is* a keyword and produces its own independent match. The example is
wrong; the mechanism is right, and `bearer`/`credential`/`accesskey` demonstrate
it.

**And my own control was worthless.** Matrix row A4 (`{token: "${…}",
password: "…"}`) passes under D2 only because its sibling carries a keyword and
generates an independent match — the walk contributes nothing to that verdict. It
would have stayed green while D2 silently blinded the walk. Replace it with a
non-keyword-sibling row.

---

## 4. Why D3 is dead

D3 does not leak alone. **D2+D3 leak as a pair**, and D3 is the proximate cause:
before D3, a comma-swallowing capture failed the placeholder test and never
reached the walk at all. D3 promotes it to placeholder status and hands the walk
a comma that D2 immediately consumes.

The result is a **one-byte evasion**: adding a single comma turns a SENSITIVE
line CLEAN, in any `.env`/`.yaml`/`.properties` file, reachable by accident as
well as deliberately. Measured at baseline, both the comma and no-comma variants
are SENSITIVE; under D2+D3 only the comma variant goes CLEAN.

The rotation case makes it concrete: `DEPLOY_TOKEN=${CI_FALLBACK_TOKEN}, "<live
token>"`. A placeholder and a live secret coexisting on one line is *exactly*
what a credential rotation window looks like.

D3 also misreads data commas as structural — a comma inside a quoted value.

---

## 5. What to do about the config false positives

`Y1u`, `Y1q` and the non-keyword-sibling row remain false positives. **Accept
them as a stated ceiling.** Five comma heuristics have now been tried and every
one produced a leak. The failure direction is tolerable and the fix direction
keeps being catastrophic; that asymmetry is the answer.

I also mislabelled one of them: I recorded the non-keyword-sibling row (`note:`)
as a false positive "fixed" by D2. It is better understood as **coverage the walk
provides for keyword-set gaps**, so removing it was removing protection, not
noise.

If it is revisited later, the lever is *what follows the comma* — a `name:` /
`name =` shape implies a new binding — and the primitive already exists: **D4's
mate check**. If `bytes[secret.end()]` equals the opener at
`bytes[secret.start()-1]`, the capture ended at its own closing quote and any
trailing comma was inside the value, hence data, not structure. That is a
separate decision needing its own review, not a sixth heuristic bolted on here.

**New ceiling to log:** bracketed arrays of quoted strings never match at all.
`api_key = ["placeholder", "<cred>"]` measures CLEAN because after `=` the next
byte is `[` and the next is `"`, giving a 1-byte capture. Pre-existing, unrelated
to any comma decision, and a total blind spot across TOML/YAML/JSON.

---

## 6. Pinning matrix — revised build spec

Trust gate: the 27-row oracle stayed green through **every** broken variant.

Variants: **V0** baseline `20b51c8` · **V1** = D1 (the proposal) · **V2** = V1+D4.

Every row needs a **match-proving control** — placeholders must exceed the 8-byte
capture floor or the row is vacuous and reads CLEAN for the wrong reason
(`${HOST}` is 7 bytes; this produced three false results this campaign). Assert
`finding_count` where it discriminates, not just SENSITIVE/CLEAN — that is what
exposed the `apikey` error.

| Row | Path | Shape | V0 | V1 expected |
|---|---|---|---|---|
| C5a | `.js` | multi-declarator across lines | CLEAN | **SENSITIVE** |
| C5b | `.js` | same, one line (control) | SENSITIVE | SENSITIVE |
| C5d | `.rs` | multi-binding across lines | CLEAN | **SENSITIVE** |
| G10b | `.rs` | struct literal, sibling field | CLEAN | SENSITIVE — **accepted regression**, pin as accepted |
| D1-scan | any | long post-D1 walk passing over a later independent match | — | count must not drop |
| P1/P4/E2 | `.py` | tuple + exemption-B shapes | SENSITIVE | SENSITIVE |
| S20a/S21a | `.py` | depth-1 arg comma; FO-2 concat | SENSITIVE | SENSITIVE |
| K2/K3/K4 | `.yaml` | non-keyword sibling (`bearer`/`credential`/`accesskey`) | SENSITIVE[1] | SENSITIVE[1] |
| K1 | `.yaml` | `apikey` sibling — **two** findings, not a walk test | SENSITIVE[2] | SENSITIVE[2] |
| F5 | `.env` | comma-joined list, one key | SENSITIVE[1] | SENSITIVE[1] |
| F3/F3ctl | `.env` | one-comma evasion pair | SENSITIVE / SENSITIVE | SENSITIVE / SENSITIVE |
| Y1u/Y1q/A3 | `.yaml` | config sibling FPs | SENSITIVE | SENSITIVE — **accepted ceiling** |
| B1 | `.toml` | bracketed array | CLEAN | CLEAN — **stated blind spot** |

Plus, per variant: the full `oracle_` suite **and** the whole-tree tripwire. The
tripwire is the only check that can size D1's struct-literal cost against real
source.

Acceptance: no row moves toward CLEAN; `finding_count` never drops; the two C5
rows flip to SENSITIVE; oracle green; tripwire green or each failure individually
adjudicated.
