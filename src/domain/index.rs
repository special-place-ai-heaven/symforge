use std::collections::HashMap;
use std::fmt;

#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum LanguageId {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Swift,
    Kotlin,
    Dart,
    Perl,
    Elixir,
    Json,
    Toml,
    Yaml,
    Markdown,
    Env,
    Html,
    Css,
    Scss,
}

impl LanguageId {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" => Some(Self::Python),
            "js" | "jsx" => Some(Self::JavaScript),
            "ts" | "tsx" => Some(Self::TypeScript),
            "go" => Some(Self::Go),
            "java" => Some(Self::Java),
            "c" | "h" => Some(Self::C),
            "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => Some(Self::Cpp),
            "cs" => Some(Self::CSharp),
            "rb" => Some(Self::Ruby),
            "php" => Some(Self::Php),
            "swift" => Some(Self::Swift),
            "dart" => Some(Self::Dart),
            "kt" | "kts" => Some(Self::Kotlin),
            "pl" | "pm" => Some(Self::Perl),
            "ex" | "exs" => Some(Self::Elixir),
            "json" => Some(Self::Json),
            "toml" => Some(Self::Toml),
            "yaml" | "yml" => Some(Self::Yaml),
            "md" => Some(Self::Markdown),
            "env" => Some(Self::Env),
            "html" => Some(Self::Html),
            "css" => Some(Self::Css),
            "scss" => Some(Self::Scss),
            _ => None,
        }
    }

    /// Returns `true` when `relative_path` is a TSX source file (`.tsx`).
    ///
    /// `.tsx` and `.ts` both map to [`LanguageId::TypeScript`], but they require
    /// different tree-sitter grammars: `.tsx` needs the TSX grammar (JSX-aware,
    /// rejects legacy `<T>expr` casts) while `.ts` needs the plain TypeScript
    /// grammar (no JSX, accepts angle-bracket casts). The grammar is therefore
    /// selected from the file extension, not the [`LanguageId`], so this helper
    /// threads the TSX flavor to the parse sites.
    pub fn is_tsx_path(relative_path: &str) -> bool {
        relative_path
            .rsplit(['/', '\\'])
            .next()
            .and_then(|name| name.rsplit_once('.'))
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("tsx"))
    }

    pub fn extensions(&self) -> &[&str] {
        match self {
            Self::Rust => &["rs"],
            Self::Python => &["py"],
            Self::JavaScript => &["js", "jsx"],
            Self::TypeScript => &["ts", "tsx"],
            Self::Go => &["go"],
            Self::Java => &["java"],
            Self::C => &["c", "h"],
            Self::Cpp => &["cpp", "cxx", "cc", "hpp", "hxx", "hh"],
            Self::CSharp => &["cs"],
            Self::Ruby => &["rb"],
            Self::Php => &["php"],
            Self::Swift => &["swift"],
            Self::Kotlin => &["kt", "kts"],
            Self::Dart => &["dart"],
            Self::Perl => &["pl", "pm"],
            Self::Elixir => &["ex", "exs"],
            Self::Json => &["json"],
            Self::Toml => &["toml"],
            Self::Yaml => &["yaml", "yml"],
            Self::Markdown => &["md"],
            Self::Env => &["env"],
            Self::Html => &["html"],
            Self::Css => &["css"],
            Self::Scss => &["scss"],
        }
    }

    pub fn support_tier(&self) -> SupportTier {
        match self {
            Self::Rust | Self::Python | Self::JavaScript | Self::TypeScript | Self::Go => {
                SupportTier::QualityFocus
            }
            Self::Java
            | Self::C
            | Self::Cpp
            | Self::CSharp
            | Self::Ruby
            | Self::Php
            | Self::Swift
            | Self::Kotlin
            | Self::Dart
            | Self::Perl
            | Self::Elixir
            | Self::Json
            | Self::Toml
            | Self::Yaml
            | Self::Markdown
            | Self::Env
            | Self::Html
            | Self::Css
            | Self::Scss => SupportTier::Broader,
        }
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Swift => "Swift",
            Self::Kotlin => "Kotlin",
            Self::Dart => "Dart",
            Self::Perl => "Perl",
            Self::Elixir => "Elixir",
            Self::Json => "JSON",
            Self::Toml => "TOML",
            Self::Yaml => "YAML",
            Self::Markdown => "Markdown",
            Self::Env => "Env",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Scss => "SCSS",
        };
        write!(f, "{name}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupportTier {
    QualityFocus,
    Broader,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FileClass {
    Code,
    Text,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FileClassification {
    pub class: FileClass,
    pub is_generated: bool,
    pub is_test: bool,
    pub is_vendor: bool,
    #[serde(default)]
    pub is_config: bool,
}

impl FileClassification {
    pub fn for_code_path(relative_path: &str) -> Self {
        let lower = relative_path.replace('\\', "/").to_ascii_lowercase();
        let segments: Vec<&str> = lower
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        let basename = segments.last().copied().unwrap_or("");
        let stem = basename
            .rsplit_once('.')
            .map(|(name, _)| name)
            .unwrap_or(basename);

        let is_test = segments
            .iter()
            .any(|segment| matches!(*segment, "tests" | "test" | "__tests__" | "spec"))
            || stem.starts_with("test_")
            || stem.ends_with("_test")
            || stem.ends_with(".test")
            || stem.ends_with("_spec")
            || stem.ends_with(".spec");

        let is_vendor = segments.iter().any(|segment| {
            matches!(
                *segment,
                "vendor"
                    | "third_party"
                    | "third-party"
                    | "node_modules"
                    | ".venv"
                    | "venv"
                    | "site-packages"
                    | "pods"
            )
        });

        let is_generated = segments.iter().any(|segment| {
            matches!(
                *segment,
                "generated" | "__generated__" | "generated-sources"
            )
        }) || basename.contains(".generated.")
            || basename.contains(".gen.")
            || basename.ends_with(".g.dart")
            || basename.ends_with(".pb.go")
            || basename.ends_with(".designer.cs")
            || basename.ends_with(".min.js");

        let ext = basename.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        let is_config = matches!(ext, "json" | "toml" | "yaml" | "yml" | "md" | "env");

        Self {
            class: FileClass::Code,
            is_generated,
            is_test,
            is_vendor,
            is_config,
        }
    }

    pub const fn is_code(&self) -> bool {
        matches!(self.class, FileClass::Code)
    }

    pub const fn is_text(&self) -> bool {
        matches!(self.class, FileClass::Text)
    }

    pub const fn is_binary(&self) -> bool {
        matches!(self.class, FileClass::Binary)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ParseDiagnostic {
    pub parser: String,
    pub message: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub byte_span: Option<(u32, u32)>,
    pub fallback_used: bool,
}

impl ParseDiagnostic {
    pub fn location_display(&self) -> Option<String> {
        match (self.line, self.column) {
            (Some(line), Some(column)) => Some(format!("line {line}, column {column}")),
            (Some(line), None) => Some(format!("line {line}")),
            (None, Some(column)) => Some(format!("column {column}")),
            (None, None) => None,
        }
    }

    pub fn summary(&self) -> String {
        let mut summary = format!("{}: {}", self.parser, self.message);
        if let Some(location) = self.location_display() {
            summary.push_str(&format!(" ({location})"));
        }
        if self.fallback_used {
            summary.push_str(" [fallback symbol extraction used]");
        }
        summary
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileProcessingResult {
    pub relative_path: String,
    pub language: LanguageId,
    pub classification: FileClassification,
    pub outcome: FileOutcome,
    pub parse_diagnostic: Option<ParseDiagnostic>,
    pub symbols: Vec<SymbolRecord>,
    pub byte_len: u64,
    pub content_hash: String,
    /// Cross-references extracted by `parsing::xref::extract_references`.
    /// Empty until Task 2 wires xref extraction into the parse pipeline.
    pub references: Vec<ReferenceRecord>,
    /// Import alias map for this file: alias -> original name (e.g. "Map" -> "HashMap").
    pub alias_map: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileOutcome {
    Processed,
    PartialParse { warning: String },
    Failed { error: String },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymbolRecord {
    pub name: String,
    pub kind: SymbolKind,
    pub depth: u32,
    pub sort_order: u32,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub doc_byte_range: Option<(u32, u32)>,
    #[serde(default)]
    pub item_byte_range: Option<(u32, u32)>,
}

impl SymbolRecord {
    /// Returns the start byte of the full editable item when available.
    pub fn item_start(&self) -> u32 {
        self.item_byte_range.map_or_else(
            || {
                self.doc_byte_range
                    .map_or(self.byte_range.0, |(start, _)| start)
            },
            |(start, _)| start,
        )
    }

    /// Returns the end byte of the full editable item when available.
    pub fn item_end(&self) -> u32 {
        self.item_byte_range
            .map_or(self.byte_range.1, |(_, end)| end)
    }

    /// Returns the full editable item range when available.
    pub fn item_range(&self) -> (u32, u32) {
        (self.item_start(), self.item_end())
    }

    /// Returns the parser's core symbol node range.
    pub fn core_range(&self) -> (u32, u32) {
        self.byte_range
    }

    /// Returns the effective start byte, including doc comments if present.
    pub fn effective_start(&self) -> u32 {
        self.doc_byte_range
            .map_or(self.byte_range.0, |(start, _)| start)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Module,
    Constant,
    Variable,
    Type,
    Trait,
    Impl,
    Other,
    Key,
    Section,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = match self {
            Self::Function => "fn",
            Self::Method => "fn",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Module => "mod",
            Self::Constant => "const",
            Self::Variable => "let",
            Self::Type => "type",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Other => "other",
            Self::Key => "key",
            Self::Section => "section",
        };
        write!(f, "{prefix}")
    }
}

/// A single cross-reference (call site, import, type usage, or macro use) extracted
/// from a source file. Part of the Phase 4 cross-reference pipeline.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReferenceRecord {
    /// Simple name at the reference site (e.g. "new", "process", "HashMap").
    pub name: String,
    /// Best-effort qualified name when available (e.g. "Vec::new", "fmt.Println").
    pub qualified_name: Option<String>,
    /// What kind of reference this is.
    pub kind: ReferenceKind,
    /// Byte range in the source file (start, end).
    pub byte_range: (u32, u32),
    /// Line range in the source file (start, end — zero-indexed).
    pub line_range: (u32, u32),
    /// Index into the file's symbol list for the innermost containing definition.
    /// `None` means the reference is at module/top level.
    pub enclosing_symbol_index: Option<u32>,
}

/// Discriminates the semantic role of a cross-reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReferenceKind {
    /// A function or method call site.
    Call,
    /// An import/use/require statement.
    Import,
    /// A type annotation, generic parameter, or other type usage.
    TypeUsage,
    /// A macro invocation.
    MacroUse,
    /// A trait/interface implementation relationship.
    /// `name` holds the trait/interface name, `qualified_name` holds the implementing type.
    Implements,
    /// A read of a named `const`/`static` value at a value-expression site
    /// (e.g. iterated in a `for` loop, used as a receiver, or passed as a bare
    /// argument). Distinct from `Call` so callee/type analysis stays clean.
    ValueUse,
}

impl fmt::Display for ReferenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Call => "call",
            Self::Import => "import",
            Self::TypeUsage => "type_usage",
            Self::MacroUse => "macro_use",
            Self::Implements => "implements",
            Self::ValueUse => "value_use",
        };
        write!(f, "{s}")
    }
}

/// Returns the index of the innermost `SymbolRecord` whose `line_range` contains
/// `ref_line`, or `None` if the reference is at module level.
///
/// "Innermost" is defined as the symbol with the latest `line_range.0` (start line)
/// that still contains `ref_line`. This handles nested function definitions correctly.
pub fn find_enclosing_symbol(symbols: &[SymbolRecord], ref_line: u32) -> Option<u32> {
    let mut best: Option<(u32, u32)> = None; // (start_line, index)
    for (idx, sym) in symbols.iter().enumerate() {
        let (start, end) = sym.line_range;
        if ref_line >= start && ref_line <= end {
            match best {
                None => best = Some((start, idx as u32)),
                Some((best_start, _)) if start > best_start => best = Some((start, idx as u32)),
                _ => {}
            }
        }
    }
    best.map(|(_, idx)| idx)
}

/// Admission tier — whether a file is eligible for indexing/parsing at all.
/// Separate from NoiseClass (which is about ranking/filtering signal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdmissionTier {
    /// Tier 1: Fully indexed — parsed, symbols extracted, text searchable.
    Normal,
    /// Tier 2: Metadata only — path, size, classification stored. No parsing.
    MetadataOnly,
    /// Tier 3: Hard-skipped — counted in health, minimal registration.
    HardSkip,
}

/// Reason a file was placed in Tier 2 or Tier 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    SizeCeiling,
    DenylistedExtension,
    SizeThreshold,
    BinaryContent,
    /// File is a dependency lockfile (e.g. `package-lock.json`, `Cargo.lock`).
    /// Demoted to Tier-2 metadata-only so its machine-generated content does not
    /// flood the index with thousands of junk key/value symbols (a single
    /// `package-lock.json` can mint ~9k JSON-key symbols, dominating symbol counts
    /// and `conventions` complexity heuristics). The path stays searchable as
    /// metadata; only symbol extraction is skipped. See `LOCKFILE_BASENAMES`.
    DependencyLockfile,
    /// SF-009: file demoted to Tier-2 because it is not git-tracked, under the
    /// opt-in `SYMFORGE_EXCLUDE_UNTRACKED` policy (default OFF). Only minted when
    /// that env gate is explicitly enabled; the default admission path never
    /// produces this reason, so admission defaults are unchanged.
    Untracked,
    /// SF-004 / SF-012: file exists on disk and is small/non-binary, but its
    /// extension maps to no supported tree-sitter grammar (e.g. `.tcl`, `.sh`,
    /// `.m`, `.eex`, extensionless `LICENSE`/`Makefile`). It cannot be parsed, so
    /// it is admitted Tier-2 metadata-only instead of being stored with a
    /// contradictory Tier-1/Normal decision (which made it vanish from tier
    /// accounting and minted a false "File not found"). The path stays searchable
    /// as metadata; only symbol extraction is skipped.
    UnsupportedLanguage,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkipReason::SizeCeiling => write!(f, ">100MB"),
            SkipReason::DenylistedExtension => write!(f, "artifact"),
            SkipReason::SizeThreshold => write!(f, ">1MB"),
            SkipReason::BinaryContent => write!(f, "binary"),
            SkipReason::DependencyLockfile => write!(f, "lockfile"),
            SkipReason::Untracked => write!(f, "untracked"),
            SkipReason::UnsupportedLanguage => write!(f, "unsupported language"),
        }
    }
}

