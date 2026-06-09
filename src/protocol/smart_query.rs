//! Smart query routing: natural-language entry point that classifies intent
//! and dispatches to the right specialized tool internally.

/// Classified intent from a natural-language query.
#[derive(Debug)]
pub enum QueryIntent {
    /// "who calls X", "callers of X", "references to X"
    FindCallers {
        symbol: String,
        path: Option<String>,
    },
    /// "where is X defined", "find symbol X"
    FindSymbol { name: String, kind: Option<String> },
    /// "find file X", "where is file X", "path to X"
    FindFile { hint: String },
    /// "what changed", "recent changes", "uncommitted"
    FindChanges,
    /// "how does X work", "explain X", "understand X"
    Understand { concept: String },
    /// Broad explanation query upgraded to a direct symbol walkthrough.
    UnderstandSymbol { symbol: String },
    /// Broad explanation query upgraded to implementation discovery.
    UnderstandImplementations { name: String },
    /// "search for X in code", "grep X", code pattern
    SearchCode { pattern: String },
    /// "what depends on X", "dependents of X"
    FindDependents { target: String },
    /// "implementations of X", "who implements X"
    FindImplementations { name: String },
    /// "which tool should I use for X", "what tools can I use for impact analysis"
    /// — a meta question about SymForge's own tool surface, NOT a code query.
    /// `topic` is an optional workflow keyword (e.g. "impact") to filter the catalog.
    ToolHelp { topic: Option<String> },
    /// Fallback: explore the concept
    Explore { query: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteConfidence {
    Exact,
    Inferred,
    Fallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteAssessment {
    pub confidence: RouteConfidence,
    pub rationale: &'static str,
    pub suggested_next_step: Option<&'static str>,
}

/// Classify a natural-language query into a routable intent.
pub fn classify_intent(query: &str) -> QueryIntent {
    classify_intent_with_match(query).0
}

/// Strip leading articles ("the", "a", "an") from a query for cleaner routing.
pub(crate) fn strip_leading_articles(q: &str) -> &str {
    let lower = q.as_bytes();
    for (article, len) in &[
        (&b"the "[..], 4),
        (&b"The "[..], 4),
        (&b"a "[..], 2),
        (&b"A "[..], 2),
        (&b"an "[..], 3),
        (&b"An "[..], 3),
    ] {
        if lower.starts_with(article) {
            return &q[*len..];
        }
    }
    q
}

/// Classify a query and report whether it matched an explicit routing phrase.
pub(crate) fn classify_intent_with_match(query: &str) -> (QueryIntent, bool) {
    let q = strip_leading_articles(query.trim());
    let lower = q.to_ascii_lowercase();

    // --- Meta: "which tool should I use", "what tools can I use for impact analysis" ---
    // Detected EARLY so a tool-recommendation question routes to the catalog instead
    // of falling through to `what is .../search for ...` code routing. Scoped narrowly
    // to (the word `tool`/`tools`) AND (a recommendation verb) so ordinary code queries
    // that merely mention a `Tool` type — e.g. "how does the Tool registry work" — are
    // NOT hijacked. This variant is intentionally kept OUT of the Understand|Explore
    // upgrade guard in `ask`, so a topic word matching an indexed symbol cannot clobber
    // it into UnderstandSymbol.
    if let Some(topic) = detect_tool_help(&lower) {
        return (QueryIntent::ToolHelp { topic }, true);
    }

    // --- Pattern: "who/what calls X" or "callers of X" or "references to X" ---
    if let Some(sym) = strip_prefix_phrase(
        &lower,
        &[
            "who calls ",
            "what calls ",
            "callers of ",
            "callers for ",
            "references to ",
            "references for ",
            "find references ",
            "usages of ",
            "who uses ",
        ],
    ) {
        let (symbol, path) = clean_symbol_and_optional_path(sym, q);
        return (QueryIntent::FindCallers { symbol, path }, true);
    }

    // --- Pattern: "what depends on X" or "dependents of X" ---
    if let Some(target) = strip_prefix_phrase(
        &lower,
        &[
            "what depends on ",
            "depends on ",
            "dependents of ",
            "dependents for ",
            "who imports ",
            "what imports ",
        ],
    ) {
        return (
            QueryIntent::FindDependents {
                target: clean_symbol_name(target, q),
            },
            true,
        );
    }

    // --- Pattern: "implementations of X" or "who implements X" ---
    if let Some(name) = strip_prefix_phrase(
        &lower,
        &[
            "implementations of ",
            "implementors of ",
            "who implements ",
            "what implements ",
            "implementations for ",
        ],
    ) {
        return (
            QueryIntent::FindImplementations {
                name: clean_symbol_name(name, q),
            },
            true,
        );
    }

    // --- Pattern: "find file X" or "path to X" ---
    // NOTE: this MUST precede the bare "where is " FindSymbol branch below so
    // that "where is file tools.rs" routes to FindFile, not to a FindSymbol
    // whose leading token would be "file".
    if let Some(hint) = strip_prefix_phrase(
        &lower,
        &[
            "find file ",
            "path to ",
            "where is file ",
            "locate file ",
            "which file ",
        ],
    ) {
        return (
            QueryIntent::FindFile {
                hint: hint.to_string(),
            },
            true,
        );
    }

    // --- Pattern: "where is X defined" or "find symbol X" or "definition of X" ---
    if let Some(name) = strip_prefix_phrase(
        &lower,
        &[
            "where is ",
            "find symbol ",
            "definition of ",
            "show me ",
            "go to ",
            "jump to ",
            "locate ",
            "find definition ",
            "find function ",
            "find struct ",
            "find class ",
            "find type ",
            "find method ",
            "find enum ",
            "find trait ",
            "find interface ",
        ],
    ) {
        let name = name
            .trim_end_matches(" defined")
            .trim_end_matches(" declaration");
        // Handle interior articles ("where is the User class defined") that
        // strip_leading_articles could not reach before the prefix matched.
        let name = strip_leading_articles(name);
        let (kind, after_kind) = extract_kind_hint(name);
        // A symbol is a single whitespace-delimited token. Take the first one
        // as the candidate. If the query carried more than that token — e.g. a
        // compound lookup like "X defined and what module imports it?" — the
        // extra words are NOT part of the symbol; downgrade routing confidence
        // (matched_prefix=false) so assess_route reports `Inferred` and chains a
        // follow-up hint instead of a confident false negative.
        let mut tokens = after_kind.split_whitespace();
        let first = tokens.next().unwrap_or(after_kind);
        let truncated = tokens.next().is_some();
        return (
            QueryIntent::FindSymbol {
                name: clean_symbol_name(first, q),
                kind,
            },
            !truncated,
        );
    }

    // --- Pattern: "what changed" or "recent changes" ---
    if lower.starts_with("what changed")
        || lower.starts_with("recent changes")
        || lower.starts_with("uncommitted")
        || lower == "changes"
        || lower.starts_with("what's changed")
        || lower.starts_with("show changes")
        || lower.starts_with("git status")
        || lower.starts_with("what did i change")
    {
        return (QueryIntent::FindChanges, true);
    }

    // --- Pattern: "how does X work" or "explain X" or "understand X" ---
    if let Some(concept) = strip_prefix_phrase(
        &lower,
        &[
            "how does ",
            "how do ",
            "explain ",
            "understand ",
            "what is ",
            "what are ",
            "describe ",
            "tell me about ",
            "help me understand ",
            "walk me through ",
        ],
    ) {
        let concept = concept
            .trim_end_matches(" work")
            .trim_end_matches(" works")
            .trim_end_matches("?");
        return (
            QueryIntent::Understand {
                concept: concept.to_string(),
            },
            true,
        );
    }

    // --- Pattern: "search for X" or "grep X" or "find X in code" ---
    if let Some(pattern) = strip_prefix_phrase(
        &lower,
        &[
            "search for ",
            "search ",
            "grep ",
            "find in code ",
            "look for ",
            "find text ",
            "find string ",
        ],
    ) {
        return (
            QueryIntent::SearchCode {
                pattern: pattern.trim_matches('"').trim_matches('\'').to_string(),
            },
            true,
        );
    }

    // --- Heuristic: looks like a file path (contains / or common extensions) ---
    if looks_like_path(q) {
        return (
            QueryIntent::FindFile {
                hint: q.to_string(),
            },
            false,
        );
    }

    // --- Heuristic: looks like a symbol name (CamelCase, snake_case, no spaces) ---
    if looks_like_symbol(q) {
        return (
            QueryIntent::FindSymbol {
                name: q.to_string(),
                kind: None,
            },
            false,
        );
    }

    // --- Heuristic: looks like a code pattern (operators, keywords, brackets) ---
    if looks_like_code_pattern(q) {
        return (
            QueryIntent::SearchCode {
                pattern: q.to_string(),
            },
            false,
        );
    }

    // --- Default: explore the concept ---
    (
        QueryIntent::Explore {
            query: q.to_string(),
        },
        false,
    )
}

pub fn assess_route(intent: &QueryIntent, matched_prefix: bool) -> RouteAssessment {
    match intent {
        QueryIntent::FindCallers { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit caller/reference phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a symbol-centric caller query from the input shape",
                    suggested_next_step: Some(
                        "If this route looks wrong, ask with explicit phrasing like `who calls X` or `references to X`.",
                    ),
                }
            }
        }
        QueryIntent::FindDependents { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit dependent/import phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a dependency-path question from the input shape",
                    suggested_next_step: Some(
                        "If this route looks wrong, ask with explicit phrasing like `what depends on X` or call `find_dependents` directly.",
                    ),
                }
            }
        }
        QueryIntent::FindImplementations { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit implementation phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred an implementation query from the symbol-like input",
                    suggested_next_step: Some(
                        "If this route looks wrong, ask with explicit phrasing like `implementations of X`.",
                    ),
                }
            }
        }
        QueryIntent::FindSymbol { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit definition/lookup phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a symbol lookup from a symbol-like query",
                    suggested_next_step: Some(
                        "Chain `search_symbols -> find_references` to locate the definition and then who imports or calls it. If the route looks wrong, call `search_files` for paths or `search_text` for literal text instead.",
                    ),
                }
            }
        }
        QueryIntent::FindFile { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit file/path phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a file lookup from path-like input",
                    suggested_next_step: Some(
                        "If this route looks wrong, call `search_text` for literal content or `search_symbols` for code names.",
                    ),
                }
            }
        }
        QueryIntent::FindChanges => RouteAssessment {
            confidence: RouteConfidence::Exact,
            rationale: "matched explicit change/status phrasing",
            suggested_next_step: None,
        },
        QueryIntent::Understand { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit explanation/understanding phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a conceptual exploration from the query wording",
                    suggested_next_step: Some(
                        "If this route looks too broad, ask for a specific symbol, file, or caller relationship.",
                    ),
                }
            }
        }
        QueryIntent::UnderstandSymbol { .. } => RouteAssessment {
            confidence: RouteConfidence::Inferred,
            rationale: "detected an exact indexed symbol inside a broad explanation query",
            suggested_next_step: Some(
                "If this route is too narrow, ask about the wider concept explicitly or call `explore` directly.",
            ),
        },
        QueryIntent::UnderstandImplementations { .. } => RouteAssessment {
            confidence: RouteConfidence::Inferred,
            rationale: "detected a distinctive trait-like symbol inside a broad explanation query about implementations or types",
            suggested_next_step: Some(
                "If this route is too narrow, ask about the wider concept explicitly or call `explore` directly.",
            ),
        },
        QueryIntent::SearchCode { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit code-search phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred literal or pattern search from code-like syntax",
                    suggested_next_step: Some(
                        "If this route looks wrong, call `search_symbols` for names or rephrase with `search for ...`.",
                    ),
                }
            }
        }
        QueryIntent::ToolHelp { .. } => RouteAssessment {
            confidence: RouteConfidence::Exact,
            rationale: "matched a tool-recommendation question about SymForge's own tool surface",
            suggested_next_step: Some(
                "Pick a tool from the catalog below, or ask `which tool should I use for <topic>?` to narrow it.",
            ),
        },
        QueryIntent::Explore { .. } => RouteAssessment {
            confidence: RouteConfidence::Fallback,
            rationale: "no stronger route matched, so SymForge fell back to conceptual exploration",
            suggested_next_step: Some(
                "If this is too broad, rephrase with a direct intent like `who calls`, `find symbol`, `find file`, or `search for`.",
            ),
        },
    }
}

