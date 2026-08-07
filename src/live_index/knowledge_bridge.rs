//! Deterministic, source-local evidence bridge over the resident live index.
//!
//! This is a derived projection: it stores compact anchors and indices only,
//! never copied knowledge bodies or generated semantic claims.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;

use crate::domain::{FileDisposition, IndexTargets, LanguageId, SourceIdentity, SymbolKind};

use super::graph::SymbolId;
use super::store::{IndexedFile, LiveIndex};

pub const STRUCTURED_CODE_PATH_RULE_ID: &str = "bridge.structured-code-path.v1";

const DEFAULT_MAX_KNOWLEDGE_CARDS: usize = 25_000;
const DEFAULT_MAX_BRIDGE_CANDIDATES: usize = 25_000;
const DEFAULT_MAX_OWNERSHIP_SELECTORS: usize = 2_000;
const DEFAULT_MAX_AMBIGUOUS_SAMPLES: usize = 8;
const DEFAULT_MAX_BRIDGE_METADATA_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KnowledgeAnchorId {
    pub path: String,
    pub content_hash: String,
    pub start_byte: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeAnchor {
    pub id: KnowledgeAnchorId,
    pub source: SourceIdentity,
    pub content_generation: u64,
    pub path: String,
    pub content_hash: String,
    pub byte_range: Range<u32>,
    pub line_range: Range<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KnowledgeRole {
    Architecture,
    OwnershipGovernance,
    DecisionInvariant,
    SchemaContract,
    Operations,
    TestingSecurity,
    PlanHandoff,
    Other,
}

/// Why a card carries a role. Deliberately carries NO anchor: every variant
/// used to clone one, but `roles_for_card` is always handed the very anchor its
/// `KnowledgeCard` then stores, so the copy could never differ from
/// `card.anchor`. Read the anchor from the card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleEvidence {
    DeclaredSpan,
    HeadingRule { rule_id: String },
    PathConvention { rule_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeCard {
    pub anchor: KnowledgeAnchor,
    pub roles: Vec<(KnowledgeRole, RoleEvidence)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeAnchor {
    pub source: SourceIdentity,
    pub content_generation: u64,
    pub id: CodeAnchorId,
    pub content_hash: String,
    pub line_range: Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CodeAnchorId {
    File { path: String },
    Symbol { symbol: SymbolId, start_line: u32 },
}

impl Ord for CodeAnchorId {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::File { path: left }, Self::File { path: right }) => left.cmp(right),
            (Self::File { .. }, Self::Symbol { .. }) => Ordering::Less,
            (Self::Symbol { .. }, Self::File { .. }) => Ordering::Greater,
            (
                Self::Symbol {
                    symbol: left,
                    start_line: left_line,
                },
                Self::Symbol {
                    symbol: right,
                    start_line: right_line,
                },
            ) => left.cmp(right).then_with(|| left_line.cmp(right_line)),
        }
    }
}

impl PartialOrd for CodeAnchorId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BridgeEvidenceKind {
    RepositoryLink,
    ExactPathToken,
    ExactCodeSpanSymbol,
    DeclaredOwnershipSelector,
    SupportedStructuredValue { rule_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BridgeResolution {
    ResolvedExact(CodeAnchor),
    ResolvedDeclaredSet {
        selector_anchor: KnowledgeAnchor,
        matched_count: u64,
    },
    Ambiguous {
        candidate_count: u32,
        bounded_samples: Vec<CodeAnchor>,
    },
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KnowledgeCodeLinkId(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeCodeLink {
    pub id: KnowledgeCodeLinkId,
    pub evidence: KnowledgeAnchor,
    pub evidence_kind: BridgeEvidenceKind,
    pub resolution: BridgeResolution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnowledgeLinkResolution {
    ResolvedExact(KnowledgeAnchor),
    Ambiguous {
        candidate_count: u32,
        bounded_samples: Vec<KnowledgeAnchor>,
    },
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeKnowledgeLink {
    pub evidence: KnowledgeAnchor,
    pub resolution: KnowledgeLinkResolution,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DerivedLimitKind {
    Cards,
    BridgeLinks,
    OwnershipSelectors,
    AmbiguousSamples,
    AuthorityRecords,
    Findings,
    MetadataBytes,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitBreach {
    pub kind: DerivedLimitKind,
    pub omitted: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DerivedCoverage {
    #[default]
    Complete,
    Truncated {
        breaches: Vec<LimitBreach>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct KnowledgeBridge {
    pub cards: Vec<KnowledgeCard>,
    pub forward: Vec<KnowledgeCodeLink>,
    pub reverse_exact: BTreeMap<CodeAnchorId, Vec<u32>>,
    pub ownership_selectors: Vec<u32>,
    pub knowledge_links: Vec<KnowledgeKnowledgeLink>,
    pub reverse_knowledge: BTreeMap<KnowledgeAnchorId, Vec<u32>>,
    pub coverage: DerivedCoverage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BridgeLimits {
    pub max_cards: usize,
    pub max_candidates: usize,
    pub max_ownership_selectors: usize,
    pub max_ambiguous_samples: usize,
    pub max_metadata_bytes: usize,
}

impl Default for BridgeLimits {
    fn default() -> Self {
        Self {
            max_cards: DEFAULT_MAX_KNOWLEDGE_CARDS,
            max_candidates: DEFAULT_MAX_BRIDGE_CANDIDATES,
            max_ownership_selectors: DEFAULT_MAX_OWNERSHIP_SELECTORS,
            max_ambiguous_samples: DEFAULT_MAX_AMBIGUOUS_SAMPLES,
            max_metadata_bytes: DEFAULT_MAX_BRIDGE_METADATA_BYTES,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CandidateSelector {
    Path(String),
    Symbol(String),
    Ownership(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BridgeCandidate {
    range: Range<usize>,
    evidence_kind: BridgeEvidenceKind,
    selector: CandidateSelector,
}

impl BridgeCandidate {
    fn sort_key(&self) -> (usize, usize, &BridgeEvidenceKind, &CandidateSelector) {
        (
            self.range.start,
            self.range.end,
            &self.evidence_kind,
            &self.selector,
        )
    }
}

struct BudgetState<'a> {
    limits: &'a BridgeLimits,
    accepted_cards: usize,
    accepted_candidates: usize,
    accepted_selectors: usize,
    accepted_samples: usize,
    metadata_bytes: usize,
    breaches: BTreeMap<DerivedLimitKind, u64>,
}

impl<'a> BudgetState<'a> {
    fn new(limits: &'a BridgeLimits) -> Self {
        Self {
            limits,
            accepted_cards: 0,
            accepted_candidates: 0,
            accepted_selectors: 0,
            accepted_samples: 0,
            metadata_bytes: 0,
            breaches: BTreeMap::new(),
        }
    }

    fn admit_card(&mut self, metadata_bytes: usize) -> bool {
        if self.accepted_cards >= self.limits.max_cards {
            self.breach(DerivedLimitKind::Cards, 1);
            return false;
        }
        if !self.admit_metadata(metadata_bytes) {
            return false;
        }
        self.accepted_cards += 1;
        true
    }

    fn breach(&mut self, kind: DerivedLimitKind, omitted: usize) {
        let omitted = u64::try_from(omitted).unwrap_or(u64::MAX);
        self.breaches
            .entry(kind)
            .and_modify(|count| *count = count.saturating_add(omitted))
            .or_insert(omitted);
    }

    fn admit_candidate(&mut self) -> bool {
        if self.accepted_candidates >= self.limits.max_candidates {
            self.breach(DerivedLimitKind::BridgeLinks, 1);
            return false;
        }
        self.accepted_candidates += 1;
        true
    }

    fn admit_selector(&mut self) -> bool {
        if self.accepted_selectors >= self.limits.max_ownership_selectors {
            self.breach(DerivedLimitKind::OwnershipSelectors, 1);
            return false;
        }
        self.accepted_selectors += 1;
        true
    }

    fn bounded_samples(&mut self, candidates: &[CodeAnchor]) -> Vec<CodeAnchor> {
        let remaining = self
            .limits
            .max_ambiguous_samples
            .saturating_sub(self.accepted_samples);
        let kept = candidates.len().min(remaining);
        if kept < candidates.len() {
            self.breach(DerivedLimitKind::AmbiguousSamples, candidates.len() - kept);
        }
        self.accepted_samples = self.accepted_samples.saturating_add(kept);
        candidates[..kept].to_vec()
    }

    fn admit_metadata(&mut self, bytes: usize) -> bool {
        let Some(next) = self.metadata_bytes.checked_add(bytes) else {
            self.breach(DerivedLimitKind::MetadataBytes, 1);
            return false;
        };
        if next > self.limits.max_metadata_bytes {
            self.breach(DerivedLimitKind::MetadataBytes, 1);
            return false;
        }
        self.metadata_bytes = next;
        true
    }

    fn coverage(self) -> DerivedCoverage {
        if self.breaches.is_empty() {
            DerivedCoverage::Complete
        } else {
            DerivedCoverage::Truncated {
                breaches: self
                    .breaches
                    .into_iter()
                    .map(|(kind, omitted)| LimitBreach { kind, omitted })
                    .collect(),
            }
        }
    }
}

pub fn build_knowledge_bridge(
    live: &LiveIndex,
    source: &SourceIdentity,
    content_generation: u64,
    limits: &BridgeLimits,
) -> KnowledgeBridge {
    let targets = targets_by_path(live);
    let code_paths: Vec<&str> = live
        .files
        .iter()
        .filter_map(|(path, file)| {
            targets_for_file(path, file, &targets)
                .includes_code()
                .then_some(path.as_str())
        })
        .collect();
    let mut knowledge_paths: Vec<&str> = live
        .files
        .iter()
        .filter_map(|(path, file)| {
            targets_for_file(path, file, &targets)
                .includes_knowledge()
                .then_some(path.as_str())
        })
        .collect();
    knowledge_paths.sort_unstable();

    let mut cards = Vec::new();
    let mut extracted_candidates = Vec::new();
    let mut referenced_symbols = BTreeSet::new();
    let mut budget = BudgetState::new(limits);
    for path in knowledge_paths {
        let Some(file) = live.files.get(path).map(AsRef::as_ref) else {
            continue;
        };
        let Ok(decoded) = crate::knowledge::decode_searchable_text(&file.content) else {
            continue;
        };
        let leading = decoded.leading_bytes as usize;
        for card in derive_knowledge_cards(source, content_generation, path, file) {
            if budget.admit_card(knowledge_card_metadata_bytes(&card)) {
                cards.push(card);
            }
        }
        let mut candidates = extract_candidates(path, file, decoded.text);
        for candidate in &mut candidates {
            candidate.range.start = candidate.range.start.saturating_add(leading);
            candidate.range.end = candidate.range.end.saturating_add(leading);
            if let CandidateSelector::Symbol(selector) = &candidate.selector {
                referenced_symbols.insert(selector.clone());
            }
        }
        candidates.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        candidates.dedup_by(|left, right| left == right);
        extracted_candidates.push((path, file, candidates));
    }
    let symbol_index = symbol_index(
        live,
        source,
        content_generation,
        &targets,
        &referenced_symbols,
    );

    let mut forward = Vec::new();
    let mut knowledge_links = Vec::new();

    for (path, file, candidates) in extracted_candidates {
        for (ordinal, candidate) in candidates.into_iter().enumerate() {
            if matches!(candidate.selector, CandidateSelector::Ownership(_))
                && !budget.admit_selector()
            {
                continue;
            }
            if !budget.admit_candidate() {
                continue;
            }

            let evidence = knowledge_anchor(
                source,
                content_generation,
                path,
                file,
                candidate.range.clone(),
            );
            match &candidate.selector {
                CandidateSelector::Path(selector) => {
                    let Some(target_path) =
                        resolve_repo_path(path, selector, live.files.keys().map(String::as_str))
                    else {
                        continue;
                    };
                    if let Some(target_file) = live.files.get(&target_path) {
                        let target_targets = targets_for_file(&target_path, target_file, &targets);
                        if target_targets.includes_code() {
                            let resolution = BridgeResolution::ResolvedExact(file_code_anchor(
                                source,
                                content_generation,
                                &target_path,
                                target_file,
                            ));
                            push_forward_link(
                                &mut forward,
                                &mut budget,
                                evidence,
                                candidate.evidence_kind,
                                resolution,
                                ordinal,
                                selector,
                            );
                        } else if target_targets.includes_knowledge() {
                            let target = knowledge_anchor(
                                source,
                                content_generation,
                                &target_path,
                                target_file,
                                0..target_file.content.len(),
                            );
                            let link = KnowledgeKnowledgeLink {
                                evidence,
                                resolution: KnowledgeLinkResolution::ResolvedExact(target),
                            };
                            if budget.admit_metadata(knowledge_link_metadata_bytes(&link)) {
                                knowledge_links.push(link);
                            }
                        } else {
                            push_forward_link(
                                &mut forward,
                                &mut budget,
                                evidence,
                                candidate.evidence_kind,
                                BridgeResolution::Missing,
                                ordinal,
                                selector,
                            );
                        }
                    } else {
                        let language = LanguageId::from_path(&target_path);
                        let inferred_targets =
                            IndexTargets::for_path(&target_path, language.as_ref());
                        if inferred_targets.includes_code() {
                            push_forward_link(
                                &mut forward,
                                &mut budget,
                                evidence,
                                candidate.evidence_kind,
                                BridgeResolution::Missing,
                                ordinal,
                                selector,
                            );
                        } else {
                            let link = KnowledgeKnowledgeLink {
                                evidence,
                                resolution: KnowledgeLinkResolution::Missing,
                            };
                            if budget.admit_metadata(knowledge_link_metadata_bytes(&link)) {
                                knowledge_links.push(link);
                            }
                        }
                    }
                }
                CandidateSelector::Symbol(selector) => {
                    let matches = symbol_index.get(selector).map(Vec::as_slice).unwrap_or(&[]);
                    let resolution = match matches {
                        [] => BridgeResolution::Missing,
                        [only] => BridgeResolution::ResolvedExact(only.clone()),
                        many => BridgeResolution::Ambiguous {
                            candidate_count: u32::try_from(many.len()).unwrap_or(u32::MAX),
                            bounded_samples: budget.bounded_samples(many),
                        },
                    };
                    push_forward_link(
                        &mut forward,
                        &mut budget,
                        evidence,
                        candidate.evidence_kind,
                        resolution,
                        ordinal,
                        selector,
                    );
                }
                CandidateSelector::Ownership(selector) => {
                    let matched_count = ownership_match_count(selector, &code_paths);
                    let resolution = BridgeResolution::ResolvedDeclaredSet {
                        selector_anchor: evidence.clone(),
                        matched_count,
                    };
                    push_forward_link(
                        &mut forward,
                        &mut budget,
                        evidence,
                        candidate.evidence_kind,
                        resolution,
                        ordinal,
                        selector,
                    );
                }
            }
        }
    }

    cards.sort_by(|left, right| {
        (
            left.anchor.path.as_str(),
            left.anchor.byte_range.start,
            left.anchor.byte_range.end,
        )
            .cmp(&(
                right.anchor.path.as_str(),
                right.anchor.byte_range.start,
                right.anchor.byte_range.end,
            ))
    });
    forward.sort_by(|left, right| {
        (
            left.evidence.path.as_str(),
            left.evidence.byte_range.start,
            &left.evidence_kind,
            &left.id,
        )
            .cmp(&(
                right.evidence.path.as_str(),
                right.evidence.byte_range.start,
                &right.evidence_kind,
                &right.id,
            ))
    });
    knowledge_links.sort_by(|left, right| {
        (left.evidence.path.as_str(), left.evidence.byte_range.start).cmp(&(
            right.evidence.path.as_str(),
            right.evidence.byte_range.start,
        ))
    });

    let mut reverse_exact: BTreeMap<CodeAnchorId, Vec<u32>> = BTreeMap::new();
    let mut ownership_selectors = Vec::new();
    for (index, link) in forward.iter().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        match &link.resolution {
            BridgeResolution::ResolvedExact(anchor) => {
                reverse_exact
                    .entry(anchor.id.clone())
                    .or_default()
                    .push(index);
            }
            BridgeResolution::ResolvedDeclaredSet { .. } => ownership_selectors.push(index),
            BridgeResolution::Ambiguous { .. } | BridgeResolution::Missing => {}
        }
    }
    let mut reverse_knowledge: BTreeMap<KnowledgeAnchorId, Vec<u32>> = BTreeMap::new();
    for (index, link) in knowledge_links.iter().enumerate() {
        if let KnowledgeLinkResolution::ResolvedExact(anchor) = &link.resolution {
            reverse_knowledge
                .entry(anchor.id.clone())
                .or_default()
                .push(u32::try_from(index).unwrap_or(u32::MAX));
        }
    }

    KnowledgeBridge {
        cards,
        forward,
        reverse_exact,
        ownership_selectors,
        knowledge_links,
        reverse_knowledge,
        coverage: budget.coverage(),
    }
}

fn derive_knowledge_cards(
    source: &SourceIdentity,
    content_generation: u64,
    path: &str,
    file: &IndexedFile,
) -> Vec<KnowledgeCard> {
    let sections = if file.language == LanguageId::Markdown {
        crate::knowledge::project_markdown_sections(source, path, &file.content_hash, &file.symbols)
    } else {
        Vec::new()
    };

    if sections.is_empty() {
        let anchor = knowledge_anchor(
            source,
            content_generation,
            path,
            file,
            0..file.content.len(),
        );
        let roles = roles_for_card(path, None);
        return vec![KnowledgeCard { anchor, roles }];
    }

    sections
        .into_iter()
        .filter_map(|unit| {
            let start = usize::try_from(unit.byte_range.start).ok()?;
            let end = usize::try_from(unit.byte_range.end).ok()?;
            if start > end || end > file.content.len() {
                return None;
            }
            let anchor = knowledge_anchor(source, content_generation, path, file, start..end);
            let roles = roles_for_card(path, unit.heading_path.last().map(String::as_str));
            Some(KnowledgeCard { anchor, roles })
        })
        .collect()
}

fn roles_for_card(path: &str, heading: Option<&str>) -> Vec<(KnowledgeRole, RoleEvidence)> {
    let mut roles = BTreeMap::new();
    for (role, rule_id) in path_convention_roles(path) {
        roles
            .entry(role)
            .or_insert_with(|| RoleEvidence::PathConvention {
                rule_id: rule_id.to_string(),
            });
    }
    if let Some((role, rule_id)) = heading_rule(heading) {
        roles.insert(
            role,
            RoleEvidence::HeadingRule {
                rule_id: rule_id.to_string(),
            },
        );
    }
    if is_codeowners_path(path) {
        roles.insert(
            KnowledgeRole::OwnershipGovernance,
            RoleEvidence::DeclaredSpan,
        );
    }
    if is_license_path(path) {
        roles.insert(
            KnowledgeRole::OwnershipGovernance,
            RoleEvidence::PathConvention {
                rule_id: "role.path.license.v1".to_string(),
            },
        );
    }
    if roles.is_empty() {
        roles.insert(
            KnowledgeRole::Other,
            RoleEvidence::PathConvention {
                rule_id: "role.path.other.v1".to_string(),
            },
        );
    }
    roles.into_iter().collect()
}

fn heading_rule(heading: Option<&str>) -> Option<(KnowledgeRole, &'static str)> {
    match heading?.trim().to_ascii_lowercase().as_str() {
        "architecture" | "architectural design" | "system design" => {
            Some((KnowledgeRole::Architecture, "role.heading.architecture.v1"))
        }
        "ownership" | "governance" | "ownership / governance" | "ownership and governance" => {
            Some((
                KnowledgeRole::OwnershipGovernance,
                "role.heading.ownership-governance.v1",
            ))
        }
        "decision" | "decisions" | "invariant" | "invariants" | "decisions / invariants" => Some((
            KnowledgeRole::DecisionInvariant,
            "role.heading.decision-invariant.v1",
        )),
        "schema" | "schemas" | "contract" | "contracts" | "schemas / contracts" => Some((
            KnowledgeRole::SchemaContract,
            "role.heading.schema-contract.v1",
        )),
        "operations" | "runbook" | "runbooks" => {
            Some((KnowledgeRole::Operations, "role.heading.operations.v1"))
        }
        "testing" | "security" | "testing / security" | "testing and security" => Some((
            KnowledgeRole::TestingSecurity,
            "role.heading.testing-security.v1",
        )),
        "plan" | "plans" | "handoff" | "handoffs" | "plans / handoffs" => {
            Some((KnowledgeRole::PlanHandoff, "role.heading.plan-handoff.v1"))
        }
        _ => None,
    }
}

fn path_convention_roles(path: &str) -> Vec<(KnowledgeRole, &'static str)> {
    let mut roles = BTreeMap::new();
    for token in path
        .replace('\\', "/")
        .split('/')
        .flat_map(|component| component.split(|character: char| !character.is_ascii_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
    {
        let assignment = match token.as_str() {
            "architecture" | "architectures" | "design" | "designs" => {
                Some((KnowledgeRole::Architecture, "role.path.architecture.v1"))
            }
            "ownership" | "owners" | "governance" | "codeowners" => Some((
                KnowledgeRole::OwnershipGovernance,
                "role.path.ownership-governance.v1",
            )),
            "decision" | "decisions" | "adr" | "adrs" | "invariant" | "invariants" => Some((
                KnowledgeRole::DecisionInvariant,
                "role.path.decision-invariant.v1",
            )),
            "schema" | "schemas" | "contract" | "contracts" | "protocol" | "protocols" => Some((
                KnowledgeRole::SchemaContract,
                "role.path.schema-contract.v1",
            )),
            "operation" | "operations" | "ops" | "runbook" | "runbooks" => {
                Some((KnowledgeRole::Operations, "role.path.operations.v1"))
            }
            "test" | "tests" | "testing" | "security" | "threat" | "threats" => Some((
                KnowledgeRole::TestingSecurity,
                "role.path.testing-security.v1",
            )),
            "plan" | "plans" | "handoff" | "handoffs" | "tasks" | "roadmap" => {
                Some((KnowledgeRole::PlanHandoff, "role.path.plan-handoff.v1"))
            }
            _ => None,
        };
        if let Some((role, rule_id)) = assignment {
            roles.entry(role).or_insert(rule_id);
        }
    }
    roles.into_iter().collect()
}

fn targets_by_path(live: &LiveIndex) -> BTreeMap<String, IndexTargets> {
    live.manifest_entries
        .iter()
        .filter_map(|entry| {
            let FileDisposition::Indexed { targets, .. } = entry.disposition else {
                return None;
            };
            let path = entry
                .path
                .normalized_utf8
                .as_deref()
                .unwrap_or(entry.path.public_id.as_str());
            Some((path.to_string(), targets))
        })
        .collect()
}

fn targets_for_file(
    path: &str,
    file: &IndexedFile,
    targets: &BTreeMap<String, IndexTargets>,
) -> IndexTargets {
    targets
        .get(path)
        .copied()
        .unwrap_or_else(|| IndexTargets::for_path(path, Some(&file.language)))
}

fn symbol_index(
    live: &LiveIndex,
    source: &SourceIdentity,
    content_generation: u64,
    targets: &BTreeMap<String, IndexTargets>,
    referenced_symbols: &BTreeSet<String>,
) -> BTreeMap<String, Vec<CodeAnchor>> {
    let mut by_name: BTreeMap<String, Vec<CodeAnchor>> = BTreeMap::new();
    if referenced_symbols.is_empty() {
        return by_name;
    }
    let mut paths: Vec<_> = live.files.keys().collect();
    paths.sort_unstable();
    for path in paths {
        let file = &live.files[path];
        if !targets_for_file(path, file, targets).includes_code() {
            continue;
        }
        for symbol in &file.symbols {
            if symbol.kind == SymbolKind::Section || !referenced_symbols.contains(&symbol.name) {
                continue;
            }
            let symbol_line_range = line_range(
                &file.content,
                symbol.byte_range.0 as usize..symbol.byte_range.1 as usize,
            );
            let anchor = CodeAnchor {
                source: source.clone(),
                content_generation,
                id: CodeAnchorId::Symbol {
                    symbol: SymbolId {
                        path: path.clone(),
                        name: symbol.name.clone(),
                        kind: symbol.kind,
                    },
                    start_line: symbol_line_range.start,
                },
                content_hash: file.content_hash.clone(),
                line_range: symbol_line_range,
            };
            by_name.entry(symbol.name.clone()).or_default().push(anchor);
        }
    }
    for anchors in by_name.values_mut() {
        anchors.sort_by(|left, right| left.id.cmp(&right.id));
    }
    by_name
}

fn extract_candidates(path: &str, file: &IndexedFile, text: &str) -> Vec<BridgeCandidate> {
    if is_codeowners_path(path) {
        return extract_ownership_selectors(text);
    }

    let mut candidates = Vec::new();
    let mut claimed_ranges = external_url_ranges(text);
    if file.language == LanguageId::Markdown {
        for (range, selector) in extract_markdown_links(text) {
            // A Markdown destination is a syntactically closed candidate lane.
            // Claim it even when it is external so the generic path-token scan
            // cannot reinterpret a URL suffix as a repository-local path.
            claimed_ranges.push(range.clone());
            if is_external_link(&selector) || selector.starts_with('#') {
                continue;
            }
            candidates.push(BridgeCandidate {
                range,
                evidence_kind: BridgeEvidenceKind::RepositoryLink,
                selector: CandidateSelector::Path(selector),
            });
        }
    }

    for (range, selector) in extract_structured_paths(text) {
        if overlaps_any(&range, &claimed_ranges) {
            continue;
        }
        claimed_ranges.push(range.clone());
        candidates.push(BridgeCandidate {
            range,
            evidence_kind: BridgeEvidenceKind::SupportedStructuredValue {
                rule_id: STRUCTURED_CODE_PATH_RULE_ID.to_string(),
            },
            selector: CandidateSelector::Path(selector),
        });
    }

    for (range, selector) in extract_inline_code(text) {
        if overlaps_any(&range, &claimed_ranges) {
            continue;
        }
        claimed_ranges.push(range.clone());
        let (evidence_kind, selector) = if looks_like_path(&selector) {
            (
                BridgeEvidenceKind::ExactPathToken,
                CandidateSelector::Path(selector),
            )
        } else {
            (
                BridgeEvidenceKind::ExactCodeSpanSymbol,
                CandidateSelector::Symbol(normalize_symbol_selector(&selector)),
            )
        };
        candidates.push(BridgeCandidate {
            range,
            evidence_kind,
            selector,
        });
    }

    for (range, selector) in extract_path_tokens(text) {
        if overlaps_any(&range, &claimed_ranges) || selector.starts_with("//") {
            continue;
        }
        candidates.push(BridgeCandidate {
            range,
            evidence_kind: BridgeEvidenceKind::ExactPathToken,
            selector: CandidateSelector::Path(selector),
        });
    }
    candidates
}

fn extract_markdown_links(text: &str) -> Vec<(Range<usize>, String)> {
    let bytes = text.as_bytes();
    let mut links = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(open_label) = bytes[cursor..].iter().position(|byte| *byte == b'[') else {
            break;
        };
        let open_label = cursor + open_label;
        if open_label > 0 && bytes[open_label - 1] == b'!' {
            cursor = open_label + 1;
            continue;
        }
        let Some(close_label) = bytes[open_label + 1..]
            .iter()
            .position(|byte| *byte == b']')
            .map(|offset| open_label + 1 + offset)
        else {
            break;
        };
        if bytes.get(close_label + 1) != Some(&b'(') {
            cursor = close_label + 1;
            continue;
        }
        let destination_start = close_label + 2;
        let Some(close_destination) = bytes[destination_start..]
            .iter()
            .position(|byte| *byte == b')')
            .map(|offset| destination_start + offset)
        else {
            break;
        };
        let raw = text[destination_start..close_destination].trim();
        let selector = raw
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
            .unwrap_or(raw)
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default();
        if !selector.is_empty() {
            let trim_offset = text[destination_start..close_destination]
                .find(selector)
                .unwrap_or(0);
            let start = destination_start + trim_offset;
            links.push((start..start + selector.len(), selector.to_string()));
        }
        cursor = close_destination + 1;
    }
    links
}

fn structured_path_regex() -> &'static regex::Regex {
    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        regex::Regex::new(
            r#"(?mi)[\"']?(?:code_path|source_path|entrypoint)[\"']?\s*[:=]\s*[\"']([^\"'\r\n]+)[\"']"#,
        )
        .expect("bridge structured-path regex is static and valid")
    })
}

fn extract_structured_paths(text: &str) -> Vec<(Range<usize>, String)> {
    structured_path_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let value = captures.get(1)?;
            Some((value.range(), value.as_str().trim().to_string()))
        })
        .collect()
}

fn extract_inline_code(text: &str) -> Vec<(Range<usize>, String)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(open) = bytes[cursor..].iter().position(|byte| *byte == b'`') else {
            break;
        };
        let open = cursor + open;
        if bytes.get(open + 1) == Some(&b'`') || (open > 0 && bytes[open - 1] == b'`') {
            cursor = open + 1;
            continue;
        }
        let Some(close) = bytes[open + 1..]
            .iter()
            .position(|byte| *byte == b'`')
            .map(|offset| open + 1 + offset)
        else {
            break;
        };
        if bytes.get(close + 1) == Some(&b'`') {
            cursor = close + 1;
            continue;
        }
        let raw = &text[open + 1..close];
        let selector = raw.trim();
        if is_explicit_code_selector(selector) {
            let trim_offset = raw.find(selector).unwrap_or(0);
            let start = open + 1 + trim_offset;
            spans.push((start..start + selector.len(), selector.to_string()));
        }
        cursor = close + 1;
    }
    spans
}

fn is_explicit_code_selector(selector: &str) -> bool {
    !selector.is_empty()
        && !selector.chars().any(char::is_whitespace)
        && selector.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b':' | b'.' | b'#' | b'/' | b'\\' | b'-' | b'(' | b')'
                )
        })
}

fn normalize_symbol_selector(selector: &str) -> String {
    selector
        .strip_suffix("()")
        .unwrap_or(selector)
        .rsplit("::")
        .next()
        .unwrap_or(selector)
        .to_string()
}

fn path_token_regex() -> &'static regex::Regex {
    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        regex::Regex::new(r"(?:\.\.?/|/)?[A-Za-z0-9_.@+-]+(?:/[A-Za-z0-9_.@+-]+)+")
            .expect("bridge path-token regex is static and valid")
    })
}

fn extract_path_tokens(text: &str) -> Vec<(Range<usize>, String)> {
    path_token_regex()
        .find_iter(text)
        .filter_map(|matched| {
            let selector = matched.as_str();
            let final_component = selector.trim_end_matches('/').rsplit('/').next()?;
            (final_component.contains('.')
                || selector.starts_with("./")
                || selector.starts_with("../"))
            .then(|| (matched.range(), selector.to_string()))
        })
        .collect()
}

fn extract_ownership_selectors(text: &str) -> Vec<BridgeCandidate> {
    let mut candidates = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let without_newline = line.trim_end_matches(['\r', '\n']);
        let trimmed = without_newline.trim_start();
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            let selector = trimmed.split_ascii_whitespace().next().unwrap_or_default();
            let has_owner = trimmed
                .split_ascii_whitespace()
                .skip(1)
                .any(|owner| owner.starts_with('@'));
            if !selector.is_empty() && has_owner {
                let start = offset + without_newline.find(selector).unwrap_or(0);
                candidates.push(BridgeCandidate {
                    range: start..start + selector.len(),
                    evidence_kind: BridgeEvidenceKind::DeclaredOwnershipSelector,
                    selector: CandidateSelector::Ownership(selector.to_string()),
                });
            }
        }
        offset += line.len();
    }
    candidates
}

fn overlaps_any(range: &Range<usize>, claimed: &[Range<usize>]) -> bool {
    claimed
        .iter()
        .any(|other| range.start < other.end && other.start < range.end)
}

fn looks_like_path(selector: &str) -> bool {
    selector.contains('/') || selector.contains('\\')
}

fn is_external_link(selector: &str) -> bool {
    let lower = selector.to_ascii_lowercase();
    lower.contains("://")
        || lower.starts_with("mailto:")
        || lower.starts_with("data:")
        || lower.starts_with("javascript:")
        || lower.starts_with("//")
}

fn external_url_ranges(text: &str) -> Vec<Range<usize>> {
    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| {
            regex::Regex::new(r"(?i)\b[a-z][a-z0-9+.-]*://[^\s<>()\[\]{}]+")
                .expect("external URL regex is static and valid")
        })
        .find_iter(text)
        .map(|matched| matched.range())
        .collect()
}

fn is_codeowners_path(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("CODEOWNERS"))
}

/// Exact-filename match for a legal-provenance file / `<name>.<ext>` (any case,
/// e.g. `license.md`, `LICENSE.txt`, `COPYING.LESSER`), mirroring
/// [`is_codeowners_path`]. Such a file duplicated across paths (root +
/// `npm/LICENSE`, or per-subpackage in a vendored tree) is duplicated by
/// packaging/legal necessity, not content drift — deleting a copy can break
/// `npm publish`'s license packaging, a vendored dependency's own license terms,
/// or Apache-2.0 §4(d)'s requirement that `NOTICE` be redistributed.
///
/// The list covers the filenames the ecosystem actually ships: the dual-license
/// pair Rust crates use (`LICENSE-APACHE` + `LICENSE-MIT`), GNU's `COPYING`,
/// Apache's `NOTICE`, and `UNLICENSE`. `LICENSE` alone left every one of those
/// unprotected, so a duplicated `LICENSE-APACHE` was still offered for deletion.
///
/// This is an exact-filename check, not a prefix/substring/token match: a doc
/// that merely discusses licensing (e.g. `docs/license-notes.md`) must not match.
fn is_license_path(path: &str) -> bool {
    let Some(stem) = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split('.').next())
    else {
        return false;
    };
    [
        "LICENSE",
        "LICENCE",
        "LICENSE-APACHE",
        "LICENSE-MIT",
        "LICENCE-APACHE",
        "LICENCE-MIT",
        "COPYING",
        "NOTICE",
        "UNLICENSE",
    ]
    .iter()
    .any(|candidate| stem.eq_ignore_ascii_case(candidate))
}

