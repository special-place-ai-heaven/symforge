// Server-only integration test: the STEL planner/facade lives behind
// `#[cfg(feature = "server")]` (see `src/lib.rs` `pub mod stel`). Gating the
// whole file keeps `--no-default-features --features embed --all-targets`
// compiling, mirroring `tests/worktree_awareness.rs`.
#![cfg(feature = "server")]

//! A1a lossless-or-loud conformance guard (root D-A0, CULPRIT A).
//!
//! CULPRIT A is the lossy facade: it routes a curated subset of each call's
//! params and *silently drops the rest*. A1a erects the non-regressable
//! structural guard against the silent-drop class: every `StelRequest` field
//! must resolve to an explicit [`ParamDisposition`]
//! (`Routed | Forwarded | Refused | NotApplicable`) at the single plan choke
//! point — there is NO silent variant the type can hold, and these tests prove
//! it across the emittable tool surface and through the real `from_value`
//! deserialization boundary.
//!
//! **Assertion-only.** A1a records and asserts the CURRENT disposition of each
//! field; it does NOT newly forward or short-circuit anything (that is A1b).
//! These tests therefore make ZERO behavioral claims — they never assert which
//! tool a route picks or what an arg value is (the golden replay corpus owns
//! that). They assert exactly one thing: no field is silently unaccounted-for.

use serde_json::{Value, json};
use symforge::stel::{
    IntentBucket, ParamDisposition, StelRequest, build_plan, classify_param_dispositions,
};

/// Every `StelRequest` field name the classifier must account for. If a field
/// is added to `StelRequest`, the classifier's fixed-length array forces a
/// compile-time decision and this list must grow with it — keeping the guard
/// complete by construction.
const EXPECTED_FIELDS: &[&str] = &[
    "query",
    "intent",
    "symbol",
    "path",
    "max_tokens",
    "preview",
    "project",
    "projects",
];

/// A representative query per intent family, chosen to exercise distinct
/// emittable routes (find/read/trace/impact/orient/meta + auto fusion +
/// explicit phrasings). The point is route DIVERSITY across the emittable tool
/// surface, not pinning any specific tool — the golden corpus pins tools.
const ROUTE_PROBES: &[(&str, Option<IntentBucket>)] = &[
    // Auto / fusion and smart-query fallbacks.
    ("stel planner find helper", None),
    ("planner", None),
    // Explicit buckets.
    ("find cfg_if macro usage", Some(IntentBucket::Find)),
    ("locate cfg_if symbol", Some(IntentBucket::Find)),
    ("outline src/lib.rs", Some(IntentBucket::Read)),
    ("who references cfg_if", Some(IntentBucket::Trace)),
    ("what depends on TaskStatus", Some(IntentBucket::Impact)),
    ("map of workspace crates", Some(IntentBucket::Orient)),
    ("how does cfg_if work", Some(IntentBucket::Orient)),
    ("index health", Some(IntentBucket::Meta)),
    // Multi-hop ordered plans.
    ("search then fetch cfg_if body", None),
    ("find test.js then read it", None),
    // Phrasing routes that consume path/symbol on specific tools.
    ("body of cfg_if in src/lib.rs", Some(IntentBucket::Read)),
    ("Database symbol in records.py", Some(IntentBucket::Read)),
];

/// A `StelRequest` with a sentinel value in EVERY field, so each field is
/// "set" and the classifier must produce a non-silent disposition for all of
/// them. `symbol` is a bare identifier (prose is a separately-tested Refused
/// case). `query`/`intent` come from the route probe.
fn fully_populated_request(query: &str, intent: Option<IntentBucket>) -> StelRequest {
    StelRequest {
        query: query.to_string(),
        intent,
        path: Some("src/lib.rs".to_string()),
        symbol: Some("cfg_if".to_string()),
        max_tokens: Some(2048),
        preview: Some(false),
        project: Some("alpha".to_string()),
        projects: Some(vec!["alpha".to_string(), "beta".to_string()]),
    }
}