/// Structured result from the admission gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionDecision {
    pub tier: AdmissionTier,
    pub reason: Option<SkipReason>,
}

impl AdmissionDecision {
    pub fn normal() -> Self {
        Self {
            tier: AdmissionTier::Normal,
            reason: None,
        }
    }
    pub fn skip(tier: AdmissionTier, reason: SkipReason) -> Self {
        Self {
            tier,
            reason: Some(reason),
        }
    }
}

/// Metadata record for a file that was not fully indexed (Tier 2 or Tier 3).
/// Stores the AdmissionDecision directly — no re-derivation downstream.
#[derive(Debug, Clone)]
pub struct SkippedFile {
    pub path: String,
    pub size: u64,
    pub extension: Option<String>,
    pub decision: AdmissionDecision,
}

impl SkippedFile {
    pub fn tier(&self) -> AdmissionTier {
        self.decision.tier
    }
    pub fn reason(&self) -> Option<SkipReason> {
        self.decision.reason
    }
}

pub const HARD_SKIP_BYTES: u64 = 100 * 1024 * 1024;
pub const METADATA_ONLY_BYTES: u64 = 1024 * 1024;
pub const BINARY_SNIFF_BYTES: usize = 8192;

const DENYLISTED_EXTENSIONS: &[&str] = &[
    // ML models
    "safetensors",
    "ckpt",
    "pt",
    "onnx",
    "gguf",
    "pth",
    // VM/disk images
    "vmdk",
    "iso",
    "img",
    "qcow2",
    // Archives
    "tar",
    "gz",
    "zip",
    "7z",
    "rar",
    "bz2",
    "xz",
    "zst",
    // Databases
    "db",
    "sqlite",
    "sqlite3",
    "mdb",
    // Media
    "mp3",
    "mp4",
    "wav",
    "avi",
    "mov",
    "mkv",
    "png",
    "jpg",
    "jpeg",
    "gif",
    "bmp",
    "ico",
    "woff",
    "woff2",
    "ttf",
    "eot",
    // Binary
    "bin",
    // Executables and libraries
    "exe",
    "dll",
    "so",
    "dylib",
    "class",
];