fn resolve_repo_path<'a>(
    evidence_path: &str,
    selector: &str,
    existing_paths: impl Iterator<Item = &'a str>,
) -> Option<String> {
    if is_external_link(selector) {
        return None;
    }
    let selector = selector.split(['#', '?']).next().unwrap_or_default().trim();
    if selector.is_empty() {
        return None;
    }
    let existing: BTreeSet<&str> = existing_paths.collect();
    let root_relative = selector.trim_start_matches('/');
    if selector.starts_with('/') {
        return normalize_repo_path(root_relative);
    }
    if !selector.starts_with("./") && !selector.starts_with("../") {
        let direct = normalize_repo_path(root_relative)?;
        if existing.contains(direct.as_str()) {
            return Some(direct);
        }
        let parent = evidence_path.rsplit_once('/').map(|(parent, _)| parent);
        if let Some(parent) = parent {
            let relative = normalize_repo_path(&format!("{parent}/{selector}"))?;
            if existing.contains(relative.as_str()) {
                return Some(relative);
            }
        }
        return Some(direct);
    }
    let parent = evidence_path
        .rsplit_once('/')
        .map_or("", |(parent, _)| parent);
    normalize_repo_path(&format!("{parent}/{selector}"))
}

fn normalize_repo_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let mut components = Vec::new();
    for component in normalized.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            other if other.contains(':') || other.contains('\0') => return None,
            other => components.push(other),
        }
    }
    (!components.is_empty()).then(|| components.join("/"))
}