/// THE STRUCTURAL GUARD: for each emittable route, drive a request whose every
/// field is set and assert that EVERY field resolves to a non-silent
/// `ParamDisposition`. The test FAILS if any field is silently dropped — there
/// is no disposition the classifier can return that is not explicit, and the
/// array is fixed-length over all `StelRequest` fields, so absence is
/// structurally impossible. This is what makes the silent-drop class (D-A0)
/// non-regressable.
#[test]
fn every_field_resolves_to_a_non_silent_disposition_on_every_route() {
    for (query, intent) in ROUTE_PROBES {
        let request = fully_populated_request(query, *intent);
        let plan = build_plan(&request);
        let dispositions = classify_param_dispositions(&request, &plan);

        // The accounting covers exactly the known field set, in order — no field
        // missing, none extra. This is the "lossless" half: the choke point sees
        // every field.
        let names: Vec<&str> = dispositions.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names, EXPECTED_FIELDS,
            "route `{query}` ({intent:?}): classifier must account for exactly the \
             StelRequest fields, in order"
        );

        // The "loud" half: every accounted field has an EXPLICIT disposition —
        // never silently dropped.
        for (name, disposition) in &dispositions {
            assert!(
                disposition.is_explicit(),
                "route `{query}` ({intent:?}): field `{name}` resolved to a \
                 non-explicit (silent) disposition: {disposition:?}"
            );
        }
    }
}

/// Crossing the real `from_value` boundary (the same `serde_json::from_value`
/// path `dispatch_tool_for_tests` uses for the `symforge` tool) must preserve
/// every field so the classifier still sees it — a field lost in
/// deserialization would silently drop before the planner ever runs. This wires
/// the JSON-wire boundary into the same lossless-or-loud assertion.
#[test]
fn fields_survive_from_value_deserialization_and_stay_non_silent() {
    let wire: Value = json!({
        "query": "stel planner find helper",
        "intent": "find",
        "path": "src/lib.rs",
        "symbol": "cfg_if",
        "max_tokens": 2048,
        "preview": false,
        "project": "alpha",
        "projects": ["alpha", "beta"],
    });

    // Same boundary `dispatch_tool_for_tests("symforge", ...)` crosses: the wire
    // JSON deserializes into the production request struct.
    let request: StelRequest =
        serde_json::from_value(wire).expect("sentinel request must deserialize");

    // Every sentinel survived the boundary (none silently dropped in serde).
    assert_eq!(request.query, "stel planner find helper");
    assert_eq!(request.intent, Some(IntentBucket::Find));
    assert_eq!(request.path.as_deref(), Some("src/lib.rs"));
    assert_eq!(request.symbol.as_deref(), Some("cfg_if"));
    assert_eq!(request.max_tokens, Some(2048));
    assert_eq!(request.preview, Some(false));
    assert_eq!(request.project.as_deref(), Some("alpha"));
    assert_eq!(
        request.projects.as_deref(),
        Some(["alpha".to_string(), "beta".to_string()].as_slice())
    );

    let plan = build_plan(&request);
    let dispositions = classify_param_dispositions(&request, &plan);
    for (name, disposition) in &dispositions {
        assert!(
            disposition.is_explicit(),
            "post-deserialization field `{name}` silently dropped: {disposition:?}"
        );
    }
}

/// Helper: look up one field's disposition in a classified plan.
fn disposition_of(
    dispositions: &[(&'static str, ParamDisposition)],
    field: &str,
) -> ParamDisposition {
    dispositions
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, d)| d.clone())
        .unwrap_or_else(|| panic!("field `{field}` must be classified"))
}

