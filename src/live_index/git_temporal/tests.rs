use super::*;
use crate::domain::{SourceVersion, WorkingTreeState};

fn temporal_request(commit: &str) -> GitTemporalComputationRequest {
    GitTemporalComputationRequest {
        fence: GitTemporalPublicationFence {
            project_generation: 7,
            content_generation: 11,
            source: None,
            source_version: Some(SourceVersion {
                branch: Some("main".to_string()),
                commit: Some(commit.to_string()),
                working_tree: WorkingTreeState::Clean,
            }),
        },
        repo_root: PathBuf::from("repo"),
    }
}

#[test]
fn temporal_queue_keeps_one_running_and_coalesces_one_latest_request() {
    let mut queue = GitTemporalJobQueue::default();
    let first = temporal_request("a");
    let superseded = temporal_request("b");
    let latest = temporal_request("c");

    assert_eq!(queue.enqueue(first.clone()), Some(first.clone()));
    assert_eq!(queue.enqueue(first.clone()), None);
    assert_eq!(queue.enqueue(superseded), None);
    assert_eq!(queue.enqueue(latest.clone()), None);
    assert_eq!(queue.finish(&first), Some(latest.clone()));
    assert_eq!(queue.finish(&latest), None);
}

// ── Rendering helpers ───────────────────────────────────────────────

#[test]
fn test_churn_bar_zero() {
    assert_eq!(churn_bar(0.0), "░░░░░░░░░░");
}

#[test]
fn test_churn_bar_half() {
    assert_eq!(churn_bar(0.5), "█████░░░░░");
}

#[test]
fn test_churn_bar_full() {
    assert_eq!(churn_bar(1.0), "██████████");
}

#[test]
fn test_churn_bar_clamps_above_one() {
    assert_eq!(churn_bar(1.5), "██████████");
}

#[test]
fn test_churn_bar_clamps_negative() {
    assert_eq!(churn_bar(-0.2), "░░░░░░░░░░");
}

#[test]
fn test_churn_label_frozen() {
    assert_eq!(churn_label(0.1), "frozen");
}

#[test]
fn test_churn_label_cool() {
    assert_eq!(churn_label(0.3), "cool");
}

#[test]
fn test_churn_label_warm() {
    assert_eq!(churn_label(0.5), "warm");
}

#[test]
fn test_churn_label_hot() {
    assert_eq!(churn_label(0.7), "hot");
}

#[test]
fn test_churn_label_critical() {
    assert_eq!(churn_label(0.9), "critical");
}

#[test]
fn test_relative_time_today() {
    assert_eq!(relative_time(0.3), "today");
}

#[test]
fn test_relative_time_days() {
    assert_eq!(relative_time(3.0), "3d ago");
}

#[test]
fn test_relative_time_weeks() {
    assert_eq!(relative_time(14.0), "2w ago");
}

#[test]
fn test_relative_time_months() {
    assert_eq!(relative_time(60.0), "2mo ago");
}

#[test]
fn test_relative_time_negative() {
    assert_eq!(relative_time(-1.0), "just now");
}

// ── Truncation ──────────────────────────────────────────────────────

#[test]
fn test_truncate_message_short() {
    assert_eq!(truncate_message("hello", 72), "hello");
}

#[test]
fn test_truncate_message_exact_boundary() {
    let msg = "a".repeat(72);
    assert_eq!(truncate_message(&msg, 72), msg);
}

#[test]
fn test_truncate_message_long() {
    let msg = "a".repeat(100);
    let result = truncate_message(&msg, 72);
    assert!(result.ends_with("..."));
    assert_eq!(result.len(), 72);
}

// ── Numstat parsing ─────────────────────────────────────────────────

#[test]
fn test_parse_numstat_line_normal() {
    assert_eq!(
        parse_numstat_line("3\t1\tsrc/foo.rs"),
        Some("src/foo.rs".to_string())
    );
}

#[test]
fn test_parse_numstat_line_binary() {
    assert_eq!(parse_numstat_line("-\t-\timage.png"), None);
}

#[test]
fn test_parse_numstat_line_empty_path() {
    assert_eq!(parse_numstat_line("3\t1\t"), None);
}

#[test]
fn test_parse_numstat_line_not_numstat() {
    assert_eq!(parse_numstat_line("some random line"), None);
}

#[test]
fn test_parse_numstat_line_zero_changes() {
    assert_eq!(
        parse_numstat_line("0\t0\tsrc/lib.rs"),
        Some("src/lib.rs".to_string())
    );
}

