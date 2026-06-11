//! Concept → pattern mapping for the `explore` tool.

/// A set of search patterns associated with a programming concept.
pub struct ConceptPattern {
    pub label: &'static str,
    pub symbol_queries: &'static [&'static str],
    pub text_queries: &'static [&'static str],
    pub kind_filters: &'static [&'static str],
}

const FALLBACK_STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "does", "for", "from", "has", "he",
    "how", "in", "is", "it", "its", "not", "of", "on", "or", "that", "the", "this", "to", "was",
    "were", "what", "when", "where", "which", "who", "why", "will", "with",
];

// Sorted by key length descending so longer/more-specific keys match first.
pub const CONCEPT_MAP: &[(&str, ConceptPattern)] = &[
    (
        "actor supervision",
        ConceptPattern {
            label: "Actor Supervision",
            symbol_queries: &[
                "Actor",
                "ActorRef",
                "supervisor",
                "supervision",
                "mailbox",
                "spawn",
                "message",
            ],
            text_queries: &[
                "handle_supervisor_evt",
                "SupervisionEvent",
                "ActorProcessingErr",
                "Actor::spawn",
                "ActorRef",
            ],
            kind_filters: &["struct", "fn", "impl"],
        },
    ),
    (
        "error handling",
        ConceptPattern {
            label: "Error Handling",
            symbol_queries: &["Error", "Result", "anyhow", "bail", "catch"],
            text_queries: &["unwrap()", ".expect(", "return Err(", "try {", "catch"],
            kind_filters: &["struct", "enum", "fn"],
        },
    ),
    (
        "file watching",
        ConceptPattern {
            label: "File Watching",
            symbol_queries: &["watcher", "notify", "debounce", "burst"],
            text_queries: &["notify::Event", "DebouncedEvent", "file_event", "inotify"],
            kind_filters: &[],
        },
    ),
    (
        "serialization",
        ConceptPattern {
            label: "Serialization",
            symbol_queries: &["serialize", "deserialize", "serde", "json", "postcard"],
            text_queries: &[
                "#[derive(Serialize",
                "#[derive(Deserialize",
                "serde_json",
                "postcard::",
            ],
            kind_filters: &[],
        },
    ),
    (
        "authentication",
        ConceptPattern {
            label: "Authentication",
            symbol_queries: &[
                "auth",
                "login",
                "session",
                "token",
                "credential",
                "password",
            ],
            text_queries: &["Bearer", "JWT", "OAuth", "verify_token", "authenticate"],
            kind_filters: &[],
        },
    ),
    (
        "configuration",
        ConceptPattern {
            label: "Configuration",
            symbol_queries: &["config", "settings", "env", "options", "params"],
            text_queries: &["dotenv", "env::var", "process.env", "serde", "toml", "yaml"],
            kind_filters: &["struct"],
        },
    ),
    (
        "concurrency",
        ConceptPattern {
            label: "Concurrency",
            symbol_queries: &["Mutex", "RwLock", "Atomic", "channel", "spawn", "async"],
            text_queries: &[
                "tokio::spawn",
                "thread::spawn",
                ".lock()",
                ".read()",
                ".write()",
            ],
            kind_filters: &[],
        },
    ),
    (
        "permissions",
        ConceptPattern {
            label: "Permissions / Authorization",
            symbol_queries: &["permission", "role", "policy", "acl", "authorize"],
            text_queries: &["forbidden", "unauthorized", "access_control", "RBAC"],
            kind_filters: &[],
        },
    ),
    (
        "deployment",
        ConceptPattern {
            label: "Deployment / Release",
            symbol_queries: &["release", "deploy", "version", "publish", "migrate"],
            text_queries: &[
                "npm publish",
                "cargo publish",
                "release-please",
                "changelog",
            ],
            kind_filters: &[],
        },
    ),
    (
        "networking",
        ConceptPattern {
            label: "Networking",
            symbol_queries: &["socket", "listener", "bind", "connect", "server"],
            text_queries: &["TcpListener", "hyper", "axum", "reqwest", "tonic"],
            kind_filters: &[],
        },
    ),
    (
        "database",
        ConceptPattern {
            label: "Database",
            symbol_queries: &[
                "query",
                "migrate",
                "schema",
                "pool",
                "connection",
                "transaction",
            ],
            text_queries: &[
                "SELECT",
                "INSERT",
                "CREATE TABLE",
                "sqlx",
                "diesel",
                "TypeORM",
            ],
            kind_filters: &[],
        },
    ),
    (
        "indexing",
        ConceptPattern {
            label: "Indexing",
            symbol_queries: &["index", "reindex", "snapshot", "persist"],
            text_queries: &["LiveIndex", "index.bin", "reindex", "rebuild_reverse"],
            kind_filters: &[],
        },
    ),
    (
        "testing",
        ConceptPattern {
            label: "Testing",
            symbol_queries: &["test", "mock", "fixture", "assert", "expect"],
            text_queries: &["#[test]", "#[tokio::test]", "describe(", "it(", "pytest"],
            kind_filters: &["fn"],
        },
    ),
    (
        "parsing",
        ConceptPattern {
            label: "Parsing",
            symbol_queries: &["parse", "parser", "ast", "node", "tree_sitter"],
            text_queries: &["tree_sitter::", ".parse(", "syntax tree", "grammar"],
            kind_filters: &[],
        },
    ),
    (
        "caching",
        ConceptPattern {
            label: "Caching",
            symbol_queries: &["cache", "lru", "memoize", "ttl", "expire"],
            text_queries: &["LruCache", "cache.get(", "cached::", "moka::"],
            kind_filters: &[],
        },
    ),
    (
        "logging",
        ConceptPattern {
            label: "Logging / Observability",
            symbol_queries: &["log", "trace", "span", "metric", "telemetry"],
            text_queries: &["tracing::", "log::", "debug!", "warn!", "info!"],
            kind_filters: &[],
        },
    ),
    (
        "api",
        ConceptPattern {
            label: "API / HTTP",
            symbol_queries: &[
                "handler",
                "route",
                "endpoint",
                "controller",
                "request",
                "response",
            ],
            text_queries: &[
                "GET", "POST", "PUT", "DELETE", "Router", "axum", "actix", "express",
            ],
            kind_filters: &["fn"],
        },
    ),
    (
        "cli",
        ConceptPattern {
            label: "CLI / Command Line",
            symbol_queries: &["cli", "args", "command", "subcommand"],
            text_queries: &["clap", "structopt", "Arg::", "Command::new"],
            kind_filters: &[],
        },
    ),
    // -- C/C++ and systems programming concepts --
    (
        "memory allocation",
        ConceptPattern {
            label: "Memory Allocation",
            symbol_queries: &[
                "malloc",
                "calloc",
                "realloc",
                "free",
                "alloc",
                "aligned_malloc",
                "aligned_free",
                "mmap",
                "munmap",
                "arena",
                "pool",
                "allocator",
            ],
            text_queries: &[
                "malloc(",
                "calloc(",
                "realloc(",
                "free(",
                "new ",
                "delete ",
                "aligned_alloc(",
                "mmap(",
                "VirtualAlloc",
                "HeapAlloc",
            ],
            kind_filters: &["fn"],
        },
    ),
    (
        "tensor operations",
        ConceptPattern {
            label: "Tensor Operations",
            symbol_queries: &[
                "tensor",
                "matmul",
                "mul_mat",
                "conv",
                "softmax",
                "layernorm",
                "attention",
                "embedding",
                "quantize",
                "dequantize",
            ],
            text_queries: &[
                "ggml_tensor",
                "ggml_mul_mat",
                "ggml_add",
                "torch::Tensor",
                "at::Tensor",
                "cudnn",
                "cublas",
            ],
            kind_filters: &[],
        },
    ),
    (
        "simd",
        ConceptPattern {
            label: "SIMD / Vectorization",
            symbol_queries: &[
                "simd",
                "avx",
                "sse",
                "neon",
                "wasm_simd",
                "intrinsic",
                "vec_dot",
            ],
            text_queries: &[
                "__m128",
                "__m256",
                "__m512",
                "_mm_",
                "_mm256_",
                "_mm512_",
                "vfmadd",
                "vmulps",
                "float32x4",
                "vdotq",
                "#include <immintrin.h>",
            ],
            kind_filters: &[],
        },
    ),
    (
        "threading",
        ConceptPattern {
            label: "Threading / Parallelism",
            symbol_queries: &[
                "thread",
                "pthread",
                "mutex",
                "barrier",
                "atomic",
                "parallel",
                "task_queue",
                "worker",
                "pool",
            ],
            text_queries: &[
                "pthread_create",
                "pthread_mutex",
                "std::thread",
                "std::mutex",
                "omp_get_thread_num",
                "#pragma omp",
                "atomic_load",
                "atomic_store",
            ],
            kind_filters: &[],
        },
    ),
    (
        "gpu compute",
        ConceptPattern {
            label: "GPU / Compute",
            symbol_queries: &[
                "cuda",
                "metal",
                "vulkan",
                "opencl",
                "sycl",
                "kernel",
                "backend",
                "device",
                "queue",
                "command_buffer",
            ],
            text_queries: &[
                "cudaMalloc",
                "cudaMemcpy",
                "__global__",
                "MTLDevice",
                "vkCreateDevice",
                "clCreateContext",
                "ggml_backend",
            ],
            kind_filters: &[],
        },
    ),
    (
        "quantization",
        ConceptPattern {
            label: "Quantization",
            symbol_queries: &["quantize", "dequantize", "quant", "block_q", "ggml_type"],
            text_queries: &[
                "GGML_TYPE_Q",
                "block_q4",
                "block_q8",
                "quantize_row",
                "dequantize_row",
                "ggml_quantize",
            ],
            kind_filters: &[],
        },
    ),
    (
        "file io",
        ConceptPattern {
            label: "File I/O",
            symbol_queries: &[
                "read", "write", "open", "close", "fopen", "fread", "fwrite", "mmap", "stream",
                "buffer",
            ],
            text_queries: &[
                "fopen(",
                "fclose(",
                "fread(",
                "fwrite(",
                "std::ifstream",
                "std::ofstream",
                "open(",
                "read(",
                "write(",
            ],
            kind_filters: &["fn"],
        },
    ),
    (
        "string processing",
        ConceptPattern {
            label: "String Processing",
            symbol_queries: &[
                "string", "str", "format", "parse", "split", "token", "utf8", "encode", "decode",
            ],
            text_queries: &[
                "snprintf",
                "strlen",
                "strcmp",
                "strcat",
                "std::string",
                "string_view",
                "fmt::format",
            ],
            kind_filters: &[],
        },
    ),
];

