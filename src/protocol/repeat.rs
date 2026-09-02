//! Feature 032 (US1): session-scoped repeat-call tracker.
//!
//! The `call_tool` seam (`src/protocol/mod.rs`) fingerprints every eligible
//! read call with the generic [`RequestHash`], keyed by the observed session
//! lane, and counts consecutive-in-kind serves whose attached
//! [`ProjectEvidence`] is positively observed equal. When the same
//! fingerprint is served a third time on one observed session with the
//! evidence unchanged across the run, a [`RepeatNotice`] is appended to the
//! response — text and `_meta`, contract in
//! `specs/032-repeat-call-breaker/contracts/repeat-notice.md`.
//!
//! Zero false claims dominates every trade-off here (spec SC-002). The notice
//! makes a claim about the FUTURE ("the result cannot differ until the index
//! changes"), so equality of the past serves is necessary but not sufficient —
//! it must also be true that every input to the rendered answer is fenced by
//! the compared evidence. Two independent guards enforce that:
//!
//! 1. A [`RepeatNotice`] is constructible ONLY from a [`RepeatWitness`], which
//!    exists only on observed full equality of two typed evidence values AND
//!    of the rendered result bytes.
//! 2. A renderer that reaches outside the index — `search_text`'s zero-hit
//!    untracked-file sweep (live `git status` + raw worktree bytes), the
//!    admission-tier disk fallback (`fs::metadata` + first bytes) — latches
//!    [`result_status::note_unfenced_input`] on the dispatch scope, and the
//!    seam reads it back as [`UnobservedReason::UnfencedInput`]: the run is
//!    REMOVED, so such a response never notices and never accumulates.
//!
//! Anything else the seam cannot observe — an inert lane, an unavailable-
//! evidence marker, an unbound project, an internal failure, a serve whose
//! observation predates an incarnation clear — removes the run instead of
//! guessing. Interleaved *different* calls never reset a run (spec FR-002);
//! evidence drift is how an index change is observed (research.md R6), and a
//! body that differs under equal evidence restarts the run (a false negative,
//! never a claim).

use std::collections::HashMap;

use rmcp::RoleServer;
use rmcp::model::{CallToolResult, ContentBlock, JsonObject, MetaObject};
use rmcp::service::RequestContext;

use crate::idempotency::RequestHash;
use crate::protocol::result_status::{
    self, OutcomeClass, PROJECT_EVIDENCE_META_KEY, ProjectEvidence, REPEAT_NOTICE_CONTRACT_VERSION,
    REPEAT_NOTICE_META_KEY, RepeatNoticeView,
};

/// Serves of one fingerprint with continuously-equal evidence at which the
/// notice fires. The first retry after a transient failure is legitimate; the
/// second identical retry is the degenerate-loop signal (spec Assumptions).
pub const NOTICE_THRESHOLD: u32 = 3;

/// Hard cap on tracked runs per process; see [`RepeatTracker::record_serve`].
pub const REPEAT_TRACKER_MAX_ENTRIES: usize = 512;

/// The read tools whose rendered output is fully determined by the published
/// index generation the response's evidence names (research.md R4). Widening
/// this list requires a per-tool proof that every input to its output is
/// fenced by the compared evidence: `get_symbol` (wall-clock session cache-hit
/// body) and `search_files` (frecency ranking) were refuted out.
pub const REPEAT_ELIGIBLE_TOOLS: [&str; 5] = [
    "search_symbols",
    "search_text",
    "get_repo_map",
    "find_references",
    "find_dependents",
];

/// The `project_id` placeholder the local adapter emits when it has no
/// repository root (`local_project_evidence_for_generation` in
/// `src/protocol/tools.rs`): full-shaped evidence with no project to be
/// current about, so it is treated as unobserved.
const UNBOUND_PROJECT_ID: &str = "unbound";

pub fn is_repeat_eligible(tool: &str) -> bool {
    REPEAT_ELIGIBLE_TOOLS.contains(&tool)
}

/// How the server serving this request was WIRED — declared once at
/// construction by the entry point, never inferred from a request.
///
/// `Stdio` is set ONLY by [`crate::protocol::SymForgeServer::with_stdio_transport_lane`],
/// which only the two stdio entry points in `src/cli/entry.rs` call: one
/// process serves exactly one client there, so the server instance IS the
/// session. Everything else — the shared HTTP `/mcp` server, the daemon
/// worker, an in-process `serve_directly`, a future SSE or socket adapter —
/// keeps the default `Shared` and never accumulates. Absence of a declaration
/// is not evidence of single-client-ness (F3 / research.md R12 finding 3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransportLane {
    Stdio,
    #[default]
    Shared,
}

/// The session lane the seam OBSERVED for one request (research.md R9).
///
/// * `Stdio`: the entry point declared a single-client transport AND rmcp's
///   HTTP marker is absent, so the server instance is the session.
/// * `HttpInert`: everything else — the shared, stateless `/mcp` lane (rmcp
///   never creates a session there: `mcp_http.rs` pins
///   `legacy_session_mode == false`) and any transport that did not declare
///   itself single-client. No per-session identity is observable, so the
///   tracker never interacts: an unattributable count would be an unobserved
///   claim (spec FR-002).
///
/// (Round 2 note: `HttpInert` now also covers undeclared non-HTTP transports.
/// The variant name is the one data-model.md pins; the wider meaning is
/// documented here rather than renamed under a spec this change may not edit.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionDiscriminator {
    Stdio,
    HttpInert,
}

impl SessionDiscriminator {
    /// Observation: the lane the entry point DECLARED, plus whether rmcp's
    /// streamable-HTTP transport inserted the inbound `http::request::Parts`
    /// into the request extensions. The result is a [`LaneWitness`]: proof the
    /// lane was read off a real request context and a real declaration.
    pub fn observe(declared: TransportLane, context: &RequestContext<RoleServer>) -> LaneWitness {
        Self::from_observations(
            declared,
            context
                .extensions
                .get::<axum::http::request::Parts>()
                .is_some(),
        )
    }

    /// The rule, separated from the un-constructible `RequestContext` so it is
    /// directly testable (research.md R12 finding 13: no test can build one).
    ///
    /// `Stdio` requires BOTH a positive declaration and the absence of the
    /// HTTP marker. Either observation failing yields the inert lane — a
    /// missing declaration is not a `Stdio`, and a declared-stdio server that
    /// somehow sees an HTTP request is a contradiction, not a session.
    fn from_observations(declared: TransportLane, http_parts_present: bool) -> LaneWitness {
        LaneWitness(match (declared, http_parts_present) {
            (TransportLane::Stdio, false) => Self::Stdio,
            _ => Self::HttpInert,
        })
    }
}

/// Proof that the session lane was OBSERVED from a request context and the
/// server's own declaration rather than asserted by a caller. The private
/// field means only [`SessionDiscriminator::observe`] (and the in-crate test
/// door) can mint one, so no seam can hand the tracker a `Stdio` it never
/// observed.
#[derive(Debug, Clone, Copy)]
pub struct LaneWitness(SessionDiscriminator);

impl LaneWitness {
    #[cfg(test)]
    pub(crate) fn assume(lane: SessionDiscriminator) -> Self {
        Self(lane)
    }