pub fn is_denylisted_extension(ext: &str) -> bool {
    DENYLISTED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Dependency lockfiles matched by exact basename (case-sensitive, as these
/// names are conventionally fixed). These are machine-generated manifests whose
/// content (resolved dependency trees) parses into thousands of structurally
/// meaningless symbols — e.g. a single `package-lock.json` can mint ~9k JSON-key
/// symbols, dwarfing real source and skewing `conventions` complexity stats.
/// Admission demotes them to Tier-2 (metadata only): the path stays searchable,
/// but no symbols are extracted.
///
/// Cross-reference: the search ranker keeps an OVERLAPPING but distinct list,
/// `CHORE_ANCHOR_FILENAMES` in `src/live_index/rank_signals.rs`, used to suppress
/// co-change promotion. That list is intentionally NOT shared: it also contains
/// non-lockfile chore anchors (`CHANGELOG.md`, `.release-please-manifest.json`)
/// and lives in the `live_index` layer, which `domain` must not depend on. The
/// two lists answer different questions (admission vs. ranking) and are kept
/// separate on purpose; update both when the lockfile ecosystem changes.
const LOCKFILE_BASENAMES: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "Cargo.lock",
    "composer.lock",
    "Gemfile.lock",
    "poetry.lock",
    "uv.lock",
    "Pipfile.lock",
    "flake.lock",
    "packages.lock.json",
    "go.sum",
];

/// Returns `true` when `path`'s file name is an exact (case-sensitive) match for
/// a known dependency lockfile. See [`LOCKFILE_BASENAMES`].
pub fn is_dependency_lockfile(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| LOCKFILE_BASENAMES.contains(&name))
}