/// Lightweight English word stemmer for concept matching.
/// Strips common suffixes so inflected queries ("errors", "handling", "serialization")
/// match concept keys ("error", "handling", "serialization").
pub fn stem_word(word: &str) -> String {
    let w = word.to_ascii_lowercase();
    // Longest suffixes first to avoid partial stripping.
    for (suffix, min_base) in &[
        ("ization", 3usize),
        ("isation", 3),
        ("ation", 3),
        ("tion", 3),
        ("sion", 3),
        ("ment", 3),
        ("ness", 3),
        ("ible", 3),
        ("able", 3),
        ("ize", 3),
        ("ise", 3),
        ("ing", 3),
        ("ed", 3),
        ("er", 3),
        ("ly", 3),
        ("es", 3),
        ("s", 3),
    ] {
        if let Some(base) = w.strip_suffix(suffix)
            && base.len() >= *min_base
            && !(*suffix == "s" && w.ends_with("ss"))
        {
            return base.to_string();
        }
    }
    w
}

/// Check whether two words match after stemming, using exact-stem or tight prefix overlap.
/// Prefix overlap (min 4 chars, max 2 char difference) handles -ing → base vs base+e
/// ("handl" ↔ "handle") without false positives like "data" ↔ "database".
fn stems_match(a: &str, b: &str) -> bool {
    let sa = stem_word(a);
    let sb = stem_word(b);
    if sa == sb {
        return true;
    }
    let min_len = sa.len().min(sb.len());
    let max_len = sa.len().max(sb.len());
    min_len >= 4
        && max_len - min_len <= 2
        && (sa.starts_with(sb.as_str()) || sb.starts_with(sa.as_str()))
}

