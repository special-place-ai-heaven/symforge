#![cfg(feature = "server")]

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use symforge::domain::{ReferenceKind, StatePlacement};
use symforge::live_index::LiveIndex;
use symforge::live_index::index_lifecycle::activation::project_source_authority;
use symforge::live_index::persist::{self, IndexSnapshot};
use symforge::live_index::single_file::{ReindexResult, update_file_from_disk};
use symforge::live_index::store::{
    IndexLoadSource, SharedIndex, SharedIndexHandle, SnapshotVerifyState,
};
use symforge::watcher::{WatcherInfo, WatcherState, run_watcher};
use tempfile::TempDir;

const FIXTURE: &[(&str, &str)] = &[
    (
        "src/util.ts",
        r#"/** Builds a stable id. */
export function makeId(prefix: string): string {
    return prefix + "-1";
}

/** Trims a label. */
export const formatLabel = (value: string): string => value.trim();
"#,
    ),
    (
        "src/handlers.ts",
        r#"import { formatLabel, makeId } from "./util";

export function wire(register: (fn: unknown) => void): void {
    register(formatLabel);
    register(makeId);
}
"#,
    ),
    (
        "src/widgets.tsx",
        r#"/** Renders a labelled panel. */
export function Panel(props: { label: string }) {
    return <div className="panel">{props.label}</div>;
}
"#,
    ),
    (
        "src/view.tsx",
        r#"import { Panel } from "./widgets";

export function View() {
    return <Panel label="hello" />;
}
"#,
    ),
    (
        "src/worker.js",
        r#"class Worker {
    handle() {}
    stable() {}
    bind(fn) {
        this["h\u0061ndle"] = fn;
        install(this.handle);
        install(this.stable);
    }
}

function install(callback) {
    callback();
}

export { Worker, install };
"#,
    ),
];

const UTIL: &str = "src/util.ts";