pub fn route_confidence_label(confidence: RouteConfidence) -> &'static str {
    match confidence {
        RouteConfidence::Exact => "exact",
        RouteConfidence::Inferred => "inferred",
        RouteConfidence::Fallback => "fallback",
    }
}

pub fn route_invocation(intent: &QueryIntent) -> String {
    match intent {
        QueryIntent::FindCallers { symbol, path } => {
            if let Some(path) = path {
                format!("find_references(name=\"{symbol}\", path=\"{path}\")")
            } else {
                format!("find_references(name=\"{symbol}\")")
            }
        }
        QueryIntent::FindSymbol { name, kind } => {
            if let Some(k) = kind {
                format!("search_symbols(query=\"{name}\", kind=\"{k}\")")
            } else {
                format!("search_symbols(query=\"{name}\")")
            }
        }
        QueryIntent::FindFile { hint } => {
            format!("search_files(query=\"{hint}\")")
        }
        QueryIntent::FindChanges => "what_changed(uncommitted=true)".to_string(),
        QueryIntent::Understand { concept } => {
            format!("explore(query=\"{concept}\", depth=2)")
        }
        QueryIntent::UnderstandSymbol { symbol } => {
            format!("get_symbol_context(name=\"{symbol}\")")
        }
        QueryIntent::UnderstandImplementations { name } => {
            format!("find_references(name=\"{name}\", mode=\"implementations\")")
        }
        QueryIntent::SearchCode { pattern } => {
            format!("search_text(query=\"{pattern}\")")
        }
        QueryIntent::FindDependents { target } => {
            format!("find_dependents(path=\"{target}\")")
        }
        QueryIntent::FindImplementations { name } => {
            format!("find_references(name=\"{name}\", mode=\"implementations\")")
        }
        QueryIntent::ToolHelp { topic } => match topic {
            Some(topic) => format!("ask(query=\"which tool should I use for {topic}?\")"),
            None => "ask(query=\"which tool should I use?\")".to_string(),
        },
        QueryIntent::Explore { query } => {
            format!("explore(query=\"{query}\")")
        }
    }
}