/// How a query matched a concept: either an exact word-boundary hit or a looser
/// stemmed fallback. The header-rendering logic in the explore handler treats a
/// stem-only match more cautiously than an exact match (a single-word concept
/// matched only by stemming should not lead the header over the query's own
/// specific terms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConceptMatchKind {
    /// At least one window of query words matched the concept key verbatim
    /// (case-insensitive).
    Exact,
    /// No exact match; the concept key words matched query words only after
    /// stemming (e.g. "indexed" -> "indexing").
    Stemmed,
}

/// Find the best matching concept for a query.
/// Returns the matched key, the corresponding pattern, and how it matched
/// (exact word-boundary vs stemmed fallback), or `None` if no concept matches.
/// Uses word-boundary matching to avoid substring collisions (e.g. "clinical" should not match "cli").
/// Falls back to stemmed matching when exact words don't match.
pub fn match_concept(
    query: &str,
) -> Option<(&'static str, &'static ConceptPattern, ConceptMatchKind)> {
    let query_words: Vec<&str> = query.split_whitespace().collect();

    // Exact word-boundary match (original behavior).
    let exact = CONCEPT_MAP.iter().find(|(key, _)| {
        let key_words: Vec<&str> = key.split_whitespace().collect();
        query_words.windows(key_words.len()).any(|window| {
            window
                .iter()
                .zip(key_words.iter())
                .all(|(qw, kw)| qw.eq_ignore_ascii_case(kw))
        })
    });
    if let Some((key, pattern)) = exact {
        return Some((*key, pattern, ConceptMatchKind::Exact));
    }

    // Stemmed fallback with bag-of-words matching: each key word must match some query
    // word after stemming.  Order-independent so "handle errors" matches "error handling".
    CONCEPT_MAP
        .iter()
        .find(|(key, _)| {
            let key_words: Vec<&str> = key.split_whitespace().collect();
            key_words
                .iter()
                .all(|kw| query_words.iter().any(|qw| stems_match(qw, kw)))
        })
        .map(|(key, pattern)| (*key, pattern, ConceptMatchKind::Stemmed))
}