fn fixture() -> (TempDir, StatePlacement) {
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in FIXTURE {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let binding = match symforge::discovery::resolve_root_candidate(
        dir.path(),
        symforge::domain::RootCandidateSource::LaunchCwd,
        symforge::domain::RootRequestMode::Automatic,
    ) {
        symforge::domain::RootResolution::Bound(binding) => binding,
        resolution => panic!("fixture root should bind: {resolution:?}"),
    };
    let placement = symforge::discovery::resolve_state_placement(&binding);
    (dir, placement)
}

async fn checkpoint_and_restore(root: &Path, placement: &StatePlacement) -> SharedIndex {
    let fresh = LiveIndex::load(root).unwrap();
    persist::checkpoint_shared_index(&fresh, root, placement).expect("checkpoint");
    let snapshot = persist::load_snapshot(root, placement).expect("current snapshot restores");
    let mtimes = snapshot
        .files
        .iter()
        .map(|(path, file)| (path.clone(), file.mtime_secs))
        .collect();
    let restored = SharedIndexHandle::shared(persist::snapshot_to_live_index(snapshot, root));
    persist::background_verify(
        Arc::clone(&restored),
        root.to_path_buf(),
        mtimes,
        project_source_authority(root).active_observer(),
    )
    .await;
    {
        let index = restored.read();
        assert_eq!(index.load_source(), IndexLoadSource::SnapshotRestore);
        assert_eq!(
            index.snapshot_verify_state(),
            SnapshotVerifyState::completed_without_mismatches()
        );
    }
    restored
}

type Site = (
    String,
    (u32, u32),
    (u32, u32),
    ReferenceKind,
    Option<String>,
);

fn sites(hits: Vec<(&str, &symforge::domain::ReferenceRecord)>) -> Vec<Site> {
    let mut sites: Vec<Site> = hits
        .into_iter()
        .map(|(path, r)| {
            (
                path.to_owned(),
                r.byte_range,
                r.line_range,
                r.kind,
                r.qualified_name.clone(),
            )
        })
        .collect();
    sites.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    sites
}

fn exact(index: &LiveIndex, path: &str, name: &str) -> Vec<Site> {
    sites(
        index
            .find_exact_references_for_symbol(path, name, None, None, None)
            .unwrap_or_else(|error| panic!("{path} {name}: {error}")),
    )
}

type SymbolRange = (
    String,
    String,
    (u32, u32),
    (u32, u32),
    Option<(u32, u32)>,
    Option<(u32, u32)>,
);

#[derive(Debug, PartialEq)]
struct Answers {
    jsx: Vec<Site>,
    exact: Vec<(&'static str, &'static str, Vec<Site>)>,
    dependents: Vec<Site>,
    symbols: Vec<SymbolRange>,
}

fn answers(index: &LiveIndex) -> Answers {
    let mut symbols = Vec::new();
    for (path, _) in FIXTURE {
        for s in &index.get_file(path).unwrap().symbols {
            symbols.push((
                path.to_string(),
                s.name.clone(),
                s.line_range,
                s.byte_range,
                s.item_byte_range,
                s.doc_byte_range,
            ));
        }
    }
    Answers {
        jsx: sites(index.find_references_for_name("Panel", Some(ReferenceKind::Call), false)),
        exact: [
            (UTIL, "makeId"),
            (UTIL, "formatLabel"),
            ("src/widgets.tsx", "Panel"),
            ("src/worker.js", "handle"),
            ("src/worker.js", "stable"),
        ]
        .into_iter()
        .map(|(path, name)| (path, name, exact(index, path, name)))
        .collect(),
        dependents: sites(index.find_dependents_for_file(UTIL)),
        symbols,
    }
}

fn value_use_from(sites: &[Site], path: &str, line: u32) -> bool {
    sites
        .iter()
        .any(|s| s.0 == path && s.2.0 == line && s.3 == ReferenceKind::ValueUse)
}

#[test]
fn a_format_11_snapshot_is_preserved_and_never_restored() {
    let (dir, placement) = fixture();
    let fresh = LiveIndex::load(dir.path()).unwrap();
    persist::checkpoint_shared_index(&fresh, dir.path(), &placement).expect("checkpoint");
    let snapshot_path = dir.path().join(".symforge").join("index.bin");
    let current = fs::read(&snapshot_path).unwrap();
    let mut snapshot: IndexSnapshot =
        postcard::from_bytes(&current[..current.len() - 32]).expect("current payload decodes");
    assert_eq!(snapshot.version, 12);
    snapshot.version = 11;
    let mut bytes = postcard::to_stdvec(&snapshot).unwrap();
    use sha2::Digest;
    let checksum = sha2::Sha256::digest(&bytes);
    bytes.extend_from_slice(&checksum);
    fs::write(&snapshot_path, &bytes).unwrap();

    assert!(
        persist::load_snapshot(dir.path(), &placement).is_none(),
        "a format-11 snapshot must not restore under format 12"
    );
    assert_eq!(
        fs::read(&snapshot_path).ok().as_deref(),
        Some(bytes.as_slice()),
        "the format-11 original stays in place for rollback"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restored_snapshot_answers_like_a_fresh_parse() {
    let (dir, placement) = fixture();
    let fresh = LiveIndex::load(dir.path()).unwrap();
    let expected = answers(&fresh.read());
    let restored = checkpoint_and_restore(dir.path(), &placement).await;
    let actual = answers(&restored.read());
    assert_eq!(actual, expected);

    assert!(
        value_use_from(&expected.exact[0].2, "src/handlers.ts", 4),
        "{expected:?}"
    );
    assert!(
        value_use_from(&expected.exact[1].2, "src/handlers.ts", 3),
        "{expected:?}"
    );
    assert!(
        expected
            .jsx
            .iter()
            .any(|s| s.0 == "src/view.tsx" && s.2.0 == 3),
        "{expected:?}"
    );
    assert!(
        expected.exact[3].2.is_empty(),
        "rebound method stays blocked: {expected:?}"
    );
    assert!(
        value_use_from(&expected.exact[4].2, "src/worker.js", 6),
        "{expected:?}"
    );
    assert!(
        expected.dependents.iter().any(|s| s.0 == "src/handlers.ts"),
        "{expected:?}"
    );
    let util = fresh.read().get_file(UTIL).unwrap().content.clone();
    let item = |name: &str| {
        let symbol = expected
            .symbols
            .iter()
            .find(|s| s.0 == UTIL && s.1 == name)
            .unwrap();
        let (start, end) = symbol.4.unwrap();
        String::from_utf8(util[start as usize..end as usize].to_vec()).unwrap()
    };
    assert!(item("makeId").starts_with("/** Builds a stable id. */\nexport function makeId"));
    assert!(item("formatLabel").starts_with("/** Trims a label. */\nexport const formatLabel"));
}

fn imported_value_use(shared: &SharedIndex) -> bool {
    value_use_from(&exact(&shared.read(), UTIL, "makeId"), "src/handlers.ts", 4)
}

const UNEXPORTED_UTIL: &str = "/** Builds a stable id. */\nfunction makeId(prefix: string): string {\n    return prefix + \"-1\";\n}\n\n/** Trims a label. */\nexport const formatLabel = (value: string): string => value.trim();\n";

fn assert_exporter_edit_reaches_importer(dir: &Path, shared: &SharedIndex) {
    let original = fs::read_to_string(dir.join(UTIL)).unwrap();
    let importer = shared
        .read()
        .get_file("src/handlers.ts")
        .unwrap()
        .content_hash
        .clone();
    assert!(imported_value_use(shared));
    fs::write(dir.join(UTIL), UNEXPORTED_UTIL).unwrap();
    assert_eq!(
        update_file_from_disk(shared, dir, UTIL),
        ReindexResult::Reindexed
    );
    assert!(
        !imported_value_use(shared),
        "unexported function is still an exact import target"
    );
    fs::write(dir.join(UTIL), &original).unwrap();
    assert_eq!(
        update_file_from_disk(shared, dir, UTIL),
        ReindexResult::Reindexed
    );
    assert!(
        imported_value_use(shared),
        "re-exported function lost its importer"
    );
    assert_eq!(
        shared
            .read()
            .get_file("src/handlers.ts")
            .unwrap()
            .content_hash,
        importer
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_exporter_edit_updates_importer_answers_in_fresh_and_restored_indexes() {
    let (dir, placement) = fixture();
    let fresh = LiveIndex::load(dir.path()).unwrap();
    assert_exporter_edit_reaches_importer(dir.path(), &fresh);
    let restored = checkpoint_and_restore(dir.path(), &placement).await;
    assert_exporter_edit_reaches_importer(dir.path(), &restored);
}

async fn wait_until(shared: &SharedIndex, expected: bool) -> bool {
    tokio::time::timeout(Duration::from_secs(10), async {
        while imported_value_use(shared) != expected {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .is_ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_watcher_carries_an_exporter_edit_to_a_restored_importer() {
    let (dir, placement) = fixture();
    let shared = checkpoint_and_restore(dir.path(), &placement).await;
    assert!(imported_value_use(&shared));
    let info = Arc::new(Mutex::new(WatcherInfo::default()));
    let watcher = tokio::spawn(run_watcher(
        dir.path().to_path_buf(),
        Arc::clone(&shared),
        Arc::clone(&info),
    ));
    tokio::time::timeout(Duration::from_secs(10), async {
        while info.lock().state != WatcherState::Active {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("watcher becomes active");

    fs::write(dir.path().join(UTIL), UNEXPORTED_UTIL).unwrap();
    assert!(
        wait_until(&shared, false).await,
        "watcher left a stale exact import"
    );
    fs::write(dir.path().join(UTIL), FIXTURE[0].1).unwrap();
    assert!(
        wait_until(&shared, true).await,
        "watcher did not restore the exact import"
    );
    watcher.abort();
}