pub fn route_tool_name(intent: &QueryIntent) -> &'static str {
    match intent {
        QueryIntent::FindCallers { .. } => "find_references",
        QueryIntent::FindSymbol { .. } => "search_symbols",
        QueryIntent::FindFile { .. } => "search_files",
        QueryIntent::FindChanges => "what_changed",
        QueryIntent::Understand { .. } => "explore",
        QueryIntent::UnderstandSymbol { .. } => "get_symbol_context",
        QueryIntent::UnderstandImplementations { .. } => "find_references",
        QueryIntent::SearchCode { .. } => "search_text",
        QueryIntent::FindDependents { .. } => "find_dependents",
        QueryIntent::FindImplementations { .. } => "find_references",
        QueryIntent::ToolHelp { .. } => "ask",
        QueryIntent::Explore { .. } => "explore",
    }
}

/// Try each prefix phrase; return the remainder if one matches.
fn strip_prefix_phrase<'a>(lower: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for prefix in prefixes {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return Some(rest);
            }
        }
    }
    None
}

/// Extract a kind hint from phrases like "function foo" or "struct Bar".
fn extract_kind_hint(name: &str) -> (Option<String>, &str) {
    let kind_prefixes = [
        ("function ", "fn"),
        ("fn ", "fn"),
        ("struct ", "struct"),
        ("class ", "class"),
        ("type ", "type"),
        ("method ", "method"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("interface ", "interface"),
        ("constant ", "constant"),
        ("const ", "constant"),
        ("variable ", "variable"),
        ("var ", "variable"),
    ];
    for (prefix, kind) in &kind_prefixes {
        if let Some(stripped) = name.strip_prefix(prefix) {
            return (Some(kind.to_string()), stripped);
        }
    }
    (None, name)
}

/// Preserve original casing from the user's query for the matched portion.
fn clean_symbol_name(lower_match: &str, original: &str) -> String {
    clean_symbol_and_optional_path(lower_match, original).0
}

fn clean_symbol_and_optional_path(lower_match: &str, original: &str) -> (String, Option<String>) {
    // Try to find the original-cased version in the user's query
    let lower_original = original.to_ascii_lowercase();
    if let Some(pos) = lower_original.find(lower_match) {
        return split_trailing_scope_hint(original[pos..pos + lower_match.len()].trim());
    }
    split_trailing_scope_hint(lower_match.trim())
}

fn split_trailing_scope_hint(value: &str) -> (String, Option<String>) {
    for needle in [" within ", " inside ", " under ", " in "] {
        if let Some((head, tail)) = value.rsplit_once(needle) {
            let head = head.trim();
            let tail = tail.trim();
            if !head.is_empty() && looks_like_scope_hint(tail) {
                return (head.to_string(), Some(tail.to_string()));
            }
        }
    }
    (value.trim().to_string(), None)
}

fn looks_like_scope_hint(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    looks_like_path(trimmed)
        || trimmed.contains("::")
        || ((trimmed.contains('/') || trimmed.contains('\\'))
            && trimmed
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '/' | '\\' | '_' | '-' | '.' | ':')))
}