/// Return additional search terms for a concept based on the project's detected import roots.
/// Maps concept labels to known relevant crate/module names and returns any that are present
/// in the project but not already in the concept's symbol_queries.
pub fn enrich_concept_with_imports(
    concept: &ConceptPattern,
    project_imports: &[String],
) -> Vec<String> {
    let relevant: &[&str] = match concept.label {
        "Error Handling" => &["anyhow", "thiserror", "eyre", "miette", "color_eyre"],
        "Serialization" => &["serde", "postcard", "bincode", "rmp", "ciborium", "ron"],
        "Concurrency" => &["tokio", "rayon", "crossbeam", "parking_lot", "async_std"],
        "Logging / Observability" => &["tracing", "log", "slog", "env_logger", "opentelemetry"],
        "Caching" => &["moka", "cached", "lru", "dashmap"],
        "API / HTTP" => &["axum", "actix", "rocket", "warp", "tonic", "tower"],
        "Database" => &["sqlx", "diesel", "sea_orm", "rusqlite", "mongodb"],
        "CLI / Command Line" => &["clap", "structopt", "argh", "dialoguer"],
        "Parsing" => &["tree_sitter", "nom", "pest", "lalrpop", "winnow"],
        "Networking" => &["hyper", "reqwest", "tonic", "tower", "quinn"],
        "Testing" => &["proptest", "quickcheck", "rstest", "criterion", "insta"],
        "Authentication" => &["jsonwebtoken", "oauth2", "argon2", "bcrypt"],
        "Configuration" => &["config", "dotenvy", "figment", "toml", "serde_yaml"],
        "Deployment / Release" => &["release_please", "cargo_release", "semver"],
        "Permissions / Authorization" => &["casbin", "oso", "cedar"],
        "File Watching" => &["notify", "watchexec", "hotwatch"],
        "Indexing" => &["tantivy", "meilisearch", "sonic"],
        _ => &[],
    };

    let existing: std::collections::HashSet<&str> =
        concept.symbol_queries.iter().copied().collect();

    relevant
        .iter()
        .filter(|root| {
            project_imports
                .iter()
                .any(|imp| imp.eq_ignore_ascii_case(root))
                && !existing.iter().any(|e| e.eq_ignore_ascii_case(root))
        })
        .map(|s| s.to_string())
        .collect()
}