// ── Git log parsing ─────────────────────────────────────────────────

fn sample_git_log() -> String {
    // Simulate two commits: one 2 days ago, one 10 days ago.
    let now_unix = 1_741_520_000_u64;
    let ts_2d = now_unix - (2 * 86400);
    let ts_10d = now_unix - (10 * 86400);
    format!(
        "{delim}\n\
             H:abc1234\n\
             U:{ts_2d}\n\
             D:2026-03-07T10:00:00+00:00\n\
             A:Alice\n\
             M:fix: resolve parsing bug\n\
             \n\
             3\t1\tsrc/protocol/tools.rs\n\
             5\t2\tsrc/protocol/format.rs\n\
             {delim}\n\
             H:def5678\n\
             U:{ts_10d}\n\
             D:2026-02-27T10:00:00+00:00\n\
             A:Bob\n\
             M:feat: add search feature\n\
             \n\
             10\t0\tsrc/protocol/tools.rs\n\
             20\t5\tsrc/live_index/query.rs\n",
        delim = COMMIT_DELIMITER,
    )
}

#[test]
fn test_parse_git_log_extracts_commits() {
    let log = sample_git_log();
    let now_unix = 1_741_520_000_u64;
    let commits = parse_git_log(&log, now_unix);
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].hash, "abc1234");
    assert_eq!(commits[0].author, "Alice");
    assert_eq!(commits[0].files.len(), 2);
    assert_eq!(commits[1].hash, "def5678");
    assert_eq!(commits[1].author, "Bob");
    assert_eq!(commits[1].files.len(), 2);
}

#[test]
fn test_parse_git_log_computes_days_ago() {
    let log = sample_git_log();
    let now_unix = 1_741_520_000_u64;
    let commits = parse_git_log(&log, now_unix);
    assert!((commits[0].days_ago - 2.0).abs() < 0.01);
    assert!((commits[1].days_ago - 10.0).abs() < 0.01);
}

#[test]
fn test_parse_git_log_empty_input() {
    let commits = parse_git_log("", 1_741_520_000);
    assert!(commits.is_empty());
}

#[test]
fn test_parse_git_log_skips_commit_with_no_files() {
    let log = format!(
        "{delim}\nH:aaa1111\nU:1741520000\nD:2026-03-09\nA:Eve\nM:empty commit\n",
        delim = COMMIT_DELIMITER,
    );
    let commits = parse_git_log(&log, 1_741_520_000);
    assert!(commits.is_empty());
}

// ── Churn score computation ─────────────────────────────────────────

#[test]
fn test_churn_score_recent_beats_old() {
    // Build a minimal log: file_a changed today, file_b changed 60 days ago.
    let now_unix = 1_741_520_000_u64;
    let ts_today = now_unix;
    let ts_old = now_unix - (60 * 86400);
    let log = format!(
        "{delim}\nH:aaa\nU:{ts_today}\nD:2026-03-09\nA:X\nM:today\n\n1\t0\tfile_a.rs\n\
             {delim}\nH:bbb\nU:{ts_old}\nD:2026-01-08\nA:Y\nM:old\n\n1\t0\tfile_b.rs\n",
        delim = COMMIT_DELIMITER,
    );
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    let a = index.files.get("file_a.rs").unwrap();
    let b = index.files.get("file_b.rs").unwrap();
    assert!(
        a.churn_score > b.churn_score,
        "recent file ({}) should have higher churn than old file ({})",
        a.churn_score,
        b.churn_score
    );
}

#[test]
fn test_churn_score_multiple_commits_beats_single() {
    let now_unix = 1_741_520_000_u64;
    let ts1 = now_unix - 86400;
    let ts2 = now_unix - (3 * 86400);
    let ts3 = now_unix - (5 * 86400);
    let log = format!(
        "{delim}\nH:a1\nU:{ts1}\nD:d1\nA:X\nM:m1\n\n1\t0\thot.rs\n\
             {delim}\nH:a2\nU:{ts2}\nD:d2\nA:X\nM:m2\n\n1\t0\thot.rs\n\
             {delim}\nH:a3\nU:{ts3}\nD:d3\nA:X\nM:m3\n\n1\t0\thot.rs\n1\t0\tcold.rs\n",
        delim = COMMIT_DELIMITER,
    );
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    let hot = index.files.get("hot.rs").unwrap();
    let cold = index.files.get("cold.rs").unwrap();
    assert!(hot.churn_score > cold.churn_score);
}