/// Maximum number of NUL byte offsets [`scan_nul_bytes`] enumerates explicitly.
/// Beyond this we report only the count plus the first few offsets — the warning
/// exists to alert, not to exhaustively map every NUL.
pub const NUL_OFFSET_SAMPLE_LIMIT: usize = 5;

/// Result of scanning a byte slice for literal NUL (`0x00`) bytes.
///
/// A NUL is valid UTF-8, so it survives `String::from_utf8_lossy` intact and is
/// then rendered invisibly (most terminals show nothing or a space). Source code
/// that smuggles a NUL into a string/template literal (legal for Node, Python,
/// etc.) therefore reads back as text that does NOT byte-match the file — an
/// agent copying that rendered text for an edit produces a mismatch. This scan
/// powers an honest warning at the content-rendering boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NulScan {
    /// Total count of NUL bytes in the scanned slice.
    pub count: usize,
    /// Byte offsets (relative to the scanned slice) of the first
    /// [`NUL_OFFSET_SAMPLE_LIMIT`] NUL bytes, in ascending order.
    pub first_offsets: Vec<usize>,
}

impl NulScan {
    /// `true` when the scanned slice contained at least one NUL byte.
    pub fn has_nul(&self) -> bool {
        self.count > 0
    }
}

/// Scan `content` for literal NUL (`0x00`) bytes, returning a count and the
/// first [`NUL_OFFSET_SAMPLE_LIMIT`] offsets. Pure; scans the WHOLE slice it is
/// given (callers pass the exact bytes whose rendering they are warning about).
pub fn scan_nul_bytes(content: &[u8]) -> NulScan {
    let mut count = 0usize;
    let mut first_offsets = Vec::new();
    for (offset, &byte) in content.iter().enumerate() {
        if byte == 0 {
            count += 1;
            if first_offsets.len() < NUL_OFFSET_SAMPLE_LIMIT {
                first_offsets.push(offset);
            }
        }
    }
    NulScan {
        count,
        first_offsets,
    }
}