    #[cfg(test)]
    fn lane(&self) -> SessionDiscriminator {
        self.0
    }
}

/// The tracker's incarnation counter, minted ONLY by [`RepeatTracker::epoch`]
/// (and the in-crate test door) so no caller can assert an epoch it did not
/// read off the tracker.
///
/// The seam reads it BEFORE dispatch and carries it on the [`RepeatKey`];
/// [`RepeatTracker::record_serve`] refuses any serve whose epoch is not the
/// current one. That is what stops an in-flight serve — observed against
/// incarnation X, recorded after a concurrent request's transition
/// [`RepeatTracker::clear`] — from anchoring a run that spans X and its
/// replacement (`ProjectEvidence.generation` is a per-process counter, so a
/// replacement index can return evidence byte-equal to the dead one's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackerEpoch(u64);

impl TrackerEpoch {
    #[cfg(test)]
    pub(crate) const fn assume(epoch: u64) -> Self {
        Self(epoch)
    }
}

/// Identity of one repeatable request: the observed lane, the tracker
/// incarnation the seam read before dispatch, the tool as dispatched, and the
/// canonical-JSON fingerprint of its arguments. Two calls are "identical" iff
/// `tool` and `request_hash` match within one incarnation; only `Stdio` keys
/// are ever constructed, so `session` is a witness that the lane was observed.
/// `epoch` is part of the derived identity as well as an explicit guard, so a
/// pre-clear key cannot even hash-match a post-clear run.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepeatKey {
    session: SessionDiscriminator,
    epoch: TrackerEpoch,
    tool: String,
    request_hash: RequestHash,
}

impl RepeatKey {
    /// `Some` only for a call the tracker may attribute: an observed `Stdio`
    /// session, an eligible tool, and no set-valued `projects` fan-out (that
    /// lane structurally withholds per-project evidence — research.md R6).
    /// `None` means no tracker interaction at all: the seam cannot key what
    /// it cannot attribute. Absent arguments fingerprint as the empty object.
    ///
    /// `epoch` must be read from the tracker BEFORE the dispatch whose result
    /// this key will record, so an incarnation change during the dispatch is
    /// observable at record time.
    pub fn observe(
        lane: LaneWitness,
        epoch: TrackerEpoch,
        tool: &str,
        arguments: Option<&JsonObject>,
    ) -> Option<Self> {
        let session = lane.0;
        if session != SessionDiscriminator::Stdio || !is_repeat_eligible(tool) {
            return None;
        }
        if arguments
            .and_then(|arguments| arguments.get("projects"))
            .is_some_and(|projects| !projects.is_null())
        {
            return None;
        }
        let args = serde_json::Value::Object(arguments.cloned().unwrap_or_default());
        let request_hash = RequestHash::for_tool_request(tool, &args).ok()?;
        Some(Self {
            session,
            epoch,
            tool: tool.to_string(),
            request_hash,
        })
    }

    pub fn tool(&self) -> &str {
        &self.tool
    }

    pub fn request_hash(&self) -> &RequestHash {
        &self.request_hash
    }
}

/// SHA-256 over the response's rendered text content, taken at the seam
/// BEFORE any notice is appended. Each `ContentBlock::Text` contributes its
/// byte length (u64, little-endian) followed by its bytes, so block framing
/// is part of the identity (`["ab","c"]` and `["a","bc"]` differ). Non-text
/// blocks contribute nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyDigest([u8; 32]);

impl BodyDigest {
    fn of_content(content: &[ContentBlock]) -> Self {
        let mut frame = Vec::new();
        for block in content {
            if let ContentBlock::Text(text) = block {
                frame.extend_from_slice(&(text.text.len() as u64).to_le_bytes());
                frame.extend_from_slice(text.text.as_bytes());
            }
        }
        Self(crate::hash::digest(&frame))
    }
}

/// Everything the seam positively observed about one serve: the typed
/// evidence the response carried and the digest of the body it rendered.
/// Built only by [`ServeObservation::from_result`] (and the in-crate test
/// door), so no caller can assert an observation it did not make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedServe {
    evidence: ProjectEvidence,
    body_digest: BodyDigest,
}

impl ObservedServe {
    #[cfg(test)]
    pub(crate) fn for_test(evidence: ProjectEvidence, content: &[ContentBlock]) -> Self {
        Self {
            evidence,
            body_digest: BodyDigest::of_content(content),
        }
    }
}

/// Proof that two observed serves were equal — typed evidence AND rendered
/// body (Constitution V).
///
/// The only constructor is [`RepeatWitness::observe`]; the private field makes
/// a forged witness unspellable outside this module (the same repair shape as
/// `SourceAuthority::from_freshness` in `search_format.rs`).
#[derive(Debug)]
pub struct RepeatWitness {
    generation: u64,
}

impl RepeatWitness {
    /// `Some` only on FULL evidence equality — any drift in generation, index
    /// state, file/symbol counts, root, load source, or identity — AND body
    /// digest equality. A body that differs under equal evidence is exactly
    /// the result that just differed; the claim cannot survive it. Private:
    /// only [`RepeatTracker::record_serve`] may witness.
    fn observe(prior: &ObservedServe, current: &ObservedServe) -> Option<Self> {
        // F4: compare the WHOLE value, not a list of named fields. A named
        // list silently excludes any field added to `ObservedServe` later —
        // the observation would quietly stop covering part of what it claims
        // to have observed. This relies on `ObservedServe`'s derived
        // `PartialEq`, which is regenerated with the struct, so a new field
        // joins the witness by construction. (An exhaustive destructure would
        // force a decision at the cost of failing to compile, but is strictly
        // weaker here: it must then re-list the fields to compare them.)
        (prior == current).then_some(Self {
            generation: current.evidence.generation,
        })
    }

    pub fn evidence_generation(&self) -> u64 {
        self.generation
    }
}

/// The notice: constructible only from a [`RepeatWitness`] and a count at or
/// above [`NOTICE_THRESHOLD`], so a notice without an observed equality is
/// unspellable.
#[derive(Debug)]
pub struct RepeatNotice {
    witness: RepeatWitness,
    repeat_count: u32,
    tool: String,
    request_hash: RequestHash,
}

impl RepeatNotice {
    /// Private: only [`RepeatTracker::record_serve`] may spell a notice.
    fn new(witness: RepeatWitness, repeat_count: u32, key: &RepeatKey) -> Option<Self> {
        (repeat_count >= NOTICE_THRESHOLD).then(|| Self {
            witness,
            repeat_count,
            tool: key.tool().to_string(),
            request_hash: key.request_hash().clone(),
        })
    }

    pub fn repeat_count(&self) -> u32 {
        self.repeat_count
    }

    pub fn evidence_generation(&self) -> u64 {
        self.witness.evidence_generation()
    }

    /// Byte-canonical text (contract §2). It says "published" — the
    /// observation is publication-level, never disk-level — and never claims
    /// the files are unchanged.
    pub fn text(&self) -> String {
        format!(
            "Repeat notice: identical request served {}x with no index change published in between (project evidence unchanged). The result cannot differ until the index changes - change the request instead of retrying.",
            self.repeat_count
        )
    }