fn ownership_match_count(selector: &str, code_paths: &[&str]) -> u64 {
    let mut pattern = selector.trim_start_matches('/').replace('\\', "/");
    if pattern.ends_with('/') {
        pattern.push_str("**");
    }
    if !pattern.contains('/') {
        pattern = format!("**/{pattern}");
    }
    let Ok(glob) = globset::GlobBuilder::new(&pattern)
        .literal_separator(true)
        .backslash_escape(false)
        .build()
    else {
        return 0;
    };
    let matcher = glob.compile_matcher();
    u64::try_from(
        code_paths
            .iter()
            .filter(|path| matcher.is_match(path))
            .count(),
    )
    .unwrap_or(u64::MAX)
}

fn knowledge_anchor(
    source: &SourceIdentity,
    content_generation: u64,
    path: &str,
    file: &IndexedFile,
    range: Range<usize>,
) -> KnowledgeAnchor {
    let start = u32::try_from(range.start).unwrap_or(u32::MAX);
    let end = u32::try_from(range.end).unwrap_or(u32::MAX);
    KnowledgeAnchor {
        id: KnowledgeAnchorId {
            path: path.to_string(),
            content_hash: file.content_hash.clone(),
            start_byte: start,
        },
        source: source.clone(),
        content_generation,
        path: path.to_string(),
        content_hash: file.content_hash.clone(),
        byte_range: start..end,
        line_range: line_range(&file.content, range),
    }
}