/// Records the CURRENT disposition of each field, pinning today's behavior so a
/// future increment that CHANGES routing (e.g. A1b forwarding `path` /
/// `max_tokens` into plan args) updates this deliberately rather than drifting
/// silently. Documents the A1a baseline; it is the behavior-change tripwire, not
/// a behavioral claim about which tool a route picks.
#[test]
fn current_dispositions_pin_the_a1a_baseline() {
    // Constant across routes: query/intent are always consumed; max_tokens /
    // preview are handler-forwarded (post-planner); project / projects are
    // loudly refused (D9).
    let populated = fully_populated_request("stel planner find helper", Some(IntentBucket::Find));
    let populated_plan = build_plan(&populated);
    let populated_d = classify_param_dispositions(&populated, &populated_plan);

    assert_eq!(
        disposition_of(&populated_d, "query"),
        ParamDisposition::Routed
    );
    assert_eq!(
        disposition_of(&populated_d, "intent"),
        ParamDisposition::Routed
    );
    assert!(matches!(
        disposition_of(&populated_d, "project"),
        ParamDisposition::Refused { .. }
    ));
    assert!(matches!(
        disposition_of(&populated_d, "projects"),
        ParamDisposition::Refused { .. }
    ));
    assert!(matches!(
        disposition_of(&populated_d, "max_tokens"),
        ParamDisposition::Forwarded { .. }
    ));
    assert!(matches!(
        disposition_of(&populated_d, "preview"),
        ParamDisposition::Forwarded { .. }
    ));
    // With a bare-identifier `symbol` AND a `path`, the symbol route fires and
    // BOTH are honored — the plan carries them, so both are Routed today.
    assert_eq!(
        disposition_of(&populated_d, "symbol"),
        ParamDisposition::Routed
    );
    assert_eq!(
        disposition_of(&populated_d, "path"),
        ParamDisposition::Routed
    );

    // A1a BASELINE GAP (the A1b target): a multi-word fuzzy find with NO
    // `symbol` routes through find-fusion, which does NOT thread `path` into its
    // args today. `path` is therefore explicitly NotApplicable — the planner saw
    // it and did not consume it on this route — NOT a silent drop. A1b will flip
    // this to Routed/Forwarded and must re-baseline this exact assertion.
    let no_symbol = StelRequest {
        query: "stel planner find helper".to_string(),
        intent: Some(IntentBucket::Find),
        path: Some("src/lib.rs".to_string()),
        ..Default::default()
    };
    let no_symbol_plan = build_plan(&no_symbol);
    let no_symbol_d = classify_param_dispositions(&no_symbol, &no_symbol_plan);
    assert_eq!(
        disposition_of(&no_symbol_d, "path"),
        ParamDisposition::NotApplicable,
        "A1a baseline: fuzzy-find route does not consume `path` yet (A1b target)"
    );
    // The unset `symbol` on this route is NotApplicable, never silent.
    assert_eq!(
        disposition_of(&no_symbol_d, "symbol"),
        ParamDisposition::NotApplicable
    );
}

/// A prose `symbol` is loudly Refused (the `symbol_contract_violation`
/// precedent), never silently swallowed as a tool `name`. Confirms the Refused
/// disposition fires for the prose case, matching today's handler behavior.
#[test]
fn prose_symbol_classifies_as_refused() {
    let request = StelRequest {
        query: "trace how status updates flow".to_string(),
        intent: Some(IntentBucket::Trace),
        symbol: Some("how status updates flow".to_string()),
        ..Default::default()
    };
    let plan = build_plan(&request);
    let dispositions = classify_param_dispositions(&request, &plan);
    let symbol = dispositions
        .iter()
        .find(|(name, _)| *name == "symbol")
        .map(|(_, d)| d.clone())
        .expect("symbol must be classified");
    assert!(
        matches!(symbol, ParamDisposition::Refused { .. }),
        "prose symbol must be Refused, got {symbol:?}"
    );
}

/// An UNSET field is NotApplicable — the planner saw the (absent) field and did
/// not act on it. Still an explicit disposition, never silent.
#[test]
fn unset_optional_fields_are_not_applicable_not_silent() {
    let request = StelRequest {
        query: "find cfg_if macro usage".to_string(),
        ..Default::default()
    };
    let plan = build_plan(&request);
    let dispositions = classify_param_dispositions(&request, &plan);

    for field in [
        "symbol",
        "path",
        "max_tokens",
        "preview",
        "project",
        "projects",
    ] {
        let disposition = dispositions
            .iter()
            .find(|(name, _)| *name == field)
            .map(|(_, d)| d.clone())
            .unwrap_or_else(|| panic!("field `{field}` must be classified"));
        assert_eq!(
            disposition,
            ParamDisposition::NotApplicable,
            "unset field `{field}` must be NotApplicable, got {disposition:?}"
        );
        assert!(disposition.is_explicit());
    }
    // query is always present and Routed even when everything else is default.
    let query = dispositions
        .iter()
        .find(|(name, _)| *name == "query")
        .map(|(_, d)| d.clone())
        .expect("query classified");
    assert_eq!(query, ParamDisposition::Routed);
}