    /// Wire view under [`REPEAT_NOTICE_META_KEY`] (contract §1).
    pub fn view(&self) -> RepeatNoticeView {
        RepeatNoticeView {
            contract_version: REPEAT_NOTICE_CONTRACT_VERSION,
            repeat_count: self.repeat_count(),
            tool: self.tool.clone(),
            request_hash: self.request_hash.as_str().to_string(),
            evidence_generation: self.evidence_generation(),
        }
    }

    /// Deliver both carriers: the text is appended (with a `\n\n` separator)
    /// to the FINAL text content block so the original content is a strict
    /// byte prefix (spec FR-004), and the view is inserted under
    /// [`REPEAT_NOTICE_META_KEY`]. When the content has no text block at all,
    /// one is pushed so the two carriers are never split. `isError` and every
    /// prior byte are untouched.
    pub fn attach(&self, result: &mut CallToolResult) {
        let text = self.text();
        match result
            .content
            .iter()
            .rposition(|block| matches!(block, ContentBlock::Text(_)))
        {
            Some(index) => {
                if let ContentBlock::Text(block) = &mut result.content[index] {
                    block.text.push_str("\n\n");
                    block.text.push_str(&text);
                }
            }
            None => result.content.push(ContentBlock::text(text)),
        }
        let meta = result
            .meta
            .get_or_insert_with(|| MetaObject(JsonObject::new()));
        meta.0.insert(
            REPEAT_NOTICE_META_KEY.to_string(),
            serde_json::to_value(self.view()).expect("RepeatNoticeView must serialize to JSON"),
        );
    }
}

/// Why a serve could not be attributed to a run. Each one removes the run:
/// cannot observe ⇒ cannot accumulate ⇒ cannot claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnobservedReason {
    /// The `symforge/project_evidence` value is missing or does not
    /// deserialize as a full `ProjectEvidence` (the `{"bound": false}`
    /// unavailable marker lands here — "absence" does not exist on the wire).
    EvidenceUnavailable,
    /// Full-shaped evidence carrying the `"unbound"` placeholder project.
    ProjectUnbound,
    /// `ResultStatus` observed as `InternalFailure` (research.md R5): an
    /// internal failure is not evidence the next attempt fails.
    InternalFailure,
    /// `OutcomeClass` unobservable on this lane (no `symforge/result_status`)
    /// and `isError == true`: cleared conservatively.
    ErrorWithoutOutcomeClass,
    /// A renderer in this dispatch consulted an input the published index does
    /// not fence (`result_status::note_unfenced_input`). Evidence equality
    /// witnesses that the PAST serves matched; the notice's sentence is about
    /// the FUTURE, and nothing here fences that input, so the claim is
    /// withheld and the run removed.
    UnfencedInput,
}

/// What the seam observed on the OUTGOING response of one eligible call.
/// Constructible only by [`ServeObservation::from_result`] (and the in-crate
/// test door): the inner enum is private, so a caller cannot assert an
/// observation it did not make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeObservation(Observation);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Observation {
    Observed(ObservedServe),
    Unobserved(UnobservedReason),
}

impl ServeObservation {
    #[cfg(test)]
    pub(crate) fn observed_for_test(serve: ObservedServe) -> Self {
        Self(Observation::Observed(serve))
    }

    #[cfg(test)]
    fn observed_serve(&self) -> Option<&ObservedServe> {
        match &self.0 {
            Observation::Observed(serve) => Some(serve),
            Observation::Unobserved(_) => None,
        }
    }

    #[cfg(test)]
    fn unobserved_reason(&self) -> Option<UnobservedReason> {
        match &self.0 {
            Observation::Observed(_) => None,
            Observation::Unobserved(reason) => Some(*reason),
        }
    }

    /// Observation: the dispatch's own unfenced-input latch, then the
    /// response's `_meta` — the evidence the seam attached (or the statused
    /// writer attached first), the outcome class read leniently from
    /// `symforge/result_status` — and the digest of the text content the
    /// response renders (before any notice is appended).
    ///
    /// The latch is read FIRST because it is the claim-killer: no amount of
    /// observed equality can license "the result cannot differ" for a body
    /// built from something the index never published. Every outcome other
    /// than that, an observed `InternalFailure`, or an unclassifiable error
    /// ADVANCES a run: `NotFound`, `EmptyResult`, `InvalidRequest` loops are
    /// the motivating case (data-model.md state machine). Anything this cannot
    /// read emits `Unobserved`, never a default.
    ///
    /// Called at the `call_tool` seam INSIDE the dispatch's evidence scope, so
    /// the latch it reads is the one this response's renderers set and no
    /// other; the scope is constructed per dispatch and dropped with it.
    pub fn from_result(result: &CallToolResult) -> Self {
        if result_status::unfenced_input_consulted() {
            return Self(Observation::Unobserved(UnobservedReason::UnfencedInput));
        }
        let meta = result.meta.as_ref();
        let Some(evidence) = meta
            .and_then(|meta| meta.0.get(PROJECT_EVIDENCE_META_KEY))
            .and_then(|value| serde_json::from_value::<ProjectEvidence>(value.clone()).ok())
        else {
            return Self(Observation::Unobserved(
                UnobservedReason::EvidenceUnavailable,
            ));
        };
        if evidence.project_id == UNBOUND_PROJECT_ID {
            return Self(Observation::Unobserved(UnobservedReason::ProjectUnbound));
        }
        let observed = || ObservedServe {
            evidence: evidence.clone(),
            body_digest: BodyDigest::of_content(&result.content),
        };
        Self(match result_status::observed_outcome_class(meta) {
            Some(OutcomeClass::InternalFailure) => {
                Observation::Unobserved(UnobservedReason::InternalFailure)
            }
            Some(_) => Observation::Observed(observed()),
            None if result.is_error == Some(true) => {
                Observation::Unobserved(UnobservedReason::ErrorWithoutOutcomeClass)
            }
            None => Observation::Observed(observed()),
        })
    }
}

/// One run: serves of a key with continuously-equal observation. The stored
/// value is the TYPED observation from the run's first serve — never raw
/// `_meta` JSON, so two unavailable markers can never compare equal and
/// accumulate.
#[derive(Debug)]
struct RepeatRun {
    count: u32,
    observed: ObservedServe,
}

/// Bounded per-process map of runs (data-model.md state machine). Shared by
/// every clone of `SymForgeServer` behind one `Arc<Mutex<_>>`; the seam locks
/// it synchronously (before dispatch to read [`Self::epoch`], after dispatch
/// to record) and never holds it across an `.await`.
#[derive(Debug, Default)]
pub struct RepeatTracker {
    runs: HashMap<RepeatKey, RepeatRun>,
    /// Incarnation counter, bumped by [`Self::clear`]. Kept INSIDE the mutex
    /// rather than in a sibling `AtomicU64` so the bump and the run drop are
    /// one atomic step for every reader: with two cells a pre-dispatch reader
    /// could observe the new epoch while the map still held the old runs (or
    /// the reverse), which is exactly the interleaving this field exists to
    /// rule out. The cost is one extra uncontended lock per eligible call.
    epoch: u64,
}

impl RepeatTracker {
    /// The current incarnation. The seam reads this BEFORE dispatch and mints
    /// the [`RepeatKey`] with it; [`Self::record_serve`] refuses anything else.
    pub fn epoch(&self) -> TrackerEpoch {
        TrackerEpoch(self.epoch)
    }