fn line_range(content: &[u8], range: Range<usize>) -> Range<u32> {
    let start_byte = range.start.min(content.len());
    let end_byte = range.end.min(content.len());
    let start = content
        .get(..start_byte)
        .unwrap_or_default()
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        .saturating_add(1);
    let end = if end_byte <= start_byte {
        start
    } else {
        let newline_count = content[..end_byte]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        if content.get(end_byte - 1) == Some(&b'\n') {
            newline_count.saturating_add(1)
        } else {
            newline_count.saturating_add(2)
        }
    };
    u32::try_from(start).unwrap_or(u32::MAX)..u32::try_from(end).unwrap_or(u32::MAX)
}

fn file_code_anchor(
    source: &SourceIdentity,
    content_generation: u64,
    path: &str,
    file: &IndexedFile,
) -> CodeAnchor {
    CodeAnchor {
        source: source.clone(),
        content_generation,
        id: CodeAnchorId::File {
            path: path.to_string(),
        },
        content_hash: file.content_hash.clone(),
        line_range: line_range(&file.content, 0..file.content.len()),
    }
}

#[allow(clippy::too_many_arguments)]
fn push_forward_link(
    forward: &mut Vec<KnowledgeCodeLink>,
    budget: &mut BudgetState<'_>,
    evidence: KnowledgeAnchor,
    evidence_kind: BridgeEvidenceKind,
    resolution: BridgeResolution,
    ordinal: usize,
    selector: &str,
) {
    let id = stable_link_id(&evidence, &evidence_kind, ordinal, selector);
    let link = KnowledgeCodeLink {
        id,
        evidence,
        evidence_kind,
        resolution,
    };
    if budget.admit_metadata(link_metadata_bytes(&link)) {
        forward.push(link);
    }
}