fn looks_like_path(q: &str) -> bool {
    // Contains path separators or file extensions
    (q.contains('/') || q.contains('\\'))
        || q.ends_with(".rs")
        || q.ends_with(".py")
        || q.ends_with(".ts")
        || q.ends_with(".js")
        || q.ends_with(".go")
        || q.ends_with(".java")
        || q.ends_with(".toml")
        || q.ends_with(".yaml")
        || q.ends_with(".yml")
        || q.ends_with(".json")
        || q.ends_with(".md")
}

fn looks_like_symbol(q: &str) -> bool {
    if q.len() < 3 || q.contains(' ') {
        return false;
    }
    if q.chars().all(|c| c == '_') {
        return false;
    }
    // CamelCase: has uppercase not at start, or has underscore (snake_case)
    let has_camel = q.chars().skip(1).any(|c| c.is_uppercase());
    let has_snake = q.contains('_');
    let has_colons = q.contains("::");
    let all_alnum = q
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ':');
    all_alnum && (has_camel || has_snake || has_colons)
}

fn looks_like_code_pattern(q: &str) -> bool {
    // Contains operators, brackets, or obvious code syntax
    q.contains("==")
        || q.contains("!=")
        || q.contains("->")
        || q.contains("=>")
        || q.contains("fn ")
        || q.contains("pub ")
        || q.contains("let ")
        || q.contains("def ")
        || q.contains("class ")
        || q.contains("impl ")
        || q.contains("struct ")
        || (q.contains('(') && q.contains(')'))
        || (q.contains('{') && q.contains('}'))
}

/// Describe which tool was routed to, for the LLM to learn the mapping.
pub fn route_description(intent: &QueryIntent) -> String {
    format!("[Routed to: {}]", route_invocation(intent))
}

// ---------------------------------------------------------------------------
// Tool catalog — workflow-grouped index of SymForge's own tool surface.
//
// This powers the `ask` ToolHelp intent and the `symforge://tools/catalog`
// resource. Every tool name listed here MUST exist in `SYMFORGE_TOOL_NAMES`
// (asserted by a drift-guard test in `cli/init.rs`) so the catalog cannot
// silently drift from the real registered surface.
// ---------------------------------------------------------------------------

/// One workflow group in the tool catalog.
#[derive(Debug, Clone, Copy)]
pub struct ToolGroup {
    /// Stable group key, also used as the primary topic-filter keyword.
    pub key: &'static str,
    /// Additional topic-filter keywords that select this group.
    pub aliases: &'static [&'static str],
    /// One-line description of what this group of tools is for.
    pub blurb: &'static str,
    /// Bare tool names (no `mcp__symforge__` prefix) belonging to this group.
    pub tools: &'static [&'static str],
}