    /// Apply one observed serve and return the notice when the run reaches
    /// [`NOTICE_THRESHOLD`]. A key from a superseded incarnation is refused
    /// outright. Unobserved ⇒ the run is removed. Observed and witnessed equal
    /// (evidence AND body) ⇒ `count += 1` (saturating). Observed and unequal
    /// in either ⇒ the run restarts at 1 on the new observation.
    pub fn record_serve(
        &mut self,
        key: RepeatKey,
        observation: ServeObservation,
    ) -> Option<RepeatNotice> {
        if key.epoch != self.epoch() {
            // F2: this serve was observed against an index incarnation the
            // tracker has since dropped, and a replacement's evidence can be
            // byte-equal to the dead one's. Refuse it entirely — not even as a
            // fresh run, since the observation it carries describes the old
            // incarnation's answer.
            return None;
        }
        let current = match observation.0 {
            Observation::Observed(current) => current,
            Observation::Unobserved(_) => {
                self.runs.remove(&key);
                return None;
            }
        };
        if let Some(run) = self.runs.get_mut(&key) {
            return match RepeatWitness::observe(&run.observed, &current) {
                Some(witness) => {
                    run.count = run.count.saturating_add(1);
                    RepeatNotice::new(witness, run.count, &key)
                }
                None => {
                    run.observed = current;
                    run.count = 1;
                    None
                }
            };
        }
        if self.runs.len() >= REPEAT_TRACKER_MAX_ENTRIES {
            // ponytail: clear-on-cap at REPEAT_TRACKER_MAX_ENTRIES (512) drops
            // every in-flight run at once — losing only true notices, never
            // creating a false one; upgrade path is LRU eviction keyed on
            // last-serve order if a real session ever fills this map.
            //
            // Deliberately drops the runs WITHOUT bumping the epoch: this is
            // memory pressure, not an incarnation change. The index that
            // served those keys is still the one serving now, so their next
            // serve honestly restarts at 1 instead of being refused forever.
            self.runs.clear();
        }
        self.runs.insert(
            key,
            RepeatRun {
                count: 1,
                observed: current,
            },
        );
        None
    }