fn stable_link_id(
    evidence: &KnowledgeAnchor,
    evidence_kind: &BridgeEvidenceKind,
    ordinal: usize,
    selector: &str,
) -> KnowledgeCodeLinkId {
    let source = serde_json::to_vec(&evidence.source)
        .expect("SourceIdentity serialization is infallible for its closed data model");
    let kind = match evidence_kind {
        BridgeEvidenceKind::RepositoryLink => "repository-link",
        BridgeEvidenceKind::ExactPathToken => "exact-path-token",
        BridgeEvidenceKind::ExactCodeSpanSymbol => "exact-code-span-symbol",
        BridgeEvidenceKind::DeclaredOwnershipSelector => "declared-ownership-selector",
        BridgeEvidenceKind::SupportedStructuredValue { rule_id } => rule_id,
    };
    let mut bytes = Vec::with_capacity(
        source.len()
            + evidence.id.path.len()
            + evidence.id.content_hash.len()
            + selector.len()
            + kind.len()
            + 32,
    );
    bytes.extend_from_slice(&source);
    bytes.extend_from_slice(evidence.id.path.as_bytes());
    bytes.extend_from_slice(evidence.id.content_hash.as_bytes());
    bytes.extend_from_slice(&evidence.id.start_byte.to_le_bytes());
    bytes.extend_from_slice(kind.as_bytes());
    bytes.extend_from_slice(&ordinal.to_le_bytes());
    bytes.extend_from_slice(selector.as_bytes());
    KnowledgeCodeLinkId(crate::hash::digest_hex(&bytes))
}

fn link_metadata_bytes(link: &KnowledgeCodeLink) -> usize {
    let mut bytes =
        link.id.0.len() + link.evidence.path.len() + link.evidence.content_hash.len() + 32;
    match &link.resolution {
        BridgeResolution::ResolvedExact(anchor) => {
            bytes = bytes.saturating_add(code_anchor_metadata_bytes(anchor));
        }
        BridgeResolution::ResolvedDeclaredSet {
            selector_anchor, ..
        } => {
            bytes = bytes
                .saturating_add(selector_anchor.path.len())
                .saturating_add(selector_anchor.content_hash.len());
        }
        BridgeResolution::Ambiguous {
            bounded_samples, ..
        } => {
            for anchor in bounded_samples {
                bytes = bytes.saturating_add(code_anchor_metadata_bytes(anchor));
            }
        }
        BridgeResolution::Missing => {}
    }
    bytes
}

fn knowledge_card_metadata_bytes(card: &KnowledgeCard) -> usize {
    let mut bytes = card
        .anchor
        .path
        .len()
        .saturating_add(card.anchor.content_hash.len())
        .saturating_add(32);
    for (_, evidence) in &card.roles {
        bytes = bytes.saturating_add(std::mem::size_of::<KnowledgeRole>());
        // Roles no longer carry an anchor (it was always a clone of
        // `card.anchor`, already counted above), so only the rule_id is
        // per-role weight now.
        match evidence {
            RoleEvidence::DeclaredSpan => {
                bytes = bytes.saturating_add(32);
            }
            RoleEvidence::HeadingRule { rule_id } | RoleEvidence::PathConvention { rule_id } => {
                bytes = bytes.saturating_add(rule_id.len()).saturating_add(32);
            }
        }
    }
    bytes
}