/// For queries that don't match a concept, split into search terms.
pub fn fallback_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != ':' && c != '-')
                .to_ascii_lowercase()
        })
        .filter(|w| w.len() >= 3)
        .filter(|w| !FALLBACK_STOP_WORDS.contains(&w.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_concept_finds_error_handling() {
        let concept = match_concept("error handling patterns");
        assert!(concept.is_some());
        assert_eq!(concept.unwrap().1.label, "Error Handling");
    }

    #[test]
    fn test_match_concept_case_insensitive() {
        let concept = match_concept("Error Handling");
        assert!(concept.is_some());
        assert_eq!(concept.unwrap().1.label, "Error Handling");
    }

    #[test]
    fn test_match_concept_returns_none_for_unknown() {
        let concept = match_concept("origami folding");
        assert!(concept.is_none());
    }

    #[test]
    fn test_fallback_terms_splits_query() {
        let terms = fallback_terms("process data handler");
        assert_eq!(terms, vec!["process", "data", "handler"]);
    }

    #[test]
    fn test_fallback_terms_filters_short_words() {
        let terms = fallback_terms("a bb ccc");
        assert_eq!(terms, vec!["ccc"]);
    }

    #[test]
    fn test_fallback_terms_filters_stop_words_and_punctuation() {
        let terms = fallback_terms("how does actor supervision and error recovery work?");
        assert_eq!(
            terms,
            vec!["actor", "supervision", "error", "recovery", "work"]
        );
    }

    #[test]
    fn test_match_concept_finds_actor_supervision() {
        let concept = match_concept("actor supervision and error recovery");
        assert!(concept.is_some());
        assert_eq!(concept.unwrap().1.label, "Actor Supervision");
    }

    #[test]
    fn test_match_concept_word_boundary_no_substring() {
        assert!(match_concept("clinical trial data").is_none());
        assert!(match_concept("capital investment").is_none());
        assert!(match_concept("cli tools").is_some());
        assert!(match_concept("api endpoints").is_some());
    }

    #[test]
    fn test_stem_word_common_suffixes() {
        assert_eq!(stem_word("errors"), "error");
        assert_eq!(stem_word("handling"), "handl");
        assert_eq!(stem_word("serialization"), "serial");
        assert_eq!(stem_word("serialize"), "serial");
        assert_eq!(stem_word("parsed"), "pars");
        assert_eq!(stem_word("caching"), "cach");
        assert_eq!(stem_word("deployments"), "deployment"); // strips -s first
    }

    #[test]
    fn test_stem_word_preserves_short_words() {
        assert_eq!(stem_word("cli"), "cli");
        assert_eq!(stem_word("api"), "api");
        assert_eq!(stem_word("log"), "log");
    }

    #[test]
    fn test_stems_match_inflected_variants() {
        assert!(stems_match("error", "errors"));
        assert!(stems_match("handle", "handling"));
        assert!(stems_match("parse", "parsing"));
        assert!(stems_match("serialize", "serialization"));
    }

    #[test]
    fn test_stems_match_rejects_short_prefix_overlap() {
        // "cli" vs "clinical" — short stem, should not prefix-match
        assert!(!stems_match("cli", "clinical"));
    }

    #[test]
    fn test_match_concept_stemmed_fallback() {
        // "handle errors" should match "error handling" via stemming
        let concept = match_concept("handle errors");
        assert!(concept.is_some());
        assert_eq!(concept.unwrap().1.label, "Error Handling");
    }

    #[test]
    fn test_match_concept_reports_match_provenance() {
        // Exact word-boundary hit reports ConceptMatchKind::Exact.
        let exact = match_concept("error handling patterns").expect("exact concept must match");
        assert_eq!(exact.0, "error handling");
        assert_eq!(exact.2, ConceptMatchKind::Exact);

        // The stem-misfire shape from the explore-header bug: "indexed" matches the
        // single-word "indexing" concept ONLY via stemming, never verbatim. The
        // provenance must be Stemmed so the header logic can demote it.
        let stemmed = match_concept("admission tiering decide which files get indexed")
            .expect("stemmed concept must match");
        assert_eq!(stemmed.0, "indexing");
        assert_eq!(stemmed.1.label, "Indexing");
        assert_eq!(stemmed.2, ConceptMatchKind::Stemmed);
    }

    #[test]
    fn test_match_concept_stemmed_serialized() {
        let concept = match_concept("serialized data");
        assert!(concept.is_some());
        assert_eq!(concept.unwrap().1.label, "Serialization");
    }

    #[test]
    fn test_match_concept_memory_allocation() {
        let concept = match_concept("memory allocation");
        assert!(concept.is_some(), "should match memory allocation concept");
        assert_eq!(concept.unwrap().1.label, "Memory Allocation");
    }

    #[test]
    fn test_match_concept_tensor_operations() {
        let concept = match_concept("tensor operations");
        assert!(concept.is_some(), "should match tensor ops concept");
        assert_eq!(concept.unwrap().1.label, "Tensor Operations");
    }

    #[test]
    fn test_match_concept_gpu_compute() {
        let concept = match_concept("gpu compute");
        assert!(concept.is_some(), "should match gpu concept");
        assert_eq!(concept.unwrap().1.label, "GPU / Compute");
    }

    #[test]
    fn test_match_concept_simd_vectorization() {
        let concept = match_concept("SIMD intrinsics");
        assert!(concept.is_some(), "should match simd concept");
        assert_eq!(concept.unwrap().1.label, "SIMD / Vectorization");
    }
}