/// The static workflow-grouped catalog of SymForge tools.
pub fn tool_catalog_groups() -> &'static [ToolGroup] {
    const GROUPS: &[ToolGroup] = &[
        ToolGroup {
            key: "orientation",
            aliases: &["orient", "overview", "explore", "start", "navigate"],
            blurb: "Get your bearings in an unfamiliar repository.",
            tools: &[
                "get_repo_map",
                "get_file_context",
                "explore",
                "ask",
                "conventions",
            ],
        },
        ToolGroup {
            key: "search",
            aliases: &["find", "grep", "lookup", "discover"],
            blurb: "Locate files, symbols, and text across the codebase.",
            tools: &[
                "search_symbols",
                "search_text",
                "search_files",
                "inspect_match",
            ],
        },
        ToolGroup {
            key: "symbol-context",
            aliases: &["symbol", "definition", "callers", "callees", "references"],
            blurb: "Read a symbol's source and trace how it connects to the rest of the code.",
            tools: &[
                "get_symbol",
                "get_symbol_context",
                "find_references",
                "get_file_content",
            ],
        },
        ToolGroup {
            key: "impact-analysis",
            aliases: &[
                "impact",
                "blast-radius",
                "dependents",
                "what-breaks",
                "change",
            ],
            blurb: "Understand what a change touches and what would break.",
            tools: &[
                "find_references",
                "find_dependents",
                "get_symbol_context",
                "analyze_file_impact",
                "what_changed",
                "diff_symbols",
            ],
        },
        ToolGroup {
            key: "dry-run-edits",
            aliases: &["edit", "edits", "refactor", "rename", "modify", "mutate"],
            blurb: "Make structural source edits (preview with dry_run before writing).",
            tools: &[
                "edit_plan",
                "replace_symbol_body",
                "edit_within_symbol",
                "insert_symbol",
                "delete_symbol",
                "batch_edit",
                "batch_insert",
                "batch_rename",
            ],
        },
        ToolGroup {
            key: "project-switching",
            aliases: &["switch", "reindex", "index", "checkpoint", "project"],
            blurb: "Re-index a different folder or persist the current index.",
            tools: &["index_folder", "checkpoint_now"],
        },
        ToolGroup {
            key: "diagnostics",
            aliases: &["diagnostic", "health", "status", "validate", "session"],
            blurb: "Check index health, validate syntax, and inspect session context.",
            tools: &[
                "health",
                "health_compact",
                "validate_file_syntax",
                "context_inventory",
                "investigation_suggest",
            ],
        },
    ];
    GROUPS
}

/// Render the full tool catalog as a human-readable workflow guide.
pub fn render_tool_catalog() -> String {
    let mut out = String::from(
        "SymForge tool catalog — grouped by workflow.\nCall any tool by name; pass `dry_run=true` to edit tools to preview without writing.\n",
    );
    for group in tool_catalog_groups() {
        push_group(&mut out, group);
    }
    out.push_str(
        "\nTip: ask `which tool should I use for <topic>?` (e.g. impact, search, edits) to filter this catalog.",
    );
    out
}

/// Render only the catalog group(s) matching `topic`. Falls back to the full
/// catalog when `topic` is `None` or matches no group.
pub fn render_tool_catalog_for_topic(topic: Option<&str>) -> String {
    let Some(topic) = topic
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
    else {
        return render_tool_catalog();
    };

    let matches: Vec<&ToolGroup> = tool_catalog_groups()
        .iter()
        .filter(|group| group_matches_topic(group, &topic))
        .collect();

    if matches.is_empty() {
        // Unknown topic — give the full catalog rather than nothing.
        return render_tool_catalog();
    }

    let mut out = format!("SymForge tools for \"{topic}\":\n");
    for group in matches {
        push_group(&mut out, group);
    }
    out.push_str("\nTip: omit the topic to see the full tool catalog.");
    out
}

/// True when `topic` (already lowercased) selects this group via its key or aliases.
fn group_matches_topic(group: &ToolGroup, topic: &str) -> bool {
    if group.key.contains(topic) || topic.contains(group.key) {
        return true;
    }
    group
        .aliases
        .iter()
        .any(|alias| alias.contains(topic) || topic.contains(alias))
}

fn push_group(out: &mut String, group: &ToolGroup) {
    out.push_str(&format!("\n## {} — {}\n", group.key, group.blurb));
    out.push_str(&format!("  {}\n", group.tools.join(", ")));
}

/// Recommendation verbs that, combined with the word `tool`/`tools`, indicate a
/// question ABOUT SymForge's tool surface rather than a code query that merely
/// mentions a `Tool` type.
const TOOL_HELP_VERBS: &[&str] = &[
    "which ",
    "recommend",
    "should i use",
    "can i use",
    "do i use",
    "what tool",
    "what tools",
    "best tool",
    "right tool",
    "use for",
];

/// Detect a tool-recommendation meta-question and extract its optional topic.
///
/// Returns `Some(topic)` when the query is asking which SymForge tool to use:
/// it must contain the word `tool`/`tools` AND a recommendation verb. The inner
/// `Option<String>` is the workflow topic (e.g. `Some("impact")`) or `None` when
/// no recognizable topic keyword is present.
///
/// `lower` is the already-lowercased, article-stripped query.
fn detect_tool_help(lower: &str) -> Option<Option<String>> {
    let mentions_tool = contains_word(lower, "tool") || contains_word(lower, "tools");
    if !mentions_tool {
        return None;
    }
    let has_verb = TOOL_HELP_VERBS.iter().any(|verb| lower.contains(verb));
    if !has_verb {
        return None;
    }
    Some(extract_tool_help_topic(lower))
}

