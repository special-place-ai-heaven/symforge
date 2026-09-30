/// Trigram index for file-level text search acceleration.
///
/// Maps 3-byte sequences (trigrams) to posting lists of file IDs.
/// Search uses AND-intersection of posting lists to find candidate files,
/// then verifies each candidate contains the full query bytes.
///
/// Short queries (< 3 bytes) fall back to linear scan of file content.
use std::collections::{HashMap, HashSet};

use crate::live_index::store::IndexedFile;

/// Trigram-based posting list index.
///
/// `map`: trigram -> sorted Vec of file IDs (deduped per file).
/// `id_to_path`: file_id -> relative_path for result lookup.
/// `path_to_id`: relative_path -> file_id for update/remove.
/// `next_id`: auto-increment counter for fresh IDs.
#[derive(Clone)]
pub struct TrigramIndex {
    map: HashMap<[u8; 3], Vec<u64>>,
    id_to_path: HashMap<u64, String>,
    path_to_id: HashMap<String, u64>,
    next_id: u64,
}

impl TrigramIndex {
    /// Create an empty trigram index.
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            id_to_path: HashMap::new(),
            path_to_id: HashMap::new(),
            next_id: 0,
        }
    }

    /// Build a trigram index from all files in the given map.
    pub fn build_from_files<V>(files: &HashMap<String, V>) -> Self
    where
        V: AsRef<IndexedFile>,
    {
        let mut idx = Self::new();
        for (path, file) in files {
            idx.insert_file(path, &file.as_ref().content);
        }
        idx
    }

    /// Search for files containing all bytes in `query`.
    ///
    /// If `query.len() < 3`, falls back to linear scan of `files`.
    /// Otherwise uses AND-intersection of trigram posting lists,
    /// then verifies each candidate with a byte-level contains check.
    ///
    /// Returns a `Vec<String>` of matching relative paths.
    pub fn search<V>(&self, query: &[u8], files: &HashMap<String, V>) -> Vec<String>
    where
        V: AsRef<IndexedFile>,
    {
        if query.is_empty() {
            return Vec::new();
        }

        // Lowercase query once — reused for both trigram extraction and verification.
        let query_lower: Vec<u8> = query.iter().map(|b| b.to_ascii_lowercase()).collect();

        if query_lower.len() < 3 {
            // Fall back to linear scan for short queries
            return self.linear_scan_lower(&query_lower, files);
        }

        // Trigrams are extracted from already-lowercased query; index stores lowercase trigrams.
        let trigrams = extract_trigrams(&query_lower);

        // Collect posting lists for each trigram in the query
        let mut posting_lists: Vec<&Vec<u64>> =
            trigrams.iter().filter_map(|t| self.map.get(t)).collect();

        if posting_lists.is_empty() {
            // A trigram from the query has no matches at all
            return Vec::new();
        }

        // Sort posting lists by length — start intersection with smallest list
        posting_lists.sort_by_key(|l| l.len());

        // AND-intersection: start with shortest list
        let mut candidates: Vec<u64> = posting_lists[0].clone();
        for list in &posting_lists[1..] {
            candidates.retain(|id| list.binary_search(id).is_ok());
            if candidates.is_empty() {
                return Vec::new();
            }
        }

        // Verify candidates actually contain the query (eliminates false positives).
        // Uses allocation-free CI scan; query_lower was computed once above.
        candidates
            .into_iter()
            .filter_map(|id| {
                let path = self.id_to_path.get(&id)?;
                let file = files.get(path)?;
                if contains_bytes_ci(&file.as_ref().content, &query_lower) {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Replace the trigrams of `path` with those of `content`.
    ///
    /// `old` is the content this index last indexed for `path`. With it only
    /// the trigrams that vanished or appeared are touched, so the cost follows
    /// the file rather than the whole map, and shared trigrams stay put. A
    /// tracked path without `old` falls back to scanning every posting list.
    /// The file ID is stable per path.
    pub fn update_file(&mut self, path: &str, old: Option<&[u8]>, content: &[u8]) {
        let Some(&file_id) = self.path_to_id.get(path) else {
            self.insert_file(path, content);
            return;
        };
        let Some(old) = old else {
            self.remove_id_everywhere(file_id);
            self.insert_file(path, content);
            return;
        };
        let old = trigrams_of(old);
        let new = trigrams_of(content);
        for tg in old.difference(&new) {
            self.remove_id(tg, file_id);
        }
        for tg in new.difference(&old) {
            insert_sorted(self.map.entry(*tg).or_default(), file_id);
        }
    }

    /// Forget `path`. `content` is what it was indexed with, whose trigrams
    /// are the only posting lists touched; `None` scans every posting list,
    /// for when that content is not known.
    pub fn remove_file(&mut self, path: &str, content: Option<&[u8]>) {
        let Some(id) = self.path_to_id.remove(path) else {
            return;
        };
        self.id_to_path.remove(&id);
        match content {
            Some(content) => {
                for tg in trigrams_of(content) {
                    self.remove_id(&tg, id);
                }
            }
            None => self.remove_id_everywhere(id),
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Linear scan through all files. `query_lower` must already be ASCII-lowercased.
    /// Uses allocation-free CI byte scan per candidate.
    fn linear_scan_lower<V>(&self, query_lower: &[u8], files: &HashMap<String, V>) -> Vec<String>
    where
        V: AsRef<IndexedFile>,
    {
        files
            .iter()
            .filter_map(|(path, file)| {
                if contains_bytes_ci(&file.as_ref().content, query_lower) {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Remove `id` from every posting list; drop empty lists.
    fn remove_id_everywhere(&mut self, id: u64) {
        self.map.retain(|_, list| {
            if let Ok(pos) = list.binary_search(&id) {
                list.remove(pos);
            }
            !list.is_empty()
        });
    }

    /// Remove `id` from one posting list; drop it when empty.
    fn remove_id(&mut self, tg: &[u8; 3], id: u64) {
        if let Some(list) = self.map.get_mut(tg)
            && let Ok(pos) = list.binary_search(&id)
        {
            list.remove(pos);
            if list.is_empty() {
                self.map.remove(tg);
            }
        }
    }

    /// Get existing file_id for path or allocate a new one.
    fn get_or_alloc_id(&mut self, path: &str) -> u64 {
        if let Some(&id) = self.path_to_id.get(path) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.path_to_id.insert(path.to_string(), id);
        self.id_to_path.insert(id, path.to_string());
        id
    }

    /// Insert a file without clearing old trigrams first (used in build_from_files).
    fn insert_file(&mut self, path: &str, content: &[u8]) {
        let file_id = self.get_or_alloc_id(path);
        for tg in trigrams_of(content) {
            insert_sorted(self.map.entry(tg).or_default(), file_id);
        }
    }
}

/// The trigrams the index stores for `content`: lowercased for CI search.
fn trigrams_of(content: &[u8]) -> HashSet<[u8; 3]> {
    let content_lower: Vec<u8> = content.iter().map(|b| b.to_ascii_lowercase()).collect();
    extract_trigrams(&content_lower)
}

/// Insert `id` keeping the list sorted, for binary search.
fn insert_sorted(list: &mut Vec<u64>, id: u64) {
    if let Err(pos) = list.binary_search(&id) {
        list.insert(pos, id);
    }
}

impl Default for TrigramIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the set of unique 3-byte windows from `bytes`.
///
/// Callers are responsible for lowercasing `bytes` before calling this function.
/// The function operates on the bytes as-is without any case conversion.
fn extract_trigrams(bytes: &[u8]) -> HashSet<[u8; 3]> {
    if bytes.len() < 3 {
        return HashSet::new();
    }
    bytes.windows(3).map(|w| [w[0], w[1], w[2]]).collect()
}

/// Case-insensitive byte containment check without allocation.
///
/// `needle_lower` must already be ASCII-lowercased; `haystack` is compared
/// one byte at a time via `to_ascii_lowercase()` so no temporary Vec is needed.
fn contains_bytes_ci(haystack: &[u8], needle_lower: &[u8]) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if needle_lower.len() > haystack.len() {
        return false;
    }
    'outer: for i in 0..=(haystack.len() - needle_lower.len()) {
        for j in 0..needle_lower.len() {
            if haystack[i + j].to_ascii_lowercase() != needle_lower[j] {
                continue 'outer;
            }
        }
        return true;
    }
    false
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::LanguageId;
    use crate::live_index::store::{IndexedFile, ParseStatus};
    use std::collections::{BTreeMap, BTreeSet};

    fn make_file(path: &str, content: &[u8]) -> (String, IndexedFile) {
        (
            path.to_string(),
            IndexedFile {
                relative_path: path.to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path(path),
                content: content.to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
                references: vec![],
                alias_map: std::collections::HashMap::new(),
                mtime_secs: 0,
            },
        )
    }

    fn make_files(pairs: &[(&str, &[u8])]) -> HashMap<String, IndexedFile> {
        pairs.iter().map(|(p, c)| make_file(p, c)).collect()
    }

    // ── build_from_files ─────────────────────────────────────────────────────

    #[test]
    fn test_build_from_files_indexes_all_content() {
        let files = make_files(&[
            ("src/a.rs", b"fn parse_file() {}"),
            ("src/b.rs", b"fn render() {}"),
        ]);
        let idx = TrigramIndex::build_from_files(&files);

        // Both files must be tracked
        assert_eq!(idx.path_to_id.len(), 2);
        // The trigram "par" from "parse" must be in the index
        let par: [u8; 3] = [b'p', b'a', b'r'];
        assert!(
            idx.map.contains_key(&par),
            "trigram 'par' must be indexed from 'parse'"
        );
    }

    // ── search: correct file matches ─────────────────────────────────────────

    #[test]
    fn test_search_returns_files_containing_query() {
        let files = make_files(&[
            ("parser.rs", b"fn parse_source() {}"),
            ("render.rs", b"fn render_frame() {}"),
        ]);
        let idx = TrigramIndex::build_from_files(&files);
        let results = idx.search(b"parse", &files);
        assert!(
            results.contains(&"parser.rs".to_string()),
            "parser.rs should match 'parse'"
        );
        assert!(
            !results.contains(&"render.rs".to_string()),
            "render.rs should not match 'parse'"
        );
    }

    #[test]
    fn test_search_returns_empty_for_trigram_not_in_any_file() {
        let files = make_files(&[("a.rs", b"fn alpha() {}"), ("b.rs", b"fn beta() {}")]);
        let idx = TrigramIndex::build_from_files(&files);
        // "zzz" cannot appear in the files above
        let results = idx.search(b"zzz", &files);
        assert!(
            results.is_empty(),
            "should return empty vec for unknown trigram"
        );
    }

    // ── search: short query fallback ─────────────────────────────────────────

    #[test]
    fn test_search_short_query_falls_back_to_linear_scan() {
        let files = make_files(&[("foo.rs", b"fn abc() {}"), ("bar.rs", b"fn xyz() {}")]);
        let idx = TrigramIndex::build_from_files(&files);

        // 1-char query — must use linear scan
        let results = idx.search(b"a", &files);
        assert!(
            results.contains(&"foo.rs".to_string()),
            "linear scan: 'a' matches 'abc'"
        );
        assert!(
            !results.contains(&"bar.rs".to_string()),
            "linear scan: 'a' does not match 'xyz'"
        );
    }

    #[test]
    fn test_search_two_char_query_falls_back_to_linear_scan() {
        let files = make_files(&[("a.rs", b"fn fn_alpha() {}"), ("b.rs", b"fn fn_beta() {}")]);
        let idx = TrigramIndex::build_from_files(&files);

        let results = idx.search(b"al", &files);
        assert!(
            results.contains(&"a.rs".to_string()),
            "linear scan: 'al' in fn_alpha"
        );
        assert!(
            !results.contains(&"b.rs".to_string()),
            "linear scan: 'al' not in fn_beta"
        );
    }

    // ── search: empty query ───────────────────────────────────────────────────

    #[test]
    fn test_search_empty_query_returns_empty() {
        let files = make_files(&[("a.rs", b"fn foo() {}")]);
        let idx = TrigramIndex::build_from_files(&files);
        let results = idx.search(b"", &files);
        assert!(results.is_empty(), "empty query must return empty vec");
    }

    // ── search: AND intersection ─────────────────────────────────────────────

    #[test]
    fn test_search_and_intersection_correct() {
        // Only "ab.rs" contains both "alpha" and "beta"
        let files = make_files(&[
            ("alpha_only.rs", b"fn alpha() {}"),
            ("beta_only.rs", b"fn beta() {}"),
            ("ab.rs", b"fn alpha() {} fn beta() {}"),
        ]);
        let idx = TrigramIndex::build_from_files(&files);

        // Search for "alpha" — should return alpha_only.rs and ab.rs but not beta_only.rs
        let alpha_results = idx.search(b"alpha", &files);
        assert!(alpha_results.contains(&"alpha_only.rs".to_string()));
        assert!(alpha_results.contains(&"ab.rs".to_string()));
        assert!(!alpha_results.contains(&"beta_only.rs".to_string()));

        // Search for "beta" — should return beta_only.rs and ab.rs but not alpha_only.rs
        let beta_results = idx.search(b"beta", &files);
        assert!(beta_results.contains(&"beta_only.rs".to_string()));
        assert!(beta_results.contains(&"ab.rs".to_string()));
        assert!(!beta_results.contains(&"alpha_only.rs".to_string()));
    }

    // ── search: empty index ───────────────────────────────────────────────────

    #[test]
    fn test_search_empty_index_returns_empty() {
        let idx = TrigramIndex::new();
        let files: HashMap<String, IndexedFile> = HashMap::new();
        let results = idx.search(b"anything", &files);
        assert!(results.is_empty(), "empty index should return empty vec");
    }

    // ── update_file ───────────────────────────────────────────────────────────

    #[test]
    fn test_update_file_removes_old_trigrams_and_adds_new() {
        let files = make_files(&[("src/main.rs", b"fn old_function() {}")]);
        let mut idx = TrigramIndex::build_from_files(&files);

        // Verify old content is searchable
        let old_results = idx.search(b"old_function", &files);
        assert!(old_results.contains(&"src/main.rs".to_string()));

        // Update with new content
        idx.update_file(
            "src/main.rs",
            Some(b"fn old_function() {}"),
            b"fn new_function() {}",
        );

        // Build new files map reflecting the update
        let mut new_files = files;
        new_files.get_mut("src/main.rs").unwrap().content = b"fn new_function() {}".to_vec();

        // Old content no longer searchable
        let old_results = idx.search(b"old_function", &new_files);
        assert!(
            !old_results.contains(&"src/main.rs".to_string()),
            "old trigrams must be removed"
        );

        // New content is searchable
        let new_results = idx.search(b"new_function", &new_files);
        assert!(
            new_results.contains(&"src/main.rs".to_string()),
            "new trigrams must be indexed"
        );
    }

    #[test]
    fn test_update_file_reuses_existing_file_id() {
        let files = make_files(&[("src/lib.rs", b"fn alpha() {}")]);
        let mut idx = TrigramIndex::build_from_files(&files);

        let id_before = *idx.path_to_id.get("src/lib.rs").unwrap();
        idx.update_file("src/lib.rs", Some(b"fn alpha() {}"), b"fn beta() {}");
        let id_after = *idx.path_to_id.get("src/lib.rs").unwrap();

        assert_eq!(
            id_before, id_after,
            "update_file should reuse the existing file_id"
        );
    }

    // ── remove_file ───────────────────────────────────────────────────────────

    #[test]
    fn test_remove_file_clears_all_trigrams() {
        let files = make_files(&[
            ("keep.rs", b"fn keep_me() {}"),
            ("remove.rs", b"fn remove_me() {}"),
        ]);
        let mut idx = TrigramIndex::build_from_files(&files);

        idx.remove_file("remove.rs", Some(b"fn remove_me() {}"));

        // "remove.rs" path should be gone from mappings
        assert!(
            !idx.path_to_id.contains_key("remove.rs"),
            "path_to_id should not contain removed file"
        );

        // Trigrams unique to "remove.rs" must not point to any file_id anymore
        let remove_results = idx.search(b"remove_me", &files);
        assert!(
            !remove_results.contains(&"remove.rs".to_string()),
            "removed file should not appear in search"
        );

        // "keep.rs" should still be searchable
        let keep_results = idx.search(b"keep_me", &files);
        assert!(
            keep_results.contains(&"keep.rs".to_string()),
            "other files should still be searchable"
        );
    }

    #[test]
    fn test_remove_file_nonexistent_is_noop() {
        let files = make_files(&[("a.rs", b"fn foo() {}")]);
        let mut idx = TrigramIndex::build_from_files(&files);

        // Should not panic
        idx.remove_file("nonexistent.rs", None);

        // Existing file still searchable
        let results = idx.search(b"foo", &files);
        assert!(results.contains(&"a.rs".to_string()));
    }

    // ── incremental equals rebuild ───────────────────────────────────────────

    /// Trigram -> paths, independent of how file IDs were allocated.
    fn by_path(idx: &TrigramIndex) -> BTreeMap<[u8; 3], BTreeSet<String>> {
        idx.map
            .iter()
            .map(|(tg, ids)| {
                assert!(!ids.is_empty(), "no empty posting list is kept");
                assert!(ids.windows(2).all(|w| w[0] < w[1]), "sorted, deduped");
                (
                    *tg,
                    ids.iter().map(|id| idx.id_to_path[id].clone()).collect(),
                )
            })
            .collect()
    }

    /// Random update and remove sequences, with and without the old content,
    /// leave the index equal to one built from scratch over the same files.
    #[test]
    fn incremental_updates_and_removes_match_a_rebuild() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let paths = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"];
        let mut files: HashMap<String, IndexedFile> = HashMap::new();
        let mut idx = TrigramIndex::new();
        for step in 0..2_000 {
            let path = paths[(next() % paths.len() as u64) as usize];
            let pass_old = next() % 8 != 0;
            if next() % 4 == 0 {
                let old = files.remove(path);
                let old = old.as_ref().filter(|_| pass_old);
                idx.remove_file(path, old.map(|f| f.content.as_slice()));
            } else {
                // A tiny mixed-case alphabet, so files share and lose trigrams.
                let len = (next() % 12) as usize;
                let content: Vec<u8> = (0..len).map(|_| b"abAB c"[(next() % 6) as usize]).collect();
                let old = files.insert(path.to_string(), make_file(path, &content).1);
                let old = old.as_ref().filter(|_| pass_old);
                idx.update_file(path, old.map(|f| f.content.as_slice()), &content);
            }
            let rebuilt = TrigramIndex::build_from_files(&files);
            assert_eq!(by_path(&idx), by_path(&rebuilt), "step {step}");
            let tracked: BTreeSet<&String> = idx.path_to_id.keys().collect();
            assert_eq!(tracked, files.keys().collect(), "step {step}");
            assert_eq!(idx.id_to_path.len(), idx.path_to_id.len(), "step {step}");
        }
    }

    // ── extract_trigrams ─────────────────────────────────────────────────────

    #[test]
    fn test_extract_trigrams_produces_correct_windows() {
        let trigrams = extract_trigrams(b"abcd");
        assert!(trigrams.contains(b"abc"), "should contain 'abc'");
        assert!(trigrams.contains(b"bcd"), "should contain 'bcd'");
        assert_eq!(trigrams.len(), 2, "abcd has 2 unique trigrams");
    }

    #[test]
    fn test_extract_trigrams_short_bytes_empty() {
        assert!(
            extract_trigrams(b"ab").is_empty(),
            "< 3 bytes yields no trigrams"
        );
        assert!(
            extract_trigrams(b"").is_empty(),
            "empty bytes yields no trigrams"
        );
    }
}