fn code_anchor_metadata_bytes(anchor: &CodeAnchor) -> usize {
    let id = match &anchor.id {
        CodeAnchorId::File { path } => path.len(),
        CodeAnchorId::Symbol { symbol, .. } => {
            symbol.path.len() + symbol.name.len() + std::mem::size_of::<SymbolKind>()
        }
    };
    id.saturating_add(anchor.content_hash.len())
}

fn knowledge_link_metadata_bytes(link: &KnowledgeKnowledgeLink) -> usize {
    let target = match &link.resolution {
        KnowledgeLinkResolution::ResolvedExact(anchor) => {
            anchor.path.len() + anchor.content_hash.len()
        }
        KnowledgeLinkResolution::Ambiguous {
            bounded_samples, ..
        } => bounded_samples
            .iter()
            .map(|anchor| anchor.path.len() + anchor.content_hash.len())
            .sum(),
        KnowledgeLinkResolution::Missing => 0,
    };
    link.evidence
        .path
        .len()
        .saturating_add(link.evidence.content_hash.len())
        .saturating_add(target)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;
    use std::sync::{Arc, Barrier};
    use std::thread;

    use tempfile::TempDir;

    use super::*;
    use crate::domain::SourceId;
    use crate::live_index::{LiveIndex, SharedIndex};

    fn write(root: &Path, path: &str, content: &str) {
        let absolute = root.join(path);
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(absolute, content).unwrap();
    }

    fn fixture(files: &[(&str, &str)]) -> (TempDir, SharedIndex) {
        let root = TempDir::new().unwrap();
        for (path, content) in files {
            write(root.path(), path, content);
        }
        let shared = LiveIndex::load(root.path()).unwrap();
        (root, shared)
    }

    fn bridge(shared: &SharedIndex) -> Arc<KnowledgeBridge> {
        Arc::clone(&shared.published_generation().bridge)
    }

    fn build_with_limits(shared: &SharedIndex, limits: BridgeLimits) -> KnowledgeBridge {
        let published = shared.published_generation();
        build_knowledge_bridge(
            &published.live,
            published.source.as_deref().unwrap(),
            published.content_generation,
            &limits,
        )
    }

    fn breach_kinds(bridge: &KnowledgeBridge) -> BTreeSet<DerivedLimitKind> {
        match &bridge.coverage {
            DerivedCoverage::Complete => BTreeSet::new(),
            DerivedCoverage::Truncated { breaches } => {
                breaches.iter().map(|breach| breach.kind).collect()
            }
        }
    }

    #[test]
    fn exact_paths_and_unique_code_spans_resolve_bidirectionally_without_bare_or_external_links() {
        let (_root, shared) = fixture(&[
            ("src/lib.rs", "pub fn launch() {}\n"),
            (
                "docs/guide.md",
                "[library](../src/lib.rs)\n\n`launch` is exact. launch is bare.\n\n[web](https://example.com/src/lib.rs)\nBare https://example.com/src/lib.rs stays external.\n",
            ),
        ]);

        let bridge = bridge(&shared);
        assert_eq!(bridge.forward.len(), 2);
        assert_eq!(
            bridge.reverse_exact.values().map(Vec::len).sum::<usize>(),
            2
        );
        assert!(
            bridge
                .forward
                .iter()
                .all(|link| matches!(link.resolution, BridgeResolution::ResolvedExact(_)))
        );
        let kinds: BTreeSet<_> = bridge
            .forward
            .iter()
            .map(|link| link.evidence_kind.clone())
            .collect();
        assert_eq!(
            kinds,
            BTreeSet::from([
                BridgeEvidenceKind::RepositoryLink,
                BridgeEvidenceKind::ExactCodeSpanSymbol,
            ])
        );
        let line_ranges: Vec<_> = bridge
            .forward
            .iter()
            .map(|link| link.evidence.line_range.clone())
            .collect();
        assert_eq!(line_ranges, vec![1..2, 3..4]);
        for link in &bridge.forward {
            let BridgeResolution::ResolvedExact(anchor) = &link.resolution else {
                unreachable!("exact fixture resolutions were asserted above");
            };
            assert_eq!(anchor.line_range, 1..2);
            if let CodeAnchorId::Symbol { start_line, .. } = &anchor.id {
                assert_eq!(*start_line, 1);
            }
        }
    }

    #[test]
    fn same_name_and_kind_at_distinct_spans_remain_ambiguous() {
        let (_root, shared) = fixture(&[
            ("src/a.rs", "pub fn shared() {}\n"),
            ("src/b.rs", "pub fn shared() {}\n"),
            ("docs/guide.md", "Call `shared`.\n"),
        ]);

        let bridge = bridge(&shared);
        let link = bridge.forward.first().unwrap();
        match &link.resolution {
            BridgeResolution::Ambiguous {
                candidate_count,
                bounded_samples,
            } => {
                assert_eq!(*candidate_count, 2);
                assert_eq!(bounded_samples.len(), 2);
                assert_ne!(bounded_samples[0].id, bounded_samples[1].id);
            }
            other => panic!("expected typed ambiguity, got {other:?}"),
        }
        assert!(bridge.reverse_exact.is_empty());
    }

    #[test]
    fn missing_structured_and_declared_ownership_candidates_retain_typed_provenance() {
        let (_root, shared) = fixture(&[
            ("src/lib.rs", "pub fn present() {}\n"),
            (
                "docs/guide.md",
                "[missing](../src/missing.rs) and `gone` remain uncertain.\n",
            ),
            ("config/bridge.toml", "code_path = \"src/lib.rs\"\n"),
            (".github/CODEOWNERS", "/src/*.rs @runtime-team\n"),
        ]);

        let bridge = bridge(&shared);
        assert_eq!(
            bridge
                .forward
                .iter()
                .filter(|link| matches!(link.resolution, BridgeResolution::Missing))
                .count(),
            2
        );
        assert!(bridge.forward.iter().any(|link| matches!(
            (&link.evidence_kind, &link.resolution),
            (
                BridgeEvidenceKind::SupportedStructuredValue { rule_id },
                BridgeResolution::ResolvedExact(_)
            ) if rule_id == STRUCTURED_CODE_PATH_RULE_ID
        )));
        let ownership = bridge
            .forward
            .iter()
            .find(|link| {
                matches!(
                    link.evidence_kind,
                    BridgeEvidenceKind::DeclaredOwnershipSelector
                )
            })
            .unwrap();
        assert!(matches!(
            ownership.resolution,
            BridgeResolution::ResolvedDeclaredSet {
                matched_count: 1,
                ..
            }
        ));
        let source = shared
            .published_generation()
            .source
            .as_ref()
            .unwrap()
            .clone();
        assert!(
            bridge
                .forward
                .iter()
                .all(|link| link.evidence.source == *source)
        );
    }

    #[test]
    fn create_change_rename_and_remove_repair_forward_and_reverse_links_atomically() {
        let (root, shared) = fixture(&[("docs/guide.md", "[runtime](../src/runtime.rs)\n")]);
        assert!(matches!(
            bridge(&shared).forward[0].resolution,
            BridgeResolution::Missing
        ));

        write(root.path(), "src/runtime.rs", "pub fn runtime() {}\n");
        shared.reload(root.path()).unwrap();
        let created = bridge(&shared);
        assert!(matches!(
            created.forward[0].resolution,
            BridgeResolution::ResolvedExact(_)
        ));
        assert_eq!(
            created.reverse_exact.values().map(Vec::len).sum::<usize>(),
            1
        );

        fs::rename(
            root.path().join("src/runtime.rs"),
            root.path().join("src/engine.rs"),
        )
        .unwrap();
        shared.reload(root.path()).unwrap();
        assert!(matches!(
            bridge(&shared).forward[0].resolution,
            BridgeResolution::Missing
        ));

        write(
            root.path(),
            "docs/guide.md",
            "[runtime](../src/engine.rs)\n",
        );
        shared.reload(root.path()).unwrap();
        assert!(matches!(
            bridge(&shared).forward[0].resolution,
            BridgeResolution::ResolvedExact(_)
        ));

        fs::remove_file(root.path().join("src/engine.rs")).unwrap();
        shared.reload(root.path()).unwrap();
        let removed = bridge(&shared);
        assert!(matches!(
            removed.forward[0].resolution,
            BridgeResolution::Missing
        ));
        assert!(removed.reverse_exact.is_empty());
    }

    #[test]
    fn missing_knowledge_link_stays_in_the_knowledge_lane_and_repairs_bidirectionally() {
        let (root, shared) = fixture(&[("docs/source.md", "[target](target.md)\n")]);
        let missing = bridge(&shared);
        assert!(missing.forward.is_empty());
        assert!(matches!(
            missing.knowledge_links[0].resolution,
            KnowledgeLinkResolution::Missing
        ));

        write(root.path(), "docs/target.md", "# Target\n");
        shared.reload(root.path()).unwrap();
        let created = bridge(&shared);
        assert!(matches!(
            created.knowledge_links[0].resolution,
            KnowledgeLinkResolution::ResolvedExact(_)
        ));
        assert_eq!(
            created
                .reverse_knowledge
                .values()
                .map(Vec::len)
                .sum::<usize>(),
            1
        );

        fs::remove_file(root.path().join("docs/target.md")).unwrap();
        shared.reload(root.path()).unwrap();
        let removed = bridge(&shared);
        assert!(matches!(
            removed.knowledge_links[0].resolution,
            KnowledgeLinkResolution::Missing
        ));
        assert!(removed.reverse_knowledge.is_empty());
    }

    #[test]
    fn bridge_build_from_an_old_content_generation_is_rejected() {
        let (root, shared) = fixture(&[
            ("src/lib.rs", "pub fn old_name() {}\n"),
            ("docs/guide.md", "Call `old_name`.\n"),
        ]);
        let prepared = shared.prepare_bridge_rebuild();
        let pinned = shared.published_generation();

        write(root.path(), "src/lib.rs", "pub fn new_name() {}\n");
        write(root.path(), "docs/guide.md", "Call `new_name`.\n");
        shared.reload(root.path()).unwrap();
        let current = shared.publication_fence();

        assert!(!shared.publish_prepared_bridge(prepared));
        assert_eq!(shared.publication_fence(), current);
        assert!(
            String::from_utf8_lossy(
                &pinned
                    .live
                    .capture_file_content_view("src/lib.rs")
                    .expect("pinned file")
                    .content
            )
            .contains("old_name")
        );
        assert_eq!(
            pinned.bridge.forward[0].evidence.content_generation,
            pinned.content_generation
        );
        assert!(matches!(
            bridge(&shared).forward[0].resolution,
            BridgeResolution::ResolvedExact(_)
        ));
    }

    #[test]
    fn independent_bridge_budgets_degrade_only_derived_coverage() {
        let (_root, shared) = fixture(&[
            ("src/a.rs", "pub fn shared() {}\n"),
            ("src/b.rs", "pub fn shared() {}\n"),
            (
                "docs/guide.md",
                "[code](../src/a.rs) and ambiguous `shared`, then `shared` again.\n",
            ),
            (".github/CODEOWNERS", "/src/*.rs @runtime-team\n"),
        ]);

        let card_limited = build_with_limits(
            &shared,
            BridgeLimits {
                max_cards: 0,
                ..BridgeLimits::default()
            },
        );
        assert!(card_limited.cards.is_empty());
        assert!(breach_kinds(&card_limited).contains(&DerivedLimitKind::Cards));

        let candidate_limited = build_with_limits(
            &shared,
            BridgeLimits {
                max_candidates: 0,
                ..BridgeLimits::default()
            },
        );
        assert!(breach_kinds(&candidate_limited).contains(&DerivedLimitKind::BridgeLinks));

        let selector_limited = build_with_limits(
            &shared,
            BridgeLimits {
                max_ownership_selectors: 0,
                ..BridgeLimits::default()
            },
        );
        assert!(breach_kinds(&selector_limited).contains(&DerivedLimitKind::OwnershipSelectors));

        let sample_limited = build_with_limits(
            &shared,
            BridgeLimits {
                max_ambiguous_samples: 1,
                ..BridgeLimits::default()
            },
        );
        assert!(breach_kinds(&sample_limited).contains(&DerivedLimitKind::AmbiguousSamples));
        assert_eq!(
            sample_limited
                .forward
                .iter()
                .filter_map(|link| match &link.resolution {
                    BridgeResolution::Ambiguous {
                        bounded_samples, ..
                    } => Some(bounded_samples.len()),
                    _ => None,
                })
                .sum::<usize>(),
            1,
            "the sample ceiling is global to the immutable bridge, not per link"
        );

        let metadata_limited = build_with_limits(
            &shared,
            BridgeLimits {
                max_metadata_bytes: 0,
                ..BridgeLimits::default()
            },
        );
        assert!(breach_kinds(&metadata_limited).contains(&DerivedLimitKind::MetadataBytes));
        assert!(
            shared
                .published_generation()
                .live
                .capture_file_content_view("docs/guide.md")
                .is_some()
        );
    }

    #[test]
    fn role_cards_use_only_exact_versioned_heading_path_and_declared_evidence() {
        let (_root, shared) = fixture(&[
            (
                "docs/design/guide.md",
                "# Architecture\nSystem shape.\n## Operations\nRun it.\n## Contributors\nAlice helped.\n",
            ),
            ("docs/unclassified.md", "# Contributors\nBob helped.\n"),
            (".github/CODEOWNERS", "/src/*.rs @runtime-team\n"),
        ]);

        let bridge = bridge(&shared);
        let architecture = bridge
            .cards
            .iter()
            .find(|card| {
                card.anchor.path == "docs/design/guide.md" && card.anchor.line_range.start == 1
            })
            .expect("architecture section card");
        assert!(architecture.roles.iter().any(|(role, evidence)| {
            *role == KnowledgeRole::Architecture
                && matches!(
                    evidence,
                    RoleEvidence::HeadingRule { rule_id }
                        if rule_id == "role.heading.architecture.v1"
                )
        }));

        let operations = bridge
            .cards
            .iter()
            .find(|card| {
                card.anchor.path == "docs/design/guide.md" && card.anchor.line_range.start == 3
            })
            .expect("operations section card");
        assert!(operations.roles.iter().any(|(role, evidence)| {
            *role == KnowledgeRole::Operations
                && matches!(
                    evidence,
                    RoleEvidence::HeadingRule { rule_id }
                        if rule_id == "role.heading.operations.v1"
                )
        }));
        assert!(operations.roles.iter().any(|(role, evidence)| {
            *role == KnowledgeRole::Architecture
                && matches!(
                    evidence,
                    RoleEvidence::PathConvention { rule_id }
                        if rule_id == "role.path.architecture.v1"
                )
        }));

        let contributors = bridge
            .cards
            .iter()
            .filter(|card| {
                card.anchor.path == "docs/design/guide.md"
                    || card.anchor.path == "docs/unclassified.md"
            })
            .filter(|card| {
                card.anchor.line_range.start == 5 || card.anchor.path.ends_with("unclassified.md")
            });
        for card in contributors {
            assert!(
                card.roles
                    .iter()
                    .all(|(role, _)| *role != KnowledgeRole::OwnershipGovernance)
            );
        }

        let unclassified = bridge
            .cards
            .iter()
            .find(|card| card.anchor.path == "docs/unclassified.md")
            .expect("unclassified section card");
        assert!(unclassified.roles.iter().any(|(role, evidence)| {
            *role == KnowledgeRole::Other
                && matches!(
                    evidence,
                    RoleEvidence::PathConvention { rule_id }
                        if rule_id == "role.path.other.v1"
                )
        }));

        let declared_owner = bridge
            .cards
            .iter()
            .find(|card| card.anchor.path == ".github/CODEOWNERS")
            .expect("declared ownership card");
        assert!(declared_owner.roles.iter().any(|(role, evidence)| {
            *role == KnowledgeRole::OwnershipGovernance
                && matches!(evidence, RoleEvidence::DeclaredSpan)
        }));
    }

    /// A `LICENSE` file that is byte-identical across paths (root + `npm/LICENSE`,
    /// or a vendored subpackage) is duplicated by PACKAGING NECESSITY, not drift —
    /// `npm/package.json`'s `files` array requires a co-located `LICENSE` in the
    /// published tarball. Without a role here, `review_knowledge`'s duplicate
    /// detector (`effective_action`/`effective_confidence` in
    /// `knowledge_review.rs`) proposed deleting `npm/LICENSE` as a
    /// `strong_candidate` exact-duplicate — which would break `npm publish`'s
    /// license packaging if a caller trusted that label. `OwnershipGovernance` is
    /// the same role `.github/CODEOWNERS` already gets (a legally/organizationally
    /// mandated file, not curatable content), and it is what `protected_roles` in
    /// `knowledge_review.rs` uses to keep a unit out of deletion proposals.
    #[test]
    fn license_files_get_ownership_governance_role_regardless_of_directory() {
        let (_root, shared) = fixture(&[
            ("LICENSE", "PolyForm Noncommercial License 1.0.0\n"),
            ("npm/LICENSE", "PolyForm Noncommercial License 1.0.0\n"),
            ("vendor/pkg/LICENSE.txt", "MIT License\n"),
            // The rest of the legal-provenance family: the dual-license pair Rust
            // crates ship, GNU's COPYING, Apache-2.0's mandatory NOTICE, and
            // UNLICENSE. Each is duplicated across paths for the same packaging
            // reason as LICENSE, and each was unprotected while only `LICENSE`
            // matched.
            ("LICENSE-APACHE", "Apache License 2.0\n"),
            ("crates/inner/LICENSE-APACHE", "Apache License 2.0\n"),
            ("LICENSE-MIT", "MIT License\n"),
            ("COPYING", "GNU GPL v3\n"),
            ("vendor/pkg/COPYING.LESSER", "GNU LGPL v3\n"),
            ("NOTICE", "Portions copyright the contributors.\n"),
            ("UNLICENSE", "This is free and unencumbered software.\n"),
            ("docs/license-notes.md", "# Licensing\nSee LICENSE.\n"),
        ]);

        let bridge = bridge(&shared);
        for path in [
            "LICENSE",
            "npm/LICENSE",
            "vendor/pkg/LICENSE.txt",
            "LICENSE-APACHE",
            "crates/inner/LICENSE-APACHE",
            "LICENSE-MIT",
            "COPYING",
            "vendor/pkg/COPYING.LESSER",
            "NOTICE",
            "UNLICENSE",
        ] {
            let card = bridge
                .cards
                .iter()
                .find(|card| card.anchor.path == path)
                .unwrap_or_else(|| panic!("card for {path}"));
            assert!(
                card.roles.iter().any(|(role, evidence)| {
                    *role == KnowledgeRole::OwnershipGovernance
                        && matches!(
                            evidence,
                            RoleEvidence::PathConvention { rule_id }
                                if rule_id == "role.path.license.v1"
                        )
                }),
                "{path} must get OwnershipGovernance via a path-convention rule, got: {:?}",
                card.roles
            );
        }

        // A markdown file that merely MENTIONS "license" in its own name must NOT
        // be swept in — only the exact `LICENSE`/`LICENSE.<ext>` filename pattern.
        for card in bridge
            .cards
            .iter()
            .filter(|card| card.anchor.path == "docs/license-notes.md")
        {
            assert!(
                card.roles
                    .iter()
                    .all(|(role, _)| *role != KnowledgeRole::OwnershipGovernance),
                "a file merely discussing licensing must not be treated as the license file itself: {:?}",
                card.roles
            );
        }
    }

    #[test]
    fn source_identity_partitions_equal_paths_symbols_and_link_ids() {
        let (_root, shared) = fixture(&[
            ("src/lib.rs", "pub fn launch() {}\n"),
            ("docs/guide.md", "Call `launch`.\n"),
        ]);
        let published = shared.published_generation();
        let source_a = published.source.as_deref().unwrap().clone();
        let mut source_b = source_a.clone();
        source_b.source_id = SourceId::new("other-source");
        let a = build_knowledge_bridge(
            &published.live,
            &source_a,
            published.content_generation,
            &BridgeLimits::default(),
        );
        let b = build_knowledge_bridge(
            &published.live,
            &source_b,
            published.content_generation,
            &BridgeLimits::default(),
        );

        assert_ne!(a.forward[0].id, b.forward[0].id);
        assert_ne!(a.forward[0].evidence.source, b.forward[0].evidence.source);
        let BridgeResolution::ResolvedExact(a_code) = &a.forward[0].resolution else {
            panic!("source A must resolve");
        };
        let BridgeResolution::ResolvedExact(b_code) = &b.forward[0].resolution else {
            panic!("source B must resolve");
        };
        assert_ne!(a_code.source, b_code.source);
    }

    #[test]
    fn contributor_prose_never_satisfies_declared_ownership() {
        let (_root, shared) = fixture(&[
            ("src/lib.rs", "pub fn launch() {}\n"),
            (
                "docs/history.md",
                "Contributors: @history-user and @past-maintainer.\n",
            ),
            (".github/CODEOWNERS", "/src/lib.rs @declared-owner\n"),
        ]);

        let bridge = bridge(&shared);
        assert_eq!(bridge.ownership_selectors.len(), 1);
        assert_eq!(bridge.forward.len(), 1);
        assert!(matches!(
            bridge.forward[0].evidence_kind,
            BridgeEvidenceKind::DeclaredOwnershipSelector
        ));
    }

    #[test]
    fn repeated_equal_generations_produce_identical_order_and_ids() {
        let (_root, shared) = fixture(&[
            ("src/z.rs", "pub fn zed() {}\n"),
            ("src/a.rs", "pub fn alpha() {}\n"),
            (
                "docs/guide.md",
                "`zed` then [alpha](../src/a.rs) and `alpha`.\n",
            ),
        ]);
        let published = shared.published_generation();
        let source = published.source.as_deref().unwrap();
        let left = build_knowledge_bridge(
            &published.live,
            source,
            published.content_generation,
            &BridgeLimits::default(),
        );
        let right = build_knowledge_bridge(
            &published.live,
            source,
            published.content_generation,
            &BridgeLimits::default(),
        );
        assert_eq!(left, right);
    }

    #[test]
    fn secret_positive_knowledge_content_creates_no_bridge_evidence() {
        let root = TempDir::new().unwrap();
        write(root.path(), "src/lib.rs", "pub fn launch() {}\n");
        let canary = ["runtime", "-", "bridge", "-", "canary"].concat();
        write(
            root.path(),
            "docs/guide.md",
            &format!("{}={canary}\nCall `launch`.\n", ["to", "ken"].concat()),
        );

        let shared = LiveIndex::load(root.path()).unwrap();
        let published = shared.published_generation();
        assert!(published.bridge.cards.is_empty());
        assert!(published.bridge.forward.is_empty());
        assert!(
            published
                .live
                .capture_file_content_view("docs/guide.md")
                .is_none()
        );
    }

    #[test]
    fn concurrent_bridge_readers_observe_one_source_and_content_generation() {
        let (root, shared) = fixture(&[
            ("src/lib.rs", "pub fn generation_zero() {}\n"),
            ("docs/guide.md", "Call `generation_zero`.\n"),
        ]);
        let root_path = root.path().to_path_buf();
        let participant_count = 5;
        let barrier = Arc::new(Barrier::new(participant_count));

        let writer_shared = Arc::clone(&shared);
        let writer_barrier = Arc::clone(&barrier);
        let writer = thread::spawn(move || {
            writer_barrier.wait();
            for generation in 1..=24 {
                let symbol = format!("generation_{generation}");
                write(
                    &root_path,
                    "src/lib.rs",
                    &format!("pub fn {symbol}() {{}}\n"),
                );
                write(&root_path, "docs/guide.md", &format!("Call `{symbol}`.\n"));
                writer_shared.reload(&root_path).unwrap();
            }
        });

        let readers: Vec<_> = (0..4)
            .map(|_| {
                let reader_shared = Arc::clone(&shared);
                let reader_barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    reader_barrier.wait();
                    for _ in 0..1_000 {
                        let published = reader_shared.published_generation();
                        let source = published.source.as_deref().expect("bound source");
                        for link in &published.bridge.forward {
                            assert_eq!(link.evidence.source, *source);
                            assert_eq!(
                                link.evidence.content_generation,
                                published.content_generation
                            );
                            if let BridgeResolution::ResolvedExact(anchor) = &link.resolution {
                                assert_eq!(anchor.source, *source);
                                assert_eq!(anchor.content_generation, published.content_generation);
                            }
                        }
                    }
                })
            })
            .collect();

        writer.join().unwrap();
        for reader in readers {
            reader.join().unwrap();
        }
    }

    /// Pins the win. RoleEvidence used to embed a full KnowledgeAnchor -- a
    /// clone of the card's own -- so every role entry carried a duplicate of
    /// its card's anchor. This asserts the anchor is gone from the variants and
    /// that RoleEvidence stays far smaller than the anchor it used to copy,
    /// turning a layout-derived estimate into a compiler-checked fact.
    #[test]
    fn role_evidence_no_longer_embeds_a_knowledge_anchor() {
        use std::mem::size_of;

        let evidence = size_of::<RoleEvidence>();
        let anchor = size_of::<KnowledgeAnchor>();
        assert!(
            evidence < anchor,
            "RoleEvidence ({evidence} B) must not carry a KnowledgeAnchor ({anchor} B)"
        );
        // A String is 24 B on 64-bit; the largest variant is one rule_id plus
        // the discriminant, so anything approaching an anchor means a field
        // crept back in.
        assert!(
            evidence <= 32,
            "RoleEvidence grew to {evidence} B; an anchor or similar payload is back"
        );
    }
}