/// True when `needle` appears in `haystack` as a whole word (alnum/underscore
/// boundaries). Avoids matching "tool" inside "toolkit" or, more importantly,
/// avoids a substring false positive bleeding into unrelated detection.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let nbytes = needle.as_bytes();
    if nbytes.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs = start + pos;
        let before_ok = abs == 0 || !is_word_byte(bytes[abs - 1]);
        let after = abs + nbytes.len();
        let after_ok = after >= bytes.len() || !is_word_byte(bytes[after]);
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Pull a workflow topic keyword out of a tool-help query, if one is present.
/// Matches against each catalog group's key and aliases so "impact analysis"
/// resolves to the "impact-analysis" group.
fn extract_tool_help_topic(lower: &str) -> Option<String> {
    // Prefer the canonical group key when a key token appears, then fall back
    // to aliases. Return the matched keyword so the renderer can re-resolve it.
    for group in tool_catalog_groups() {
        // Match the leading token of a hyphenated key too ("impact" in
        // "impact-analysis") so natural phrasing like "impact analysis" hits.
        let key_head = group.key.split('-').next().unwrap_or(group.key);
        if contains_word(lower, group.key) || contains_word(lower, key_head) {
            return Some(key_head.to_string());
        }
    }
    for group in tool_catalog_groups() {
        for alias in group.aliases {
            if contains_word(lower, alias) {
                return Some((*alias).to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_callers() {
        match classify_intent("who calls optimize_deterministic") {
            QueryIntent::FindCallers { symbol, path } => {
                assert_eq!(symbol, "optimize_deterministic");
                assert_eq!(path, None);
            }
            other => panic!("Expected FindCallers, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_callers_references() {
        match classify_intent("references to LiveIndex") {
            QueryIntent::FindCallers { symbol, path } => {
                assert_eq!(symbol, "LiveIndex");
                assert_eq!(path, None);
            }
            other => panic!("Expected FindCallers, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_callers_preserves_trailing_path_scope_hint() {
        match classify_intent("who calls AddCoreServices in src/protocol/tools.rs") {
            QueryIntent::FindCallers { symbol, path } => {
                assert_eq!(symbol, "AddCoreServices");
                assert_eq!(path.as_deref(), Some("src/protocol/tools.rs"));
            }
            other => panic!("Expected FindCallers, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_callers_path_scope_does_not_capture_plain_language() {
        match classify_intent("who calls actor in production") {
            QueryIntent::FindCallers { symbol, path } => {
                assert_eq!(symbol, "actor in production");
                assert_eq!(path, None);
            }
            other => panic!("Expected FindCallers, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_find_symbol() {
        match classify_intent("where is optimize_deterministic defined") {
            QueryIntent::FindSymbol { name, kind } => {
                assert_eq!(name, "optimize_deterministic");
                assert!(kind.is_none());
            }
            other => panic!("Expected FindSymbol, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_find_symbol_with_kind() {
        match classify_intent("find struct LiveIndex") {
            QueryIntent::FindSymbol { name, kind } => {
                assert_eq!(name, "LiveIndex");
                assert_eq!(kind, None);
            }
            other => panic!("Expected FindSymbol, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_find_file() {
        match classify_intent("find file tools.rs") {
            QueryIntent::FindFile { hint } => assert_eq!(hint, "tools.rs"),
            other => panic!("Expected FindFile, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_path_heuristic() {
        match classify_intent("src/protocol/mod.rs") {
            QueryIntent::FindFile { hint } => assert_eq!(hint, "src/protocol/mod.rs"),
            other => panic!("Expected FindFile, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_symbol_heuristic() {
        match classify_intent("LiveIndex") {
            QueryIntent::FindSymbol { name, .. } => assert_eq!(name, "LiveIndex"),
            other => panic!("Expected FindSymbol, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_snake_case_heuristic() {
        match classify_intent("search_symbols_with_options") {
            QueryIntent::FindSymbol { name, .. } => assert_eq!(name, "search_symbols_with_options"),
            other => panic!("Expected FindSymbol, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_changes() {
        match classify_intent("what changed") {
            QueryIntent::FindChanges => {}
            other => panic!("Expected FindChanges, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_understand() {
        match classify_intent("how does the synergy pipeline work") {
            QueryIntent::Understand { concept } => assert_eq!(concept, "the synergy pipeline"),
            other => panic!("Expected Understand, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_search_code() {
        match classify_intent("search for TODO") {
            QueryIntent::SearchCode { pattern } => assert_eq!(pattern, "todo"),
            other => panic!("Expected SearchCode, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_dependents() {
        match classify_intent("what depends on src/protocol/mod.rs") {
            QueryIntent::FindDependents { target } => assert_eq!(target, "src/protocol/mod.rs"),
            other => panic!("Expected FindDependents, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_implementations() {
        match classify_intent("implementations of LlmClient") {
            QueryIntent::FindImplementations { name } => assert_eq!(name, "LlmClient"),
            other => panic!("Expected FindImplementations, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_explore_fallback() {
        match classify_intent("error handling patterns") {
            QueryIntent::Explore { query } => assert_eq!(query, "error handling patterns"),
            other => panic!("Expected Explore, got {:?}", other),
        }
    }

    #[test]
    fn test_route_description() {
        let intent = classify_intent("who calls optimize_deterministic");
        let desc = route_description(&intent);
        assert!(desc.contains("find_references"));
        assert!(desc.contains("optimize_deterministic"));
    }

    #[test]
    fn test_assess_route_exact() {
        let (intent, matched_prefix) =
            classify_intent_with_match("who calls optimize_deterministic");
        let assessment = assess_route(&intent, matched_prefix);
        assert_eq!(assessment.confidence, RouteConfidence::Exact);
        assert_eq!(assessment.suggested_next_step, None);
    }

    #[test]
    fn test_assess_route_inferred() {
        let (intent, matched_prefix) = classify_intent_with_match("LiveIndex");
        let assessment = assess_route(&intent, matched_prefix);
        assert_eq!(assessment.confidence, RouteConfidence::Inferred);
        assert!(assessment.suggested_next_step.is_some());
    }

    #[test]
    fn test_assess_route_fallback() {
        let (intent, matched_prefix) = classify_intent_with_match("error handling patterns");
        let assessment = assess_route(&intent, matched_prefix);
        assert_eq!(assessment.confidence, RouteConfidence::Fallback);
        assert!(assessment.suggested_next_step.is_some());
    }

    #[test]
    fn test_route_invocation() {
        let intent = classify_intent("src/protocol/mod.rs");
        let invocation = route_invocation(&intent);
        assert!(invocation.contains("search_files"));
        assert!(invocation.contains("src/protocol/mod.rs"));
    }

    #[test]
    fn test_assess_route_understand_implementations() {
        let intent = QueryIntent::UnderstandImplementations {
            name: "Actor".to_string(),
        };
        let assessment = assess_route(&intent, false);
        assert_eq!(assessment.confidence, RouteConfidence::Inferred);
        assert!(assessment.rationale.contains("trait-like symbol"));
        assert!(assessment.suggested_next_step.is_some());
    }

    #[test]
    fn test_classify_intent_with_match_reports_explicit_prefix() {
        let (intent, matched_prefix) =
            classify_intent_with_match("who calls optimize_deterministic");
        assert!(matched_prefix);
        assert!(matches!(intent, QueryIntent::FindCallers { .. }));
    }

    #[test]
    fn test_classify_intent_with_match_reports_inferred_symbol() {
        let (intent, matched_prefix) = classify_intent_with_match("LiveIndex");
        assert!(!matched_prefix);
        assert!(matches!(intent, QueryIntent::FindSymbol { .. }));
    }

    #[test]
    fn test_classify_intent_compound_lookup_extracts_leading_token() {
        // SF-005: a compound lookup must extract only the leading symbol token and
        // downgrade routing confidence (matched_prefix=false) so the route is
        // reported as `Inferred`, not a confident false negative.
        let (intent, matched_prefix) = classify_intent_with_match(
            "Where is TestingController defined and what module imports it?",
        );
        match &intent {
            QueryIntent::FindSymbol { name, .. } => {
                assert_eq!(
                    name, "TestingController",
                    "leading token only; intent: {intent:?}"
                );
            }
            other => panic!("expected FindSymbol, got {other:?}"),
        }
        // Assert BOTH the name AND the truncation signal: matched_prefix must be
        // false so assess_route downgrades Exact -> Inferred.
        assert!(
            !matched_prefix,
            "compound lookup truncated extra tokens, so confidence must be downgraded"
        );
        let assessment = assess_route(&intent, matched_prefix);
        assert_eq!(assessment.confidence, RouteConfidence::Inferred);
        let next = assessment
            .suggested_next_step
            .expect("downgraded route must chain a follow-up hint");
        assert!(
            next.contains("search_symbols") && next.contains("find_references"),
            "next step must name the chained sequence: {next}"
        );
    }

    #[test]
    fn test_classify_intent_clean_single_symbol_lookup_stays_exact() {
        // A clean single-symbol lookup with a trailing suffix word must still
        // extract the symbol AND keep Exact confidence (nothing was truncated).
        let (intent, matched_prefix) =
            classify_intent_with_match("where is optimize_deterministic defined");
        match &intent {
            QueryIntent::FindSymbol { name, .. } => {
                assert_eq!(name, "optimize_deterministic");
            }
            other => panic!("expected FindSymbol, got {other:?}"),
        }
        assert!(matched_prefix, "single token, no truncation -> stays Exact");
        assert_eq!(
            assess_route(&intent, matched_prefix).confidence,
            RouteConfidence::Exact
        );
    }

    #[test]
    fn test_classify_intent_interior_article_and_kind() {
        // SF-005: "where is the User class defined" — interior article + trailing
        // suffix + kind-as-suffix. Leading-token extraction after article stripping
        // must yield "User", and the leftover words must downgrade confidence.
        let (intent, matched_prefix) =
            classify_intent_with_match("where is the User class defined");
        match &intent {
            QueryIntent::FindSymbol { name, .. } => {
                assert_eq!(
                    name, "User",
                    "interior article stripped; intent: {intent:?}"
                );
            }
            other => panic!("expected FindSymbol, got {other:?}"),
        }
        assert!(
            !matched_prefix,
            "the trailing 'class' word is not part of the symbol -> Inferred"
        );
    }

    #[test]
    fn test_classify_intent_where_is_file_routes_to_find_file() {
        // SF-005 guard: after reordering "where is file " ahead of bare "where is ",
        // a file lookup must route to FindFile, not FindSymbol with name "file".
        let (intent, matched_prefix) = classify_intent_with_match("where is file tools.rs");
        assert!(matched_prefix);
        match &intent {
            QueryIntent::FindFile { hint } => assert_eq!(hint, "tools.rs"),
            other => panic!("expected FindFile, got {other:?}"),
        }
    }

    #[test]
    fn test_route_invocation_understand_implementations() {
        let intent = QueryIntent::UnderstandImplementations {
            name: "Actor".to_string(),
        };
        let invocation = route_invocation(&intent);
        assert_eq!(
            invocation,
            "find_references(name=\"Actor\", mode=\"implementations\")"
        );
        assert_eq!(route_tool_name(&intent), "find_references");
    }

    #[test]
    fn test_strip_leading_articles() {
        assert_eq!(
            strip_leading_articles("the error handling"),
            "error handling"
        );
        assert_eq!(
            strip_leading_articles("The LiveIndex struct"),
            "LiveIndex struct"
        );
        assert_eq!(strip_leading_articles("a config file"), "config file");
        assert_eq!(strip_leading_articles("an async handler"), "async handler");
        // Should NOT strip when not followed by space
        assert_eq!(strip_leading_articles("theorem"), "theorem");
        assert_eq!(strip_leading_articles("ankle"), "ankle");
    }

    #[test]
    fn test_classify_strips_article_before_routing() {
        // "the error handling" should route the same as "error handling"
        let (intent, _) = classify_intent_with_match("the error handling");
        assert!(matches!(
            intent,
            QueryIntent::Understand { .. } | QueryIntent::Explore { .. }
        ));
    }

    // --- SF-010: ToolHelp intent ------------------------------------------

    #[test]
    fn test_classify_tool_help_with_impact_topic() {
        let (intent, matched) =
            classify_intent_with_match("what tools can I use for impact analysis?");
        match intent {
            QueryIntent::ToolHelp { topic } => {
                assert_eq!(topic.as_deref(), Some("impact"));
            }
            other => panic!("expected ToolHelp, got {other:?}"),
        }
        assert!(matched, "tool-recommendation phrasing is an explicit match");
    }

    #[test]
    fn test_classify_tool_help_without_topic() {
        let (intent, _) = classify_intent_with_match("which tool should I use?");
        assert!(
            matches!(intent, QueryIntent::ToolHelp { topic: None }),
            "a topic-less tool-recommendation question routes to ToolHelp {{ None }}; got {intent:?}"
        );
    }

    #[test]
    fn test_classify_tool_help_does_not_hijack_code_query_mentioning_tool() {
        // CRITICAL hijack guard: a code query that merely mentions a `Tool` type
        // (no recommendation verb) must NOT be classified as ToolHelp.
        let (intent, _) = classify_intent_with_match("how does the Tool registry work");
        assert!(
            matches!(
                intent,
                QueryIntent::Understand { .. } | QueryIntent::Explore { .. }
            ),
            "code query mentioning `Tool` must stay Understand/Explore; got {intent:?}"
        );
        assert!(!matches!(intent, QueryIntent::ToolHelp { .. }));
    }

    #[test]
    fn test_classify_tool_help_recommend_verb_variants() {
        for q in [
            "which tool should I use for refactoring",
            "recommend a tool for searching",
            "can I use a tool for diagnostics",
        ] {
            let (intent, _) = classify_intent_with_match(q);
            assert!(
                matches!(intent, QueryIntent::ToolHelp { .. }),
                "{q:?} should route to ToolHelp; got {intent:?}"
            );
        }
    }

    #[test]
    fn test_render_tool_catalog_for_impact_lists_impact_tools() {
        let rendered = render_tool_catalog_for_topic(Some("impact"));
        for tool in [
            "find_references",
            "find_dependents",
            "get_symbol_context",
            "analyze_file_impact",
            "what_changed",
            "diff_symbols",
        ] {
            assert!(
                rendered.contains(tool),
                "impact catalog missing {tool}; got:\n{rendered}"
            );
        }
    }

    #[test]
    fn test_render_tool_catalog_unknown_topic_falls_back_to_full() {
        let rendered = render_tool_catalog_for_topic(Some("nonsense-xyzzy"));
        // Full catalog covers every group, including diagnostics.
        assert!(rendered.contains("health"));
        assert!(rendered.contains("search_symbols"));
        assert!(rendered.contains("replace_symbol_body"));
    }

    #[test]
    fn test_render_tool_catalog_none_topic_is_full() {
        let rendered = render_tool_catalog_for_topic(None);
        assert!(rendered.contains("orientation"));
        assert!(rendered.contains("diagnostics"));
    }

    #[test]
    fn test_tool_catalog_groups_cover_all_workflow_keys() {
        let keys: Vec<&str> = tool_catalog_groups().iter().map(|g| g.key).collect();
        for expected in [
            "orientation",
            "search",
            "symbol-context",
            "impact-analysis",
            "dry-run-edits",
            "project-switching",
            "diagnostics",
        ] {
            assert!(keys.contains(&expected), "missing catalog group {expected}");
        }
    }
}