// ── Co-change / Jaccard ─────────────────────────────────────────────

#[test]
fn test_co_change_jaccard_basic() {
    // Two commits: both touch A+B. One extra commit touches only A.
    // shared=2, union = 3+2-2 = 3, jaccard = 2/3 ≈ 0.667
    let now_unix = 1_741_520_000_u64;
    let ts1 = now_unix - 86400;
    let ts2 = now_unix - 2 * 86400;
    let ts3 = now_unix - 3 * 86400;
    let log = format!(
        "{d}\nH:c1\nU:{ts1}\nD:d\nA:X\nM:m\n\n1\t0\ta.rs\n1\t0\tb.rs\n\
             {d}\nH:c2\nU:{ts2}\nD:d\nA:X\nM:m\n\n1\t0\ta.rs\n1\t0\tb.rs\n\
             {d}\nH:c3\nU:{ts3}\nD:d\nA:X\nM:m\n\n1\t0\ta.rs\n",
        d = COMMIT_DELIMITER,
    );
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    let a = index.files.get("a.rs").unwrap();
    assert_eq!(a.co_changes.len(), 1);
    assert_eq!(a.co_changes[0].path, "b.rs");
    assert_eq!(a.co_changes[0].shared_commits, 2);
    // Jaccard: 2 / (3 + 2 - 2) = 2/3
    assert!((a.co_changes[0].coupling_score - 0.6667).abs() < 0.01);
}

#[test]
fn test_co_change_mega_commit_excluded() {
    // One commit touches 60 files — should be excluded from co-change.
    let now_unix = 1_741_520_000_u64;
    let ts = now_unix - 86400;
    let mut files_section = String::new();
    for i in 0..60 {
        files_section.push_str(&format!("1\t0\tfile_{i}.rs\n"));
    }
    // Add a second commit that only touches file_0 and file_1 to have data.
    let ts2 = now_unix - 2 * 86400;
    let log = format!(
        "{d}\nH:mega\nU:{ts}\nD:d\nA:X\nM:mega commit\n\n{files}\
             {d}\nH:small\nU:{ts2}\nD:d\nA:X\nM:small\n\n1\t0\tfile_0.rs\n1\t0\tfile_1.rs\n",
        d = COMMIT_DELIMITER,
        files = files_section,
    );
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    // file_0 should only see file_1 as co-change from the small commit,
    // not all 59 other files from the mega commit.
    let f0 = index.files.get("file_0.rs").unwrap();
    // Only 1 shared commit (the small one), which is below MIN_SHARED_COMMITS (2).
    // So no co-changes should appear.
    assert!(f0.co_changes.is_empty());
}

#[test]
fn test_co_change_below_min_shared_excluded() {
    // Two files share only 1 commit — below MIN_SHARED_COMMITS threshold.
    let now_unix = 1_741_520_000_u64;
    let ts = now_unix - 86400;
    let log = format!(
        "{d}\nH:c1\nU:{ts}\nD:d\nA:X\nM:m\n\n1\t0\ta.rs\n1\t0\tb.rs\n",
        d = COMMIT_DELIMITER,
    );
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    let a = index.files.get("a.rs").unwrap();
    assert!(a.co_changes.is_empty());
    assert_eq!(a.weak_co_changes.len(), 1);
    assert_eq!(a.weak_co_changes[0].path, "b.rs");
    assert_eq!(a.weak_co_changes[0].shared_commits, 1);
}

#[test]
fn test_co_change_low_jaccard_kept_as_weak_candidate() {
    let now_unix = 1_741_520_000_u64;
    let mut log = String::new();
    for i in 0..2 {
        let ts = now_unix - (i as u64 + 1) * 86400;
        log.push_str(&format!(
            "{d}\nH:shared{i}\nU:{ts}\nD:d\nA:X\nM:shared\n\n1\t0\ta.rs\n1\t0\tb.rs\n",
            d = COMMIT_DELIMITER,
        ));
    }
    for i in 0..8 {
        let ts = now_unix - (i as u64 + 10) * 86400;
        log.push_str(&format!(
            "{d}\nH:a{i}\nU:{ts}\nD:d\nA:X\nM:a-only\n\n1\t0\ta.rs\n",
            d = COMMIT_DELIMITER,
        ));
    }
    for i in 0..8 {
        let ts = now_unix - (i as u64 + 30) * 86400;
        log.push_str(&format!(
            "{d}\nH:b{i}\nU:{ts}\nD:d\nA:X\nM:b-only\n\n1\t0\tb.rs\n",
            d = COMMIT_DELIMITER,
        ));
    }
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    let a = index.files.get("a.rs").unwrap();
    assert!(
        a.co_changes.is_empty(),
        "jaccard should stay below strong threshold"
    );
    assert_eq!(a.weak_co_changes.len(), 1);
    assert_eq!(a.weak_co_changes[0].path, "b.rs");
    assert_eq!(a.weak_co_changes[0].shared_commits, 2);
}