/// Render a single-line warning describing NUL bytes found by [`scan_nul_bytes`],
/// or `None` when the slice is clean. The message is deliberately blunt: copied
/// rendered text will not byte-match the file, so edits must use byte-exact tools.
pub fn nul_byte_warning_line(scan: &NulScan) -> Option<String> {
    if !scan.has_nul() {
        return None;
    }
    let offsets = scan
        .first_offsets
        .iter()
        .map(|o| o.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let plural = if scan.count == 1 { "" } else { "s" };
    let offset_suffix = if scan.count > scan.first_offsets.len() {
        format!("{offsets}, ...")
    } else {
        offsets
    };
    Some(format!(
        "WARNING: this file contains {} NUL byte{plural} (0x00) at byte offset(s) {offset_suffix}. \
         They are rendered invisibly above; copied text will NOT byte-match the file. \
         Edit this file with byte-exact tools, not rendered text.",
        scan.count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_display_function() {
        assert_eq!(SymbolKind::Function.to_string(), "fn");
    }

    #[test]
    fn test_symbol_kind_display_method() {
        assert_eq!(SymbolKind::Method.to_string(), "fn");
    }

    #[test]
    fn test_symbol_kind_display_class() {
        assert_eq!(SymbolKind::Class.to_string(), "class");
    }

    #[test]
    fn test_symbol_kind_display_struct() {
        assert_eq!(SymbolKind::Struct.to_string(), "struct");
    }

    #[test]
    fn test_symbol_kind_display_enum() {
        assert_eq!(SymbolKind::Enum.to_string(), "enum");
    }

    #[test]
    fn test_symbol_kind_display_interface() {
        assert_eq!(SymbolKind::Interface.to_string(), "interface");
    }

    #[test]
    fn test_symbol_kind_display_module() {
        assert_eq!(SymbolKind::Module.to_string(), "mod");
    }

    #[test]
    fn test_symbol_kind_display_constant() {
        assert_eq!(SymbolKind::Constant.to_string(), "const");
    }

    #[test]
    fn test_symbol_kind_display_variable() {
        assert_eq!(SymbolKind::Variable.to_string(), "let");
    }

    #[test]
    fn test_symbol_kind_display_type() {
        assert_eq!(SymbolKind::Type.to_string(), "type");
    }

    #[test]
    fn test_symbol_kind_display_trait() {
        assert_eq!(SymbolKind::Trait.to_string(), "trait");
    }

    #[test]
    fn test_symbol_kind_display_impl() {
        assert_eq!(SymbolKind::Impl.to_string(), "impl");
    }

    #[test]
    fn test_symbol_kind_display_other() {
        assert_eq!(SymbolKind::Other.to_string(), "other");
    }

    // --- ReferenceRecord and ReferenceKind ---

    #[test]
    fn test_reference_kind_all_variants_constructible() {
        let _call = ReferenceKind::Call;
        let _import = ReferenceKind::Import;
        let _type_usage = ReferenceKind::TypeUsage;
        let _macro_use = ReferenceKind::MacroUse;
    }

    #[test]
    fn test_reference_kind_display_call() {
        assert_eq!(ReferenceKind::Call.to_string(), "call");
    }

    #[test]
    fn test_reference_kind_display_import() {
        assert_eq!(ReferenceKind::Import.to_string(), "import");
    }

    #[test]
    fn test_reference_kind_display_type_usage() {
        assert_eq!(ReferenceKind::TypeUsage.to_string(), "type_usage");
    }

    #[test]
    fn test_reference_kind_display_macro_use() {
        assert_eq!(ReferenceKind::MacroUse.to_string(), "macro_use");
    }

    #[test]
    fn test_reference_record_construction_with_all_fields() {
        let r = ReferenceRecord {
            name: "foo".to_string(),
            qualified_name: Some("Bar::foo".to_string()),
            kind: ReferenceKind::Call,
            byte_range: (10, 20),
            line_range: (1, 1),
            enclosing_symbol_index: Some(0),
        };
        assert_eq!(r.name, "foo");
        assert_eq!(r.qualified_name.as_deref(), Some("Bar::foo"));
        assert_eq!(r.kind, ReferenceKind::Call);
        assert_eq!(r.byte_range, (10, 20));
        assert_eq!(r.line_range, (1, 1));
        assert_eq!(r.enclosing_symbol_index, Some(0));
    }

    #[test]
    fn test_reference_record_without_optional_fields() {
        let r = ReferenceRecord {
            name: "baz".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Import,
            byte_range: (0, 5),
            line_range: (0, 0),
            enclosing_symbol_index: None,
        };
        assert!(r.qualified_name.is_none());
        assert!(r.enclosing_symbol_index.is_none());
    }

    #[test]
    fn test_file_processing_result_backward_compat_with_empty_refs() {
        use std::collections::HashMap;
        let result = FileProcessingResult {
            relative_path: "test.rs".to_string(),
            language: LanguageId::Rust,
            classification: FileClassification::for_code_path("test.rs"),
            outcome: FileOutcome::Processed,
            parse_diagnostic: None,
            symbols: vec![],
            byte_len: 0,
            content_hash: "abc".to_string(),
            references: vec![],
            alias_map: HashMap::new(),
        };
        assert!(result.references.is_empty());
        assert!(result.alias_map.is_empty());
    }

    #[test]
    fn test_file_classification_for_code_path_marks_code_only_by_default() {
        let classification = FileClassification::for_code_path("src/lib.rs");

        assert!(classification.is_code());
        assert!(!classification.is_text());
        assert!(!classification.is_binary());
        assert!(!classification.is_generated);
        assert!(!classification.is_test);
        assert!(!classification.is_vendor);
    }

    #[test]
    fn test_file_classification_for_code_path_marks_noise_tags_from_path() {
        let generated = FileClassification::for_code_path("src/generated/client.pb.go");
        assert!(generated.is_generated);

        let test_file = FileClassification::for_code_path("tests/parser_spec.rs");
        assert!(test_file.is_test);

        let vendor_file = FileClassification::for_code_path("node_modules/pkg/index.js");
        assert!(vendor_file.is_vendor);
    }

    #[test]
    fn test_find_enclosing_symbol_innermost_for_nested() {
        // outer: line 0..10, inner: line 3..6
        let symbols = vec![
            SymbolRecord {
                name: "outer".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 100),
                line_range: (0, 10),
                doc_byte_range: None,
                item_byte_range: None,
            },
            SymbolRecord {
                name: "inner".to_string(),
                kind: SymbolKind::Function,
                depth: 1,
                sort_order: 1,
                byte_range: (30, 60),
                line_range: (3, 6),
                doc_byte_range: None,
                item_byte_range: None,
            },
        ];
        // Reference at line 4 is inside both - should return inner (index 1)
        let idx = find_enclosing_symbol(&symbols, 4);
        assert_eq!(idx, Some(1), "should return innermost enclosing symbol");
    }

    #[test]
    fn test_find_enclosing_symbol_none_at_module_level() {
        let symbols = vec![SymbolRecord {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (50, 100),
            line_range: (5, 10),
            doc_byte_range: None,
            item_byte_range: None,
        }];
        // Reference at line 0 is not inside any symbol
        let idx = find_enclosing_symbol(&symbols, 0);
        assert_eq!(idx, None, "should return None when not inside any symbol");
    }

    #[test]
    fn test_symbol_record_item_helpers_fall_back_to_doc_range() {
        let symbol = SymbolRecord {
            name: "target".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (20, 40),
            line_range: (1, 3),
            doc_byte_range: Some((10, 20)),
            item_byte_range: None,
        };

        assert_eq!(symbol.item_start(), 10);
        assert_eq!(symbol.item_end(), 40);
        assert_eq!(symbol.item_range(), (10, 40));
        assert_eq!(symbol.core_range(), (20, 40));
    }

    #[test]
    fn test_symbol_record_item_helpers_prefer_item_range() {
        let symbol = SymbolRecord {
            name: "target".to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (20, 40),
            line_range: (1, 3),
            doc_byte_range: Some((10, 20)),
            item_byte_range: Some((5, 44)),
        };

        assert_eq!(symbol.item_start(), 5);
        assert_eq!(symbol.item_end(), 44);
        assert_eq!(symbol.item_range(), (5, 44));
        assert_eq!(symbol.core_range(), (20, 40));
    }

    #[test]
    fn test_admission_tier_variants() {
        let t1 = AdmissionTier::Normal;
        let t2 = AdmissionTier::MetadataOnly;
        let t3 = AdmissionTier::HardSkip;
        assert_ne!(t1, t2);
        assert_ne!(t2, t3);
        assert_ne!(t1, t3);
    }

    #[test]
    fn test_extension_is_denylisted() {
        assert!(is_denylisted_extension("safetensors"));
        assert!(is_denylisted_extension("ckpt"));
        assert!(is_denylisted_extension("zip"));
        assert!(is_denylisted_extension("mp4"));
        assert!(is_denylisted_extension("woff2"));
        assert!(is_denylisted_extension("png"));
        assert!(is_denylisted_extension("bin"));
    }

    #[test]
    fn test_extension_not_denylisted() {
        assert!(!is_denylisted_extension("rs"));
        assert!(!is_denylisted_extension("ts"));
        assert!(!is_denylisted_extension("json"));
        assert!(!is_denylisted_extension("svg")); // SVG intentionally NOT denylisted
        assert!(!is_denylisted_extension("md"));
        assert!(!is_denylisted_extension("toml"));
    }

    #[test]
    fn test_new_executable_extensions_denylisted() {
        assert!(is_denylisted_extension("exe"));
        assert!(is_denylisted_extension("dll"));
        assert!(is_denylisted_extension("so"));
        assert!(is_denylisted_extension("dylib"));
        assert!(is_denylisted_extension("class"));
    }

    #[test]
    fn test_denylist_case_insensitive() {
        assert!(is_denylisted_extension("DLL"));
        assert!(is_denylisted_extension("So"));
        assert!(is_denylisted_extension("EXE"));
        assert!(is_denylisted_extension("Dylib"));
        assert!(is_denylisted_extension("CLASS"));
    }

    #[test]
    fn test_skipped_file_creation() {
        let decision =
            AdmissionDecision::skip(AdmissionTier::MetadataOnly, SkipReason::DenylistedExtension);
        let sf = SkippedFile {
            path: "model.safetensors".into(),
            size: 4_200_000_000,
            extension: Some("safetensors".into()),
            decision,
        };
        assert_eq!(sf.tier(), AdmissionTier::MetadataOnly);
        assert_eq!(sf.reason(), Some(SkipReason::DenylistedExtension));
    }

    #[test]
    fn test_scan_nul_bytes_clean_slice_reports_none() {
        let scan = scan_nul_bytes(b"fn main() {}\n");
        assert!(!scan.has_nul());
        assert_eq!(scan.count, 0);
        assert!(scan.first_offsets.is_empty());
        assert_eq!(nul_byte_warning_line(&scan), None);
    }

    #[test]
    fn test_scan_nul_bytes_empty_slice_reports_none() {
        let scan = scan_nul_bytes(b"");
        assert!(!scan.has_nul());
        assert_eq!(nul_byte_warning_line(&scan), None);
    }

    #[test]
    fn test_scan_nul_bytes_single_offset_and_singular_warning() {
        let scan = scan_nul_bytes(b"ab\0cd");
        assert_eq!(scan.count, 1);
        assert_eq!(scan.first_offsets, vec![2]);
        let warning = nul_byte_warning_line(&scan).expect("NUL present");
        assert!(warning.contains("1 NUL byte (0x00)"), "warning: {warning}");
        assert!(
            warning.contains("byte offset(s) 2."),
            "single offset, no ellipsis: {warning}"
        );
        assert!(warning.contains("byte-exact tools"));
    }

    #[test]
    fn test_scan_nul_bytes_plural_warning() {
        let scan = scan_nul_bytes(b"\0a\0b");
        assert_eq!(scan.count, 2);
        assert_eq!(scan.first_offsets, vec![0, 2]);
        let warning = nul_byte_warning_line(&scan).expect("NUL present");
        assert!(warning.contains("2 NUL bytes (0x00)"), "warning: {warning}");
        assert!(
            warning.contains("byte offset(s) 0, 2."),
            "warning: {warning}"
        );
    }

    #[test]
    fn test_scan_nul_bytes_caps_offsets_and_adds_ellipsis() {
        // 7 NUL bytes; only the first NUL_OFFSET_SAMPLE_LIMIT (5) are enumerated.
        let scan = scan_nul_bytes(b"\0\0\0\0\0\0\0");
        assert_eq!(scan.count, 7);
        assert_eq!(scan.first_offsets.len(), NUL_OFFSET_SAMPLE_LIMIT);
        assert_eq!(scan.first_offsets, vec![0, 1, 2, 3, 4]);
        let warning = nul_byte_warning_line(&scan).expect("NUL present");
        assert!(warning.contains("7 NUL bytes"), "warning: {warning}");
        assert!(
            warning.contains("0, 1, 2, 3, 4, ..."),
            "more NULs than sampled => ellipsis: {warning}"
        );
    }
}