    /// Drop every run and advance the incarnation. The rule: a run must not
    /// survive a change of the index INCARNATION it was observed against — a
    /// daemon reconnect (a replacement daemon's evidence can be byte-equal to
    /// the dead one's: `generation` is a per-process counter), the
    /// degrade-to-local and restore-from-local transitions, and a local index
    /// reload. The adapter calls this at each of those sites; a cleared run
    /// can only cost a true notice, never create a false one.
    ///
    /// The epoch bump is what makes this safe under concurrency: dropping the
    /// map alone would still let a serve already in flight against the old
    /// incarnation be inserted afterwards and anchor a run that spans the
    /// transition. `wrapping_add` because 2^64 incarnation changes in one
    /// process is not a reachable state.
    pub(crate) fn clear(&mut self) {
        self.runs.clear();
        self.epoch = self.epoch.wrapping_add(1);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    #[cfg(test)]
    fn count_of(&self, key: &RepeatKey) -> Option<u32> {
        self.runs.get(key).map(|run| run.count)
    }

    #[cfg(test)]
    fn set_count_for_test(&mut self, key: &RepeatKey, count: u32) {
        if let Some(run) = self.runs.get_mut(key) {
            run.count = count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::result_status::{
        OutcomeClass, PROJECT_EVIDENCE_META_KEY, ProjectEvidence, REPEAT_NOTICE_META_KEY,
        RESULT_STATUS_META_KEY, ResultStatus,
    };
    use rmcp::model::{CallToolResult, ContentBlock, JsonObject, MetaObject};
    use serde_json::json;

    fn evidence(generation: u64) -> ProjectEvidence {
        ProjectEvidence {
            project_id: "project-v1-repeat".to_string(),
            project_name: "repeat".to_string(),
            canonical_root: Some("C:/work/repeat".to_string()),
            generation,
            index_state: "Ready".to_string(),
            load_source: "memory".to_string(),
            index_files: 3,
            index_symbols: 4,
        }
    }

    fn args(value: serde_json::Value) -> JsonObject {
        serde_json::from_value(value).expect("object arguments")
    }

    fn stdio() -> LaneWitness {
        LaneWitness::assume(SessionDiscriminator::Stdio)
    }

    /// The epoch of a freshly constructed [`RepeatTracker`]. Tests that never
    /// call [`RepeatTracker::clear`] key against this; the ones that do mint
    /// their keys from `tracker.epoch()` instead.
    fn epoch0() -> TrackerEpoch {
        TrackerEpoch::assume(0)
    }

    fn key(tool: &str, query: &str) -> RepeatKey {
        RepeatKey::observe(
            stdio(),
            epoch0(),
            tool,
            Some(&args(json!({ "query": query }))),
        )
        .expect("an eligible stdio call keys the tracker")
    }

    /// A result carrying `_meta` exactly as the seam would see it after
    /// `attach_project_evidence_meta` and the statused writer ran.
    fn result_with(
        evidence_value: serde_json::Value,
        status: Option<serde_json::Value>,
        is_error: Option<bool>,
    ) -> CallToolResult {
        let mut meta = JsonObject::new();
        meta.insert(PROJECT_EVIDENCE_META_KEY.to_string(), evidence_value);
        if let Some(status) = status {
            meta.insert(RESULT_STATUS_META_KEY.to_string(), status);
        }
        let mut result = CallToolResult::success(vec![ContentBlock::text("body")]);
        result.is_error = is_error;
        result.with_meta(Some(MetaObject(meta)))
    }

    fn status_value(outcome: OutcomeClass) -> serde_json::Value {
        serde_json::to_value(ResultStatus::new(outcome)).expect("status serializes")
    }

    /// The digest every synthetic serve in these tests renders ("body").
    fn body() -> BodyDigest {
        BodyDigest::of_content(&[ContentBlock::text("body")])
    }

    fn observed_serve(generation: u64) -> ObservedServe {
        ObservedServe {
            evidence: evidence(generation),
            body_digest: body(),
        }
    }

    fn observed(generation: u64) -> ServeObservation {
        ServeObservation::observed_for_test(observed_serve(generation))
    }

    /// Serve `key` `n` times with equal observed evidence; return the LAST
    /// serve's notice.
    fn serve_equal(
        tracker: &mut RepeatTracker,
        key: &RepeatKey,
        generation: u64,
        n: u32,
    ) -> Option<RepeatNotice> {
        let mut last = None;
        for _ in 0..n {
            last = tracker.record_serve(key.clone(), observed(generation));
        }
        last
    }

    // Oracle 5 — the eligible list is a reviewed decision, not a drift.
    #[test]
    fn eligible_list_is_pinned() {
        assert_eq!(
            REPEAT_ELIGIBLE_TOOLS,
            [
                "search_symbols",
                "search_text",
                "get_repo_map",
                "find_references",
                "find_dependents",
            ]
        );
        for tool in REPEAT_ELIGIBLE_TOOLS {
            assert!(is_repeat_eligible(tool), "{tool} must be eligible");
            assert!(
                RepeatKey::observe(stdio(), epoch0(), tool, None).is_some(),
                "{tool} must key the tracker on stdio"
            );
        }
        // Refuted out by R12 (time-varying repeat output) — a widening is a
        // per-tool proof, never a default.
        for tool in [
            "get_symbol",
            "search_files",
            "get_file_content",
            "status",
            "what_changed",
            "symforge",
            "symforge_edit",
            "index_folder",
        ] {
            assert!(!is_repeat_eligible(tool), "{tool} must stay ineligible");
            assert!(
                RepeatKey::observe(stdio(), epoch0(), tool, None).is_none(),
                "{tool} must never key the tracker"
            );
        }
        assert_eq!(NOTICE_THRESHOLD, 3);
        assert_eq!(REPEAT_TRACKER_MAX_ENTRIES, 512);
    }

    // Oracle 10 — the witness exists only on full equality (evidence AND body).
    //
    // F4 (round 2): the cases below enumerate today's fields, but the witness
    // itself compares the WHOLE `ObservedServe` and relies on that struct's
    // DERIVED `PartialEq` — so a field added later joins the comparison
    // without anyone remembering to name it here. Removing that derive, or
    // hand-writing a `PartialEq` that skips a field, silently narrows what the
    // notice claims to have observed; keep the derive. (Verified by
    // simulation: with the old named-field comparison, an added third field
    // differing alone still produced a witness.)
    #[test]
    fn witness_requires_full_equality() {
        let prior = observed_serve(7);
        let with = |evidence: ProjectEvidence| ObservedServe {
            evidence,
            body_digest: body(),
        };
        assert!(
            RepeatWitness::observe(&prior, &observed_serve(7)).is_some(),
            "equal evidence and body must be witnessable"
        );
        assert!(RepeatWitness::observe(&prior, &observed_serve(8)).is_none());
        let mut files = evidence(7);
        files.index_files += 1;
        assert!(RepeatWitness::observe(&prior, &with(files)).is_none());
        let mut state = evidence(7);
        state.index_state = "Loading".to_string();
        assert!(RepeatWitness::observe(&prior, &with(state)).is_none());
        let mut root = evidence(7);
        root.canonical_root = None;
        assert!(RepeatWitness::observe(&prior, &with(root)).is_none());
        let mut source = evidence(7);
        source.load_source = "snapshot".to_string();
        assert!(RepeatWitness::observe(&prior, &with(source)).is_none());
        let mut symbols = evidence(7);
        symbols.index_symbols += 1;
        assert!(RepeatWitness::observe(&prior, &with(symbols)).is_none());
        let mut id = evidence(7);
        id.project_id.push('x');
        assert!(RepeatWitness::observe(&prior, &with(id)).is_none());
        let mut name = evidence(7);
        name.project_name.push('x');
        assert!(RepeatWitness::observe(&prior, &with(name)).is_none());
        // F1: equal evidence, different rendered body => no witness.
        let other_body = ObservedServe {
            evidence: evidence(7),
            body_digest: BodyDigest::of_content(&[ContentBlock::text("body\n\ndiagnostic")]),
        };
        assert!(RepeatWitness::observe(&prior, &other_body).is_none());
        assert_eq!(
            RepeatWitness::observe(&prior, &observed_serve(7))
                .expect("witness")
                .evidence_generation(),
            7
        );
    }

    // Oracle 10 — threshold boundary (2 vs 3) and saturating count.
    #[test]
    fn notice_threshold_is_three_and_count_saturates() {
        let witness =
            || RepeatWitness::observe(&observed_serve(1), &observed_serve(1)).expect("witness");
        let k = key("search_symbols", "anchor");
        assert!(
            RepeatNotice::new(witness(), 2, &k).is_none(),
            "count 2 is below the threshold"
        );
        let notice = RepeatNotice::new(witness(), 3, &k).expect("count 3 notices");
        assert_eq!(notice.repeat_count(), 3);
        assert_eq!(notice.evidence_generation(), 1);
        assert_eq!(
            notice.text(),
            "Repeat notice: identical request served 3x with no index change published in between (project evidence unchanged). The result cannot differ until the index changes - change the request instead of retrying."
        );

        let mut tracker = RepeatTracker::default();
        assert!(
            serve_equal(&mut tracker, &k, 1, 2).is_none(),
            "serve 2: no notice"
        );
        assert_eq!(tracker.count_of(&k), Some(2));
        let third = serve_equal(&mut tracker, &k, 1, 1).expect("serve 3 notices");
        assert_eq!(third.repeat_count(), 3);
        let fourth = serve_equal(&mut tracker, &k, 1, 1).expect("serve 4 notices");
        assert_eq!(fourth.repeat_count(), 4);

        tracker.set_count_for_test(&k, u32::MAX);
        let saturated = serve_equal(&mut tracker, &k, 1, 1).expect("saturated serve notices");
        assert_eq!(
            saturated.repeat_count(),
            u32::MAX,
            "count saturates, never wraps"
        );
        assert_eq!(tracker.count_of(&k), Some(u32::MAX));
    }

    // Oracle 3 — unobserved evidence clears the run and never accumulates.
    #[test]
    fn unobserved_evidence_clears_run() {
        let full = serde_json::to_value(evidence(5)).expect("evidence serializes");
        let marker = json!({ "bound": false, "reason": "project_evidence_unavailable" });
        let garbage = json!("not evidence");
        let mut unbound_evidence = evidence(5);
        unbound_evidence.project_id = "unbound".to_string();
        let unbound = serde_json::to_value(&unbound_evidence).expect("serializes");

        // The typed observation rule.
        assert_eq!(
            ServeObservation::from_result(&result_with(full.clone(), None, None)).observed_serve(),
            Some(&observed_serve(5))
        );
        assert_eq!(
            ServeObservation::from_result(&result_with(marker.clone(), None, None))
                .unobserved_reason(),
            Some(UnobservedReason::EvidenceUnavailable)
        );
        assert_eq!(
            ServeObservation::from_result(&result_with(garbage.clone(), None, None))
                .unobserved_reason(),
            Some(UnobservedReason::EvidenceUnavailable)
        );
        assert_eq!(
            ServeObservation::from_result(&result_with(unbound.clone(), None, None))
                .unobserved_reason(),
            Some(UnobservedReason::ProjectUnbound)
        );
        // A missing `_meta` entirely is unobserved too.
        let bare = CallToolResult::success(vec![ContentBlock::text("body")]);
        assert_eq!(
            ServeObservation::from_result(&bare).unobserved_reason(),
            Some(UnobservedReason::EvidenceUnavailable)
        );

        for (label, unobserved) in [
            ("bound:false marker", marker),
            ("non-deserializable value", garbage),
            ("unbound project_id", unbound),
        ] {
            let mut tracker = RepeatTracker::default();
            let k = key("search_symbols", "anchor");
            assert!(serve_equal(&mut tracker, &k, 5, 2).is_none());
            assert_eq!(tracker.count_of(&k), Some(2), "{label}: run reached 2");
            let observation = ServeObservation::from_result(&result_with(unobserved, None, None));
            assert!(
                tracker.record_serve(k.clone(), observation).is_none(),
                "{label}: an unobserved serve never notices"
            );
            assert_eq!(tracker.count_of(&k), None, "{label}: the run is cleared");
            // Never accumulates: two more unobserved serves still leave nothing.
            for _ in 0..2 {
                let observation = ServeObservation::from_result(&result_with(
                    json!({ "bound": false, "reason": "project_evidence_unavailable" }),
                    None,
                    None,
                ));
                assert!(tracker.record_serve(k.clone(), observation).is_none());
            }
            assert_eq!(tracker.count_of(&k), None, "{label}: still nothing");
            // Honest restart: the next observed serves count from 1.
            assert!(
                serve_equal(&mut tracker, &k, 5, 2).is_none(),
                "{label}: 1, 2"
            );
            let third = serve_equal(&mut tracker, &k, 5, 1).expect("{label}: 3 notices");
            assert_eq!(third.repeat_count(), 3);
        }

        // Evidence-present control: the same three serves DO notice.
        let mut tracker = RepeatTracker::default();
        let k = key("search_symbols", "anchor");
        let mut last = None;
        for _ in 0..3 {
            last = tracker.record_serve(
                k.clone(),
                ServeObservation::from_result(&result_with(full.clone(), None, None)),
            );
        }
        assert_eq!(last.expect("control notices").repeat_count(), 3);
    }

    // Oracle 6 — the data-model state machine's outcome rule.
    #[test]
    fn internal_failure_clears_run() {
        let full = serde_json::to_value(evidence(9)).expect("evidence serializes");
        let mut tracker = RepeatTracker::default();
        let k = key("find_references", "anchor");
        assert!(serve_equal(&mut tracker, &k, 9, 2).is_none());

        // Observed InternalFailure clears.
        let failure = result_with(
            full.clone(),
            Some(status_value(OutcomeClass::InternalFailure)),
            Some(true),
        );
        assert_eq!(
            ServeObservation::from_result(&failure).unobserved_reason(),
            Some(UnobservedReason::InternalFailure)
        );
        assert!(
            tracker
                .record_serve(k.clone(), ServeObservation::from_result(&failure))
                .is_none()
        );
        assert_eq!(tracker.count_of(&k), None, "InternalFailure clears the run");

        // Positive control: InvalidRequest (isError:true, but an observed
        // non-internal class) ADVANCES — the A019 loop is agents re-issuing
        // failing identical calls.
        assert!(serve_equal(&mut tracker, &k, 9, 2).is_none());
        let invalid = result_with(
            full.clone(),
            Some(status_value(OutcomeClass::InvalidRequest)),
            Some(true),
        );
        let notice = tracker
            .record_serve(k.clone(), ServeObservation::from_result(&invalid))
            .expect("InvalidRequest advances to the notice");
        assert_eq!(notice.repeat_count(), 3);
        // NotFound / EmptyResult advance as well.
        for outcome in [OutcomeClass::NotFound, OutcomeClass::EmptyResult] {
            let serve = result_with(full.clone(), Some(status_value(outcome)), None);
            let notice = tracker
                .record_serve(k.clone(), ServeObservation::from_result(&serve))
                .expect("non-internal outcomes advance");
            assert!(notice.repeat_count() >= 4);
        }

        // OutcomeClass unobservable (no result_status key): isError:true
        // clears conservatively; isError absent/false advances.
        let mut tracker = RepeatTracker::default();
        assert!(serve_equal(&mut tracker, &k, 9, 2).is_none());
        let plain_error = result_with(full.clone(), None, Some(true));
        assert_eq!(
            ServeObservation::from_result(&plain_error).unobserved_reason(),
            Some(UnobservedReason::ErrorWithoutOutcomeClass)
        );
        assert!(
            tracker
                .record_serve(k.clone(), ServeObservation::from_result(&plain_error))
                .is_none()
        );
        assert_eq!(tracker.count_of(&k), None);
        assert!(serve_equal(&mut tracker, &k, 9, 2).is_none());
        let plain_ok = result_with(full, None, Some(false));
        let notice = tracker
            .record_serve(k.clone(), ServeObservation::from_result(&plain_ok))
            .expect("a plain non-error serve advances");
        assert_eq!(notice.repeat_count(), 3);
    }

    // Oracle 7 — the capacity clear loses only true notices.
    #[test]
    fn tracker_cap_clears_without_false_claim() {
        let mut tracker = RepeatTracker::default();
        let k0 = key("search_text", "k0");
        assert!(serve_equal(&mut tracker, &k0, 1, 2).is_none());
        assert_eq!(tracker.count_of(&k0), Some(2));
        for i in 1..REPEAT_TRACKER_MAX_ENTRIES {
            let k = key("search_text", &format!("k{i}"));
            assert!(tracker.record_serve(k, observed(1)).is_none());
        }
        assert_eq!(
            tracker.len(),
            REPEAT_TRACKER_MAX_ENTRIES,
            "map is exactly at cap"
        );
        assert!(!tracker.is_empty());
        assert_eq!(tracker.count_of(&k0), Some(2), "k0 survives up to the cap");

        // The next NEW key overflows: everything is cleared, then it inserts.
        let overflow = key("search_text", "overflow");
        assert!(
            tracker
                .record_serve(overflow.clone(), observed(1))
                .is_none()
        );
        assert_eq!(tracker.len(), 1, "cap clear leaves only the new key");
        assert_eq!(tracker.count_of(&overflow), Some(1));
        assert_eq!(tracker.count_of(&k0), None, "k0's run is gone");

        // No false claim: k0's next serve is count 1 — no notice — and the run
        // re-accumulates honestly to a notice at 3.
        assert!(serve_equal(&mut tracker, &k0, 1, 1).is_none());
        assert_eq!(tracker.count_of(&k0), Some(1));
        assert!(serve_equal(&mut tracker, &k0, 1, 1).is_none());
        let notice = serve_equal(&mut tracker, &k0, 1, 1).expect("re-accumulated to 3");
        assert_eq!(notice.repeat_count(), 3);
    }

    // F2 — an incarnation change (daemon reconnect, degrade-to-local,
    // restore-from-local, local reload) clears every run: a call keyed AFTER
    // the clear restarts at 1 and re-accumulates honestly. (Round 2: the key
    // is now re-minted after the clear because a pre-clear key is refused
    // outright — see `serve_observed_before_a_clear_never_joins_a_run`, which
    // owns that half. Re-serving the SAME key across a clear is exactly the
    // in-flight bridge F2 forbids, so it can no longer be this test's model of
    // "the next serve".)
    #[test]
    fn clear_restarts_runs_at_one() {
        let mut tracker = RepeatTracker::default();
        let k = key("search_symbols", "anchor");
        assert!(serve_equal(&mut tracker, &k, 1, 2).is_none());
        assert_eq!(tracker.count_of(&k), Some(2));
        tracker.clear();
        assert!(tracker.is_empty(), "clear() drops every run");
        let k = RepeatKey::observe(
            stdio(),
            tracker.epoch(),
            "search_symbols",
            Some(&args(json!({ "query": "anchor" }))),
        )
        .expect("the same call, keyed against the new incarnation");
        assert!(
            serve_equal(&mut tracker, &k, 1, 1).is_none(),
            "count 1 after clear"
        );
        assert_eq!(tracker.count_of(&k), Some(1));
        assert!(serve_equal(&mut tracker, &k, 1, 1).is_none(), "count 2");
        let notice = serve_equal(&mut tracker, &k, 1, 1).expect("count 3 notices again");
        assert_eq!(notice.repeat_count(), 3);
    }

    // F2 (round 2) — the seam mints the key BEFORE dispatch and records the
    // serve AFTER it, and rmcp runs every request in its own task, so a
    // concurrent request's incarnation `clear()` can land in between. A serve
    // whose observation predates that clear must never anchor or join a run
    // that spans it: `generation` is a per-process counter, so a replacement
    // index can hand back evidence byte-equal to the dead one's.
    #[test]
    fn serve_observed_before_a_clear_never_joins_a_run() {
        let mint = |tracker: &RepeatTracker| {
            RepeatKey::observe(
                stdio(),
                tracker.epoch(),
                "search_symbols",
                Some(&args(json!({ "query": "anchor" }))),
            )
            .expect("an eligible stdio call keys the tracker")
        };

        let mut tracker = RepeatTracker::default();
        // A run already accumulating when the incarnation changes.
        let stale = mint(&tracker);
        assert!(serve_equal(&mut tracker, &stale, 1, 2).is_none());
        assert_eq!(tracker.count_of(&stale), Some(2));
        tracker.clear();
        assert_eq!(tracker.len(), 0, "the clear dropped the run");

        // Three in-flight serves keyed before the clear: no notice, and not
        // even an insertion — a stale observation may not seed a new run.
        for serve in 1..=3 {
            assert!(
                tracker.record_serve(stale.clone(), observed(1)).is_none(),
                "serve {serve} of a pre-clear key must never notice"
            );
            assert_eq!(
                tracker.len(),
                0,
                "serve {serve} of a pre-clear key must not be recorded at all"
            );
        }

        // Positive control: a key minted AFTER the clear accumulates normally
        // and notices at 3.
        let fresh = mint(&tracker);
        assert_ne!(
            fresh, stale,
            "the epoch is part of the key, so a re-minted key is a different key"
        );
        assert!(tracker.record_serve(fresh.clone(), observed(1)).is_none());
        assert!(tracker.record_serve(fresh.clone(), observed(1)).is_none());
        let notice = tracker
            .record_serve(fresh.clone(), observed(1))
            .expect("a post-clear key notices at 3");
        assert_eq!(notice.repeat_count(), 3);
        assert_eq!(tracker.len(), 1);

        // The stale key stays inert even alongside the live run it shadows.
        assert!(tracker.record_serve(stale, observed(1)).is_none());
        assert_eq!(tracker.len(), 1, "the stale serve added nothing");
        assert_eq!(tracker.count_of(&fresh), Some(3), "and disturbed nothing");
    }

    // F3 (round 2) — the accumulating lane must be a POSITIVE declaration, not
    // the absence of an HTTP marker. A future transport that reaches
    // `call_tool` without inserting rmcp's `http::request::Parts` — its SSE
    // server, a socket adapter, an in-process `serve_directly` — would
    // otherwise silently become ONE shared accumulating bucket, which is the
    // cross-client false-notice shape research.md R12 finding 3 rated a
    // BLOCKER. The `Parts` check stays as the belt.
    #[test]
    fn only_a_declared_stdio_transport_ever_accumulates() {
        // The default is inert: a server whose entry point never declared its
        // transport cannot accumulate, with or without the HTTP marker.
        assert_eq!(TransportLane::default(), TransportLane::Shared);
        for http_parts_present in [false, true] {
            let witness =
                SessionDiscriminator::from_observations(TransportLane::Shared, http_parts_present);
            assert_eq!(
                witness.lane(),
                SessionDiscriminator::HttpInert,
                "an undeclared lane is inert (http_parts_present={http_parts_present})"
            );
            assert!(
                RepeatKey::observe(witness, epoch0(), "search_symbols", None).is_none(),
                "an undeclared lane must never key the tracker \
                 (http_parts_present={http_parts_present})"
            );
        }

        // Belt: a declared stdio server that nonetheless sees the HTTP marker
        // is inert too — the two observations must agree.
        let contradicted = SessionDiscriminator::from_observations(TransportLane::Stdio, true);
        assert_eq!(contradicted.lane(), SessionDiscriminator::HttpInert);
        assert!(RepeatKey::observe(contradicted, epoch0(), "search_symbols", None).is_none());

        // Positive control: a declared stdio lane with no HTTP marker keys the
        // tracker and notices on the third identical serve.
        let declared = SessionDiscriminator::from_observations(TransportLane::Stdio, false);
        assert_eq!(declared.lane(), SessionDiscriminator::Stdio);
        let k = RepeatKey::observe(
            declared,
            epoch0(),
            "search_symbols",
            Some(&args(json!({ "query": "anchor" }))),
        )
        .expect("a declared stdio lane keys the tracker");
        let mut tracker = RepeatTracker::default();
        assert!(serve_equal(&mut tracker, &k, 1, 2).is_none());
        let notice = serve_equal(&mut tracker, &k, 1, 1).expect("declared stdio notices at 3");
        assert_eq!(notice.repeat_count(), 3);
    }

    // Structural non-keys: the inert HTTP lane and set-valued fan-out never
    // touch the tracker; a stdio single-project call does (control).
    #[test]
    fn http_inert_lane_and_projects_fan_out_never_key() {
        let single = args(json!({ "query": "anchor" }));
        assert!(
            RepeatKey::observe(
                LaneWitness::assume(SessionDiscriminator::HttpInert),
                epoch0(),
                "search_symbols",
                Some(&single)
            )
            .is_none(),
            "no observable session identity => no key"
        );
        let fan_out = args(json!({ "query": "anchor", "projects": ["*"] }));
        assert!(
            RepeatKey::observe(stdio(), epoch0(), "search_symbols", Some(&fan_out)).is_none(),
            "a set-valued projects fan-out never keys"
        );
        let null_projects = args(json!({ "query": "anchor", "projects": null }));
        let keyed = RepeatKey::observe(stdio(), epoch0(), "search_symbols", Some(&null_projects))
            .expect("projects:null is absence");
        let control = RepeatKey::observe(stdio(), epoch0(), "search_symbols", Some(&single))
            .expect("stdio single-project keys");
        assert_eq!(control.tool(), "search_symbols");
        // Fingerprint identity: same args => same key; different args => different.
        assert_eq!(
            control,
            RepeatKey::observe(stdio(), epoch0(), "search_symbols", Some(&single)).expect("same")
        );
        assert_ne!(
            control, keyed,
            "an explicit null field changes the canonical JSON"
        );
        let other = args(json!({ "query": "other" }));
        assert_ne!(
            control,
            RepeatKey::observe(stdio(), epoch0(), "search_symbols", Some(&other)).expect("other")
        );
        // Absent arguments hash as the empty object.
        let empty = args(json!({}));
        assert_eq!(
            RepeatKey::observe(stdio(), epoch0(), "get_repo_map", None).expect("none"),
            RepeatKey::observe(stdio(), epoch0(), "get_repo_map", Some(&empty)).expect("empty")
        );
    }

    // F1 — the witness observes the RESULT, not only the evidence: a serve
    // whose rendered body changed while the evidence stayed equal (the
    // `search_text` zero-hit untracked-file diagnostic is computed from live
    // git status, outside the evidence fence) replaces the run at count 1 and
    // never earns a notice.
    #[test]
    fn body_change_with_equal_evidence_replaces_run() {
        let full = serde_json::to_value(evidence(3)).expect("evidence serializes");
        let serve = |blocks: &[&str]| {
            let mut result = result_with(full.clone(), None, None);
            result.content = blocks
                .iter()
                .map(|text| ContentBlock::text(*text))
                .collect();
            ServeObservation::from_result(&result)
        };
        let k = key("search_text", "needle-zz");

        let mut tracker = RepeatTracker::default();
        assert!(
            tracker
                .record_serve(k.clone(), serve(&["no matches"]))
                .is_none()
        );
        assert!(
            tracker
                .record_serve(k.clone(), serve(&["no matches"]))
                .is_none()
        );
        assert_eq!(tracker.count_of(&k), Some(2));
        assert!(
            tracker
                .record_serve(
                    k.clone(),
                    serve(&["no matches\n\nuntracked file may match: 1 untracked path(s)"]),
                )
                .is_none(),
            "a changed body under equal evidence must never notice"
        );
        assert_eq!(
            tracker.count_of(&k),
            Some(1),
            "the run restarts at 1 on the new body"
        );
        // Block framing is part of identity: the same bytes split differently
        // are a different result.
        assert!(
            tracker
                .record_serve(k.clone(), serve(&["ab", "c"]))
                .is_none()
        );
        assert_eq!(tracker.count_of(&k), Some(1));
        assert!(
            tracker
                .record_serve(k.clone(), serve(&["a", "bc"]))
                .is_none()
        );
        assert_eq!(tracker.count_of(&k), Some(1), "framing differs => restart");

        // Positive control: an unchanged body notices on the third serve.
        let mut tracker = RepeatTracker::default();
        assert!(
            tracker
                .record_serve(k.clone(), serve(&["no matches"]))
                .is_none()
        );
        assert!(
            tracker
                .record_serve(k.clone(), serve(&["no matches"]))
                .is_none()
        );
        let notice = tracker
            .record_serve(k.clone(), serve(&["no matches"]))
            .expect("an unchanged body under equal evidence notices");
        assert_eq!(notice.repeat_count(), 3);
    }

    // F1 (round 2) — a response that consulted an input the index does NOT
    // fence can never carry the forward claim, however equal the past serves
    // were. The reading code latches the fact on the dispatch scope; the seam
    // reads it back and REMOVES the run.
    #[tokio::test]
    async fn unfenced_input_removes_the_run_and_never_notices() {
        let full = serde_json::to_value(evidence(2)).expect("evidence serializes");
        let k = key("search_text", "needle-zz");
        let serve = |latched: bool| {
            let full = full.clone();
            async move {
                result_status::with_project_evidence_scope(None, async {
                    if latched {
                        result_status::note_unfenced_input();
                    }
                    ServeObservation::from_result(&result_with(full, None, None))
                })
                .await
            }
        };

        // A run at 2, then one latched serve: removed, never noticed.
        let mut tracker = RepeatTracker::default();
        assert!(serve_equal(&mut tracker, &k, 2, 2).is_none());
        assert_eq!(tracker.count_of(&k), Some(2), "the run reached 2");
        let observation = serve(true).await;
        assert_eq!(
            observation.unobserved_reason(),
            Some(UnobservedReason::UnfencedInput)
        );
        assert!(
            tracker.record_serve(k.clone(), observation).is_none(),
            "an unfenced serve never notices"
        );
        assert_eq!(
            tracker.count_of(&k),
            None,
            "the run is removed, not merely skipped"
        );

        // And it never accumulates on that lane: three more latched serves
        // with identical evidence and body still leave nothing behind.
        for _ in 0..3 {
            assert!(tracker.record_serve(k.clone(), serve(true).await).is_none());
        }
        assert_eq!(tracker.count_of(&k), None, "still nothing");

        // Positive control: the SAME evidence and the SAME body, with no
        // unfenced input consulted, advance to a notice at 3.
        let mut tracker = RepeatTracker::default();
        let mut last = None;
        for _ in 0..3 {
            last = tracker.record_serve(k.clone(), serve(false).await);
        }
        assert_eq!(last.expect("the fenced control notices").repeat_count(), 3);

        // Scope discipline: the latch is idempotent within one dispatch and
        // dies with it — a following dispatch starts clean, so one call's
        // latch can never leak into the next.
        result_status::with_project_evidence_scope(None, async {
            assert!(
                !result_status::unfenced_input_consulted(),
                "a fresh dispatch scope starts clean"
            );
            result_status::note_unfenced_input();
            assert!(result_status::unfenced_input_consulted());
            result_status::note_unfenced_input();
            assert!(
                result_status::unfenced_input_consulted(),
                "the latch is idempotent"
            );
            // Clearing the evidence slot must not clear the latch: they answer
            // different questions about the same dispatch.
            result_status::clear_project_evidence();
            assert!(result_status::unfenced_input_consulted());
        })
        .await;
        result_status::with_project_evidence_scope(None, async {
            assert!(
                !result_status::unfenced_input_consulted(),
                "the next dispatch scope must not observe the previous one's latch"
            );
        })
        .await;
    }

    // Delivery: both carriers, appended to the FINAL text block, prior bytes
    // and isError untouched; a text block is created when none exists.
    #[test]
    fn notice_attaches_to_final_text_block_and_meta() {
        let k = key("get_repo_map", "anchor");
        let witness =
            RepeatWitness::observe(&observed_serve(4), &observed_serve(4)).expect("witness");
        let notice = RepeatNotice::new(witness, 3, &k).expect("notice");

        let mut result = CallToolResult::success(vec![
            ContentBlock::text("first"),
            ContentBlock::text("second"),
        ]);
        result.is_error = Some(true);
        let before_meta = result.meta.clone();
        notice.attach(&mut result);

        assert_eq!(result.is_error, Some(true), "isError untouched");
        assert_eq!(
            result.content.len(),
            2,
            "no block added when a text block exists"
        );
        assert_eq!(result.content[0].as_text().expect("text").text, "first");
        assert_eq!(
            result.content[1].as_text().expect("text").text,
            format!("second\n\n{}", notice.text())
        );
        let meta = result.meta.as_ref().expect("meta created");
        assert!(before_meta.is_none());
        let view = meta.0.get(REPEAT_NOTICE_META_KEY).expect("meta carrier");
        assert_eq!(
            view,
            &json!({
                "contract_version": 1,
                "repeat_count": 3,
                "tool": "get_repo_map",
                "request_hash": k.request_hash().as_str(),
                "evidence_generation": 4,
            })
        );

        // No text block at all: one is pushed so the carriers never split.
        let mut empty = CallToolResult::success(vec![]);
        notice.attach(&mut empty);
        assert_eq!(empty.content.len(), 1);
        assert_eq!(
            empty.content[0].as_text().expect("text").text,
            notice.text()
        );
        assert!(
            empty
                .meta
                .as_ref()
                .is_some_and(|m| m.0.contains_key(REPEAT_NOTICE_META_KEY))
        );
    }
}