// ── Contributors ────────────────────────────────────────────────────

#[test]
fn test_contributors_sorted_by_percentage() {
    let now_unix = 1_741_520_000_u64;
    let mut log = String::new();
    // Alice: 3 commits, Bob: 1 commit to same file.
    for (i, author) in ["Alice", "Alice", "Alice", "Bob"].iter().enumerate() {
        let ts = now_unix - (i as u64 + 1) * 86400;
        log.push_str(&format!(
            "{d}\nH:c{i}\nU:{ts}\nD:d\nA:{author}\nM:m{i}\n\n1\t0\tshared.rs\n",
            d = COMMIT_DELIMITER,
        ));
    }
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    let h = index.files.get("shared.rs").unwrap();
    assert_eq!(h.contributors.len(), 2);
    assert_eq!(h.contributors[0].author, "Alice");
    assert_eq!(h.contributors[0].commit_count, 3);
    assert!((h.contributors[0].percentage - 75.0).abs() < 0.1);
    assert_eq!(h.contributors[1].author, "Bob");
}

// ── Last commit ─────────────────────────────────────────────────────

#[test]
fn test_last_commit_is_most_recent() {
    let now_unix = 1_741_520_000_u64;
    let ts_recent = now_unix - 86400;
    let ts_old = now_unix - 30 * 86400;
    let log = format!(
        "{d}\nH:old111\nU:{ts_old}\nD:old-date\nA:Old\nM:old commit\n\n1\t0\tf.rs\n\
             {d}\nH:new222\nU:{ts_recent}\nD:new-date\nA:New\nM:new commit\n\n1\t0\tf.rs\n",
        d = COMMIT_DELIMITER,
    );
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    let h = index.files.get("f.rs").unwrap();
    assert_eq!(h.last_commit.hash, "new222");
    assert_eq!(h.last_commit.author, "New");
}

// ── Hotspots ────────────────────────────────────────────────────────

#[test]
fn test_hotspots_sorted_descending() {
    let now_unix = 1_741_520_000_u64;
    let mut log = String::new();
    // hot.rs: 5 recent commits. cold.rs: 1 old commit.
    for i in 0..5 {
        let ts = now_unix - (i + 1) * 86400;
        log.push_str(&format!(
            "{d}\nH:h{i}\nU:{ts}\nD:d\nA:X\nM:m\n\n1\t0\thot.rs\n",
            d = COMMIT_DELIMITER,
        ));
    }
    let ts_old = now_unix - 80 * 86400;
    log.push_str(&format!(
        "{d}\nH:c0\nU:{ts_old}\nD:d\nA:Y\nM:m\n\n1\t0\tcold.rs\n",
        d = COMMIT_DELIMITER,
    ));
    let index = GitTemporalIndex::compute_from_log(&log, now_unix);
    assert!(!index.stats.hotspots.is_empty());
    assert_eq!(index.stats.hotspots[0].0, "hot.rs");
    assert!(index.stats.hotspots[0].1 > index.stats.hotspots.last().unwrap().1);
}

// ── Pending / unavailable states ────────────────────────────────────

#[test]
fn test_pending_state() {
    let idx = GitTemporalIndex::pending();
    assert_eq!(idx.state, GitTemporalState::Pending);
    assert!(idx.files.is_empty());
}

#[test]
fn test_unavailable_state() {
    let idx = GitTemporalIndex::unavailable("no git".to_string());
    assert_eq!(
        idx.state,
        GitTemporalState::Unavailable("no git".to_string())
    );
}

// ── Path normalization ──────────────────────────────────────────────

#[test]
fn test_normalize_git_path_backslash() {
    assert_eq!(normalize_git_path("src\\foo\\bar.rs"), "src/foo/bar.rs");
}

#[test]
fn test_normalize_git_path_forward_slash_unchanged() {
    assert_eq!(normalize_git_path("src/foo/bar.rs"), "src/foo/bar.rs");
}
