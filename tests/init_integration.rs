// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use symforge::cli::InitClient;
use symforge::cli::init::{
    merge_hooks_into_settings, register_codex_mcp_server, register_kilo_mcp_server,
    run_init_with_context,
};
/// Integration tests for `symforge init` — proves idempotent hook installation.
///
/// Tests use a temporary directory in place of `~/.claude/settings.json` via the
/// `merge_hooks_into_settings(settings_path, binary_path)` public function.
use tempfile::TempDir;

const FAKE_BINARY: &str = "/usr/local/bin/symforge";

fn fake_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(FAKE_BINARY)
}

/// Read settings.json from the temp dir.
fn read_settings(dir: &TempDir) -> serde_json::Value {
    let path = dir.path().join("settings.json");
    let settings_json = std::fs::read_to_string(&path).expect("settings.json must exist");
    serde_json::from_str(&settings_json).expect("settings.json must be valid JSON")
}

fn read_text(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).expect("text file must exist")
}

// ---------------------------------------------------------------------------
// test_init_writes_hooks: init produces correct hook entries
// ---------------------------------------------------------------------------

#[test]
fn test_init_writes_hooks() {
    let dir = TempDir::new().unwrap();
    let settings_path = dir.path().join("settings.json");

    merge_hooks_into_settings(&settings_path, &fake_binary_path())
        .expect("merge_hooks_into_settings must succeed");

    let settings = read_settings(&dir);

    let post = settings["hooks"]["PostToolUse"]
        .as_array()
        .expect("PostToolUse must be an array");
    let session = settings["hooks"]["SessionStart"]
        .as_array()
        .expect("SessionStart must be an array");
    let prompt = settings["hooks"]["UserPromptSubmit"]
        .as_array()
        .expect("UserPromptSubmit must be an array");

    assert_eq!(
        post.len(),
        1,
        "PostToolUse must have 1 entry (single stdin-routed entry)"
    );
    assert_eq!(session.len(), 1, "SessionStart must have 1 entry");
    assert_eq!(prompt.len(), 1, "UserPromptSubmit must have 1 entry");

    // Verify each entry has the correct binary path embedded.
    let all_commands: Vec<&str> = post
        .iter()
        .chain(session.iter())
        .flat_map(|e| e["hooks"].as_array().unwrap())
        .filter_map(|h| h["command"].as_str())
        .collect();

    for cmd in &all_commands {
        assert!(
            cmd.contains("symforge hook"),
            "command must contain 'symforge hook': {cmd}"
        );
        assert!(
            cmd.contains(FAKE_BINARY),
            "command must contain binary path {FAKE_BINARY}: {cmd}"
        );
    }

    // Verify the PostToolUse matcher covers all tools.
    let matcher = post[0]["matcher"].as_str().unwrap();
    assert_eq!(
        matcher, "Read|Edit|Write|Grep",
        "matcher must cover all tools"
    );

    // Verify session-start hook is present.
    let has_session = all_commands
        .iter()
        .any(|c| c.ends_with("hook session-start"));
    assert!(has_session, "SessionStart hook must be present");
    let has_prompt_submit = prompt
        .iter()
        .flat_map(|e| e["hooks"].as_array().unwrap())
        .filter_map(|h| h["command"].as_str())
        .any(|c| c.ends_with("hook prompt-submit"));
    assert!(has_prompt_submit, "UserPromptSubmit hook must be present");
}

// ---------------------------------------------------------------------------
// test_init_idempotent: running init twice produces identical output
// ---------------------------------------------------------------------------

#[test]
fn test_init_idempotent() {
    let dir = TempDir::new().unwrap();
    let settings_path = dir.path().join("settings.json");

    merge_hooks_into_settings(&settings_path, &fake_binary_path())
        .expect("first merge must succeed");
    let after_first = std::fs::read_to_string(&settings_path).unwrap();

    merge_hooks_into_settings(&settings_path, &fake_binary_path())
        .expect("second merge must succeed");
    let after_second = std::fs::read_to_string(&settings_path).unwrap();

    assert_eq!(
        after_first, after_second,
        "running merge_hooks_into_settings twice must produce identical output (idempotent)"
    );

    // Also assert entry count didn't grow.
    let settings = read_settings(&dir);
    let post_count = settings["hooks"]["PostToolUse"].as_array().unwrap().len();
    assert_eq!(post_count, 1, "second merge must not add duplicate entries");
}

// ---------------------------------------------------------------------------
// test_init_preserves_other_hooks: non-symforge hooks are preserved
// ---------------------------------------------------------------------------

#[test]
fn test_init_preserves_other_hooks() {
    let dir = TempDir::new().unwrap();
    let settings_path = dir.path().join("settings.json");

    // Start with an existing non-symforge hook.
    let initial = serde_json::json!({
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "/some/other/hook bash", "timeout": 10}]
                }
            ]
        }
    });
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();

    merge_hooks_into_settings(&settings_path, &fake_binary_path()).expect("merge must succeed");

    let settings = read_settings(&dir);
    let post = settings["hooks"]["PostToolUse"]
        .as_array()
        .expect("PostToolUse must be an array");

    // 1 existing + 1 symforge = 2 total.
    assert_eq!(post.len(), 2, "existing hook + 1 symforge hook = 2 entries");

    // Non-symforge hook must still be present.
    let has_bash_hook = post.iter().any(|e| {
        e["hooks"][0]["command"]
            .as_str()
            .map(|c| c == "/some/other/hook bash")
            .unwrap_or(false)
    });
    assert!(
        has_bash_hook,
        "non-symforge hook must be preserved after merge"
    );
}

// ---------------------------------------------------------------------------
// test_init_registers_mcp_server: MCP entry written to claude.json
// ---------------------------------------------------------------------------

#[test]
fn test_init_registers_mcp_server() {
    let dir = TempDir::new().unwrap();
    let claude_json_path = dir.path().join(".claude.json");
    let binary_path = "/usr/local/bin/symforge";

    symforge::cli::init::register_mcp_server(&claude_json_path, binary_path)
        .expect("register_mcp_server must succeed");

    let config_json = std::fs::read_to_string(&claude_json_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap();

    let tok = &config["mcpServers"]["symforge"];
    // On Windows, forward slashes are converted to backslashes for native process spawning.
    let expected_command = if cfg!(windows) {
        binary_path.replace('/', "\\")
    } else {
        binary_path.to_string()
    };
    assert_eq!(tok["command"], expected_command);
    assert_eq!(tok["disabled"], false, "disabled must be false");
    assert_eq!(
        tok["env"]["SYMFORGE_SURFACE"].as_str(),
        Some("full"),
        "Claude init must pin the full surface"
    );
    assert!(
        tok["alwaysAllow"].is_array(),
        "alwaysAllow must be an array"
    );
    let always_allow = tok["alwaysAllow"].as_array().unwrap();
    assert!(
        always_allow.iter().any(|v| v.as_str() == Some("health")),
        "alwaysAllow must include health"
    );
    assert!(
        always_allow
            .iter()
            .any(|v| v.as_str() == Some("search_symbols")),
        "alwaysAllow must include search_symbols"
    );
    assert!(
        always_allow
            .iter()
            .any(|v| v.as_str() == Some("replace_symbol_body")),
        "alwaysAllow must include replace_symbol_body"
    );
}

#[test]
fn test_init_mcp_registration_idempotent() {
    let dir = TempDir::new().unwrap();
    let claude_json_path = dir.path().join(".claude.json");
    let binary_path = "/usr/local/bin/symforge";

    symforge::cli::init::register_mcp_server(&claude_json_path, binary_path).unwrap();
    let first = std::fs::read_to_string(&claude_json_path).unwrap();

    symforge::cli::init::register_mcp_server(&claude_json_path, binary_path).unwrap();
    let second = std::fs::read_to_string(&claude_json_path).unwrap();

    assert_eq!(first, second, "register_mcp_server must be idempotent");
}

#[test]
fn test_init_mcp_registration_preserves_other_servers() {
    let dir = TempDir::new().unwrap();
    let claude_json_path = dir.path().join(".claude.json");

    // Pre-populate with another MCP server.
    let initial = serde_json::json!({
        "mcpServers": {
            "other-server": {"type": "stdio", "command": "other-binary"}
        }
    });
    std::fs::write(
        &claude_json_path,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();

    symforge::cli::init::register_mcp_server(&claude_json_path, "/usr/local/bin/symforge").unwrap();

    let config_json = std::fs::read_to_string(&claude_json_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap();

    assert!(
        config["mcpServers"]["other-server"].is_object(),
        "other MCP server must be preserved"
    );
    assert!(
        config["mcpServers"]["symforge"].is_object(),
        "symforge must be added"
    );
}

#[test]
fn test_init_registers_codex_mcp_server() {
    let dir = TempDir::new().unwrap();
    let codex_config_path = dir.path().join(".codex").join("config.toml");
    let binary_path = r"C:\Users\user\.symforge\bin\symforge.exe";

    register_codex_mcp_server(&codex_config_path, binary_path)
        .expect("register_codex_mcp_server must succeed");

    let config_toml = std::fs::read_to_string(&codex_config_path).unwrap();

    assert!(
        config_toml.contains("[mcp_servers.symforge]"),
        "config must contain a symforge MCP table: {config_toml}"
    );
    assert!(
        config_toml.contains(binary_path),
        "config must contain the Windows binary path: {config_toml}"
    );
    assert!(
        !config_toml.contains("startup_timeout_sec"),
        "config must not seed a Codex startup timeout (it equals Codex's default): {config_toml}"
    );
    assert!(
        !config_toml.contains("tool_timeout_sec"),
        "config must not seed a Codex tool timeout (it can only shorten the default): {config_toml}"
    );
    assert!(
        !config_toml.contains("allowed_tools"),
        "Codex has no allowed_tools key, so none may be written: {config_toml}"
    );
    assert!(
        !config_toml.contains("default_tools_approval_mode"),
        "auto is Codex's own default, so writing it would pre-approve nothing: {config_toml}"
    );
    assert!(
        config_toml.contains("project_doc_fallback_filenames"),
        "config must configure project doc fallbacks: {config_toml}"
    );
    assert!(
        config_toml.contains("CLAUDE.md"),
        "config must include CLAUDE.md as a project doc fallback: {config_toml}"
    );
}

#[test]
fn test_init_codex_registration_idempotent() {
    let dir = TempDir::new().unwrap();
    let codex_config_path = dir.path().join(".codex").join("config.toml");
    let binary_path = r"C:\Users\user\.symforge\bin\symforge.exe";

    register_codex_mcp_server(&codex_config_path, binary_path).unwrap();
    let first = std::fs::read_to_string(&codex_config_path).unwrap();

    register_codex_mcp_server(&codex_config_path, binary_path).unwrap();
    let second = std::fs::read_to_string(&codex_config_path).unwrap();

    assert_eq!(
        first, second,
        "register_codex_mcp_server must be idempotent"
    );
}

#[test]
fn test_init_codex_registration_preserves_other_config() {
    let dir = TempDir::new().unwrap();
    let codex_dir = dir.path().join(".codex");
    let codex_config_path = codex_dir.join("config.toml");
    std::fs::create_dir_all(&codex_dir).unwrap();
    std::fs::write(
        &codex_config_path,
        r#"# keep this comment
model = "gpt-5.4"
project_doc_fallback_filenames = ["README.agent.md"]

[mcp_servers.other]
command = "other.exe"
"#,
    )
    .unwrap();

    register_codex_mcp_server(
        &codex_config_path,
        r"C:\Users\user\.symforge\bin\symforge.exe",
    )
    .unwrap();

    let config_toml = std::fs::read_to_string(&codex_config_path).unwrap();
    assert!(
        config_toml.contains("# keep this comment"),
        "existing comments should survive"
    );
    assert!(
        config_toml.contains("model = \"gpt-5.4\""),
        "existing config should survive"
    );
    assert!(
        config_toml.contains("[mcp_servers.other]"),
        "other MCP servers should survive"
    );
    assert!(
        config_toml.contains("README.agent.md"),
        "existing project doc fallbacks should survive"
    );
    assert!(
        config_toml.contains("CLAUDE.md"),
        "SymForge should merge CLAUDE.md into project doc fallbacks"
    );
}

#[test]
fn test_run_init_grok_preserves_toml_and_is_idempotent() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let grok_dir = home.path().join(".grok");
    let grok_config = grok_dir.join("config.toml");
    std::fs::create_dir_all(&grok_dir).unwrap();
    std::fs::write(
        &grok_config,
        r#"# keep this comment
theme = "dark"

[mcp_servers.other]
command = "other.exe"
enabled = false

[mcp_servers.symforge]
command = "stale.exe" # keep command comment
args = ["--stale"]
enabled = false
custom = "keep"

[mcp_servers.symforge.env]
KEEP_ME = "yes"
RUST_LOG = "off" # keep env comment
SYMFORGE_SURFACE = "compact"
SYMFORGE_WORKSPACE_ROOT = "/repo/init/ran/in"
"#,
    )
    .unwrap();
    let binary_path = fake_binary_path();
    let client = <InitClient as clap::ValueEnum>::from_str("grok", true)
        .expect("grok must be a supported init client");

    run_init_with_context(client, home.path(), cwd.path(), &binary_path)
        .expect("grok init must succeed");
    let first = read_text(&grok_config);
    run_init_with_context(client, home.path(), cwd.path(), &binary_path)
        .expect("repeated grok init must succeed");
    let second = read_text(&grok_config);

    assert_eq!(first, second, "Grok registration must be idempotent");
    assert!(first.contains("# keep this comment"));
    assert!(first.contains("theme = \"dark\""));
    assert!(first.contains("[mcp_servers.other]"));
    assert!(first.contains("custom = \"keep\""));
    assert!(first.contains("# keep command comment"));
    assert!(first.contains("# keep env comment"));

    let config = first.parse::<toml_edit::DocumentMut>().unwrap();
    let symforge = &config["mcp_servers"]["symforge"];
    let expected_command = if cfg!(windows) {
        FAKE_BINARY.replace('/', "\\")
    } else {
        FAKE_BINARY.to_string()
    };
    assert_eq!(
        symforge["command"].as_str(),
        Some(expected_command.as_str())
    );
    assert_eq!(symforge["args"].as_array().map(|args| args.len()), Some(0));
    assert_eq!(symforge["enabled"].as_bool(), Some(true));
    assert_eq!(symforge["env"]["RUST_LOG"].as_str(), Some("off"));
    assert!(
        symforge["env"].get("SYMFORGE_WORKSPACE_ROOT").is_none(),
        "the pin an earlier init wrote must be removed"
    );
    assert_eq!(symforge["env"]["KEEP_ME"].as_str(), Some("yes"));
    assert_eq!(
        symforge["env"]["SYMFORGE_SURFACE"].as_str(),
        Some("compact"),
        "unmanaged Grok env values must be preserved"
    );
    assert!(!home.path().join(".codex").join("config.toml").exists());
    assert!(!home.path().join(".claude.json").exists());
    assert!(!home.path().join(".gemini").join("settings.json").exists());
    assert!(!cwd.path().join(".kilocode").join("mcp.json").exists());
}

#[test]
fn test_run_init_grok_preserves_inline_tables() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let grok_dir = home.path().join(".grok");
    let grok_config = grok_dir.join("config.toml");
    std::fs::create_dir_all(&grok_dir).unwrap();
    std::fs::write(
        &grok_config,
        r#"mcp_servers = { other = { command = "x" }, symforge = { command = "stale", custom = "keep", env = { KEEP_ME = "yes" } } }
"#,
    )
    .unwrap();
    let binary_path = fake_binary_path();
    let client = <InitClient as clap::ValueEnum>::from_str("grok", true).unwrap();

    run_init_with_context(client, home.path(), cwd.path(), &binary_path).unwrap();
    let first = read_text(&grok_config);
    run_init_with_context(client, home.path(), cwd.path(), &binary_path).unwrap();
    let second = read_text(&grok_config);

    assert_eq!(first, second);

    let config = first.parse::<toml_edit::DocumentMut>().unwrap();
    let servers = &config["mcp_servers"];
    let symforge = &servers["symforge"];
    assert_eq!(servers["other"]["command"].as_str(), Some("x"));
    assert_eq!(symforge["custom"].as_str(), Some("keep"));
    assert_eq!(symforge["env"]["KEEP_ME"].as_str(), Some("yes"));
    assert_eq!(symforge["args"].as_array().map(|args| args.len()), Some(0));
    assert_eq!(symforge["enabled"].as_bool(), Some(true));
    assert_eq!(symforge["env"]["RUST_LOG"].as_str(), Some("off"));
    assert!(
        symforge["env"].get("SYMFORGE_WORKSPACE_ROOT").is_none(),
        "the global Grok config must not carry a workspace root"
    );
}

#[test]
fn test_grok_reregistration_removes_the_old_pinned_workspace_root() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"[mcp_servers.symforge]
command = "old"

[mcp_servers.symforge.env]
KEEP_ME = "yes"
RUST_LOG = "off"
SYMFORGE_WORKSPACE_ROOT = "/repo/init/ran/in"
"#,
    )
    .unwrap();

    symforge::cli::init::register_grok_mcp_server(&path, FAKE_BINARY).unwrap();

    let config = read_text(&path).parse::<toml_edit::DocumentMut>().unwrap();
    let env = &config["mcp_servers"]["symforge"]["env"];
    assert!(
        env.get("SYMFORGE_WORKSPACE_ROOT").is_none(),
        "the pin an earlier init wrote must be removed: {env}"
    );
    assert_eq!(env["KEEP_ME"].as_str(), Some("yes"));
}

#[test]
fn test_grok_registration_writes_no_workspace_root_on_a_fresh_config() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    symforge::cli::init::register_grok_mcp_server(&path, FAKE_BINARY).unwrap();
    assert!(
        !read_text(&path).contains("SYMFORGE_WORKSPACE_ROOT"),
        "a global config must not be pinned to one project"
    );
}

#[test]
fn test_claude_code_presence_agrees_between_scan_and_all() {
    use symforge::cli::harness::{AttachEntry, HarnessId, HarnessRegistry, HarnessState};

    let claude_state = |home: &std::path::Path, cwd: &std::path::Path| {
        HarnessRegistry::known_with(home, cwd)
            .scan(&AttachEntry::new("http://127.0.0.1:1/mcp", None))
            .into_iter()
            .find(|status| status.id == HarnessId::ClaudeCode)
            .expect("Claude Code is a known harness")
            .state
    };
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    // Neither `~/.claude` nor `~/.claude.json`: not installed, and `all` skips it.
    let bare = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    assert_eq!(
        claude_state(bare.path(), cwd.path()),
        HarnessState::NotInstalled
    );
    run_init_with_context(InitClient::All, bare.path(), cwd.path(), &binary_path).unwrap();
    assert!(!bare.path().join(".claude.json").exists());
    assert!(!bare.path().join(".claude").exists());

    // `~/.claude` alone: installed, and `all` registers it.
    let present = TempDir::new().unwrap();
    std::fs::create_dir_all(present.path().join(".claude")).unwrap();
    assert_eq!(
        claude_state(present.path(), cwd.path()),
        HarnessState::Absent
    );
    run_init_with_context(InitClient::All, present.path(), cwd.path(), &binary_path).unwrap();
    assert!(present.path().join(".claude.json").exists());
}

#[test]
fn test_init_registers_kilo_mcp_server() {
    let dir = TempDir::new().unwrap();
    let kilo_config_path = dir.path().join(".kilo").join("kilo.jsonc");
    let binary_path = r"C:\Users\user\.symforge\bin\symforge.exe";

    register_kilo_mcp_server(&kilo_config_path, binary_path)
        .expect("register_kilo_mcp_server must succeed");

    let config_json = std::fs::read_to_string(&kilo_config_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap();

    let symforge = &config["mcp"]["symforge"];
    assert_eq!(symforge["type"].as_str(), Some("local"));
    assert_eq!(symforge["command"], serde_json::json!([binary_path]));
    assert!(symforge.get("args").is_none(), "{symforge}");
    assert!(symforge.get("alwaysAllow").is_none(), "{symforge}");
    assert_eq!(
        symforge["environment"]["SYMFORGE_SURFACE"].as_str(),
        Some("full"),
        "Kilo environment must make the full surface explicit"
    );
    assert!(config.get("mcpServers").is_none(), "{config}");
}

#[test]
fn test_init_kilo_registration_preserves_other_servers() {
    let dir = TempDir::new().unwrap();
    let kilo_config_path = dir.path().join(".kilo").join("kilo.jsonc");

    let initial = serde_json::json!({
        "mcp": {
            "other-server": {
                "type": "local",
                "command": ["other-binary"]
            }
        }
    });
    std::fs::create_dir_all(kilo_config_path.parent().unwrap()).unwrap();
    std::fs::write(
        &kilo_config_path,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();

    register_kilo_mcp_server(&kilo_config_path, "/usr/local/bin/symforge").unwrap();

    let config_json = std::fs::read_to_string(&kilo_config_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap();

    assert!(
        config["mcp"]["other-server"].is_object(),
        "other MCP servers must be preserved"
    );
    assert!(
        config["mcp"]["symforge"].is_object(),
        "symforge must be added"
    );
}

#[test]
fn test_run_init_codex_only_updates_codex_files() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("codex init must succeed");

    assert!(
        home.path().join(".codex").join("config.toml").exists(),
        "Codex config must be created"
    );
    assert!(
        home.path().join(".codex").join("AGENTS.md").exists(),
        "Codex global AGENTS guidance must be created"
    );
    assert!(
        !home.path().join(".claude.json").exists(),
        "Claude MCP config must not be created for codex-only init"
    );
    assert!(
        !home.path().join(".claude").join("settings.json").exists(),
        "Claude hooks config must not be created for codex-only init"
    );
    assert!(
        !home.path().join(".claude").join("CLAUDE.md").exists(),
        "Claude memory file must not be created for codex-only init"
    );
    assert!(
        cwd.path().join(".symforge").exists(),
        "runtime directory must still be created"
    );
}

#[test]
fn project_aware_init_reconciles_existing_root_gitignore() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    std::fs::create_dir(cwd.path().join(".git")).expect("create git metadata marker");
    std::fs::write(cwd.path().join(".gitignore"), b"target/\n").expect("write root gitignore");
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("project-aware init must succeed");
    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("repeated project-aware init must succeed");

    assert_eq!(
        std::fs::read(cwd.path().join(".gitignore")).expect("read reconciled gitignore"),
        b"target/\n/.symforge/\n",
        "init must reconcile once, preserve newline style, and remain idempotent"
    );
}

/// Claude Desktop's config directory under an injected home, per platform.
fn claude_desktop_dir(home: &std::path::Path) -> std::path::PathBuf {
    if cfg!(windows) {
        home.join("AppData").join("Roaming").join("Claude")
    } else if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
    } else {
        home.join(".config").join("Claude")
    }
}

#[test]
fn test_run_init_all_skips_harnesses_that_are_not_installed() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);
    // Only Codex is installed on this host.
    std::fs::create_dir_all(home.path().join(".codex")).unwrap();

    run_init_with_context(InitClient::All, home.path(), cwd.path(), &binary_path)
        .expect("all-client init must succeed");

    assert!(
        home.path().join(".codex").join("config.toml").exists(),
        "the installed harness must be registered"
    );
    for absent in [".claude", ".claude.json", ".gemini", ".grok", ".cursor"] {
        assert!(
            !home.path().join(absent).exists(),
            "`all` must not create {absent} for a harness that is not installed"
        );
    }
    assert!(
        !claude_desktop_dir(home.path()).exists(),
        "`all` must not create the Claude Desktop config directory"
    );
    assert!(
        !cwd.path().join(".kilocode").exists(),
        "`all` must not write project-local Kilo config"
    );
}

#[test]
fn test_run_init_explicit_client_creates_its_config_directory() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Cursor, home.path(), cwd.path(), &binary_path)
        .expect("explicit cursor init must succeed");

    assert!(
        home.path().join(".cursor").join("mcp.json").exists(),
        "an explicitly named client may create its config directory"
    );
}

#[test]
fn test_run_init_claude_only_updates_claude_files() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("claude init must succeed");

    assert!(
        home.path().join(".claude.json").exists(),
        "Claude MCP config must be created"
    );
    assert!(
        home.path().join(".claude").join("settings.json").exists(),
        "Claude hooks config must be created"
    );
    assert!(
        home.path().join(".claude").join("CLAUDE.md").exists(),
        "Claude guidance memory must be created"
    );
    assert!(
        !home.path().join(".codex").join("config.toml").exists(),
        "Codex config must not be created for claude-only init"
    );
    assert!(
        !home.path().join(".codex").join("AGENTS.md").exists(),
        "Codex AGENTS guidance must not be created for claude-only init"
    );
    assert!(
        !home.path().join(".gemini").join("settings.json").exists(),
        "Gemini config must not be created for claude-only init"
    );
    assert!(
        !cwd.path().join(".kilocode").join("mcp.json").exists(),
        "Kilo config must not be created for claude-only init"
    );
}

#[test]
fn test_run_init_all_updates_both_clients() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let stable_bin_dir = std::env::current_dir()
        .unwrap()
        .join("target")
        .join("test-global-bin")
        .join(format!("init-all-{}", std::process::id()));
    std::fs::create_dir_all(&stable_bin_dir).unwrap();
    let binary_path = stable_bin_dir.join(if cfg!(windows) {
        "symforge.exe"
    } else {
        "symforge"
    });
    std::fs::write(&binary_path, b"").unwrap();
    let claude_desktop_config = if cfg!(windows) {
        home.path()
            .join("AppData")
            .join("Roaming")
            .join("Claude")
            .join("claude_desktop_config.json")
    } else if cfg!(target_os = "macos") {
        home.path()
            .join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json")
    } else {
        home.path()
            .join(".config")
            .join("Claude")
            .join("claude_desktop_config.json")
    };

    // `all` reaches only harnesses that are already installed, so give every
    // user-scope harness its config root first.
    for root in [".codex", ".grok", ".claude", ".gemini", ".cursor"] {
        std::fs::create_dir_all(home.path().join(root)).unwrap();
    }
    std::fs::create_dir_all(claude_desktop_config.parent().unwrap()).unwrap();

    run_init_with_context(InitClient::All, home.path(), cwd.path(), &binary_path)
        .expect("all-client init must succeed");

    assert!(
        home.path().join(".codex").join("config.toml").exists(),
        "Codex config must be created"
    );
    assert!(
        home.path().join(".grok").join("config.toml").exists(),
        "Grok config must be created"
    );
    assert!(
        home.path().join(".claude.json").exists(),
        "Claude MCP config must be created"
    );
    assert!(
        home.path().join(".claude").join("settings.json").exists(),
        "Claude hooks config must be created"
    );
    assert!(
        home.path().join(".claude").join("CLAUDE.md").exists(),
        "Claude guidance memory must be created"
    );
    assert!(
        home.path().join(".codex").join("AGENTS.md").exists(),
        "Codex AGENTS guidance must be created"
    );
    assert!(
        home.path().join(".gemini").join("settings.json").exists(),
        "Gemini config must be created"
    );
    assert!(
        home.path().join(".gemini").join("GEMINI.md").exists(),
        "Gemini guidance must be created"
    );
    assert!(
        !cwd.path().join(".kilocode").exists(),
        "Kilo is project-local, so `all` must never write it"
    );
    assert!(
        home.path().join(".cursor").join("mcp.json").exists(),
        "Cursor config must be created"
    );
    assert!(
        claude_desktop_config.exists(),
        "Claude Desktop config must be created under the injected home"
    );
    let claude_desktop_config_json = read_text(&claude_desktop_config);
    let claude_desktop_config_value: serde_json::Value =
        serde_json::from_str(&claude_desktop_config_json).unwrap();
    let claude_desktop_command = claude_desktop_config_value["mcpServers"]["symforge"]["command"]
        .as_str()
        .unwrap();
    // Windows: the registered command is the wrapper in the Desktop CONFIG dir
    // (npm wipes the binary's bin dir on every swap); the wrapper launches the
    // stable binary by absolute path. Non-Windows: no wrapper is generated —
    // the command is the stable binary itself.
    #[cfg(windows)]
    {
        let desktop_config_dir = claude_desktop_config
            .parent()
            .expect("desktop config has a parent dir");
        assert!(
            claude_desktop_command.contains(&desktop_config_dir.display().to_string()),
            "Claude Desktop command must point at the wrapper in the config dir: {claude_desktop_command}"
        );
        let wrapper_script = read_text(&desktop_config_dir.join("symforge-desktop.cmd"));
        assert!(
            wrapper_script.contains(&stable_bin_dir.display().to_string()),
            "the wrapper must launch the stable binary by absolute path: {wrapper_script}"
        );
    }
    #[cfg(not(windows))]
    assert!(
        claude_desktop_command.contains(&stable_bin_dir.display().to_string()),
        "Claude Desktop command must be the stable binary path on non-Windows: {claude_desktop_command}"
    );

    let codex_config = read_text(&home.path().join(".codex").join("config.toml"));
    assert!(
        codex_config.contains(&binary_path.display().to_string()),
        "Codex config must use stable binary path: {codex_config}"
    );
}

#[test]
fn test_run_init_codex_writes_symforge_agents_guidance() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("codex init must succeed");

    let agents_path = home.path().join(".codex").join("AGENTS.md");
    let raw = read_text(&agents_path);

    assert!(
        raw.contains("SYMFORGE START"),
        "Codex AGENTS guidance must include a SymForge marker block: {raw}"
    );
    assert!(
        raw.contains("SymForge MCP"),
        "Codex AGENTS guidance must mention SymForge MCP: {raw}"
    );
    assert!(
        raw.contains("get_file_context"),
        "Codex AGENTS guidance must include tool guidance: {raw}"
    );
    assert!(
        raw.contains("validate_file_syntax"),
        "Codex AGENTS guidance must include config validation guidance: {raw}"
    );
    assert!(
        raw.contains("| Task | Call |"),
        "Codex AGENTS guidance must carry the task-to-tool map: {raw}"
    );
    assert!(
        !raw.contains("## Agent Directives: Mechanical Overrides")
            && !raw.contains("## Tooling Preference"),
        "removed guidance sections must not ship again: {raw}"
    );
    assert!(
        raw.contains("\n<!-- SYMFORGE END -->"),
        "end marker must sit alone on its own line, nothing fused before it: {raw}"
    );
}

#[test]
fn test_run_init_codex_preserves_existing_agents_content_and_is_idempotent() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);
    let codex_dir = home.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let agents_path = codex_dir.join("AGENTS.md");
    std::fs::write(&agents_path, "# Existing guidance\n\nKeep this line.\n").unwrap();

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("first codex init must succeed");
    let first = read_text(&agents_path);

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("second codex init must succeed");
    let second = read_text(&agents_path);

    assert!(
        second.contains("Keep this line."),
        "existing Codex guidance must survive"
    );
    assert_eq!(first, second, "Codex AGENTS guidance must be idempotent");
}

#[test]
fn test_run_init_codex_never_duplicates_external_overrides_heading() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);
    let codex_dir = home.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let agents_path = codex_dir.join("AGENTS.md");
    std::fs::write(
        &agents_path,
        "# Existing guidance\n\n## Agent Directives: Mechanical Overrides\n\nKeep external copy.\n",
    )
    .unwrap();

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("codex init must succeed");

    let raw = read_text(&agents_path);
    assert_eq!(
        raw.matches("## Agent Directives: Mechanical Overrides")
            .count(),
        1,
        "Codex AGENTS guidance must not duplicate an external overrides block: {raw}"
    );
}

#[test]
fn test_run_init_claude_writes_symforge_memory_guidance() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("claude init must succeed");

    let memory_path = home.path().join(".claude").join("CLAUDE.md");
    let raw = read_text(&memory_path);

    assert!(
        raw.contains("SYMFORGE START"),
        "Claude memory guidance must include a SymForge marker block: {raw}"
    );
    assert!(
        raw.contains("SymForge MCP"),
        "Claude memory guidance must mention SymForge MCP: {raw}"
    );
    assert!(
        raw.contains("get_file_context"),
        "Claude memory guidance must include tool guidance: {raw}"
    );
    assert!(
        raw.contains("| Task | Call |"),
        "Claude memory guidance must carry the task-to-tool map: {raw}"
    );
    assert!(
        raw.contains("validate_file_syntax"),
        "Claude memory guidance must include config validation guidance: {raw}"
    );
    assert!(
        !raw.contains("## Agent Directives: Mechanical Overrides")
            && !raw.contains("## Tooling Preference"),
        "removed guidance sections must not ship again: {raw}"
    );
}

#[test]
fn test_run_init_claude_preserves_existing_memory_content_and_is_idempotent() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);
    let claude_dir = home.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let memory_path = claude_dir.join("CLAUDE.md");
    std::fs::write(&memory_path, "# Existing memory\n\nKeep this line.\n").unwrap();

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("first claude init must succeed");
    let first = read_text(&memory_path);

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("second claude init must succeed");
    let second = read_text(&memory_path);

    assert!(
        second.contains("Keep this line."),
        "existing Claude memory must survive"
    );
    assert_eq!(first, second, "Claude memory guidance must be idempotent");
}

#[test]
fn test_run_init_claude_never_duplicates_external_overrides_heading() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);
    let claude_dir = home.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let memory_path = claude_dir.join("CLAUDE.md");
    std::fs::write(
        &memory_path,
        "# Existing memory\n\n## Agent Directives: Mechanical Overrides\n\nKeep external copy.\n",
    )
    .unwrap();

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("claude init must succeed");

    let raw = read_text(&memory_path);
    assert_eq!(
        raw.matches("## Agent Directives: Mechanical Overrides")
            .count(),
        1,
        "Claude memory guidance must not duplicate an external overrides block: {raw}"
    );
}

#[test]
fn test_run_init_gemini_only_updates_gemini_files() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Gemini, home.path(), cwd.path(), &binary_path)
        .expect("gemini init must succeed");

    assert!(
        home.path().join(".gemini").join("settings.json").exists(),
        "Gemini config must be created"
    );
    assert!(
        home.path().join(".gemini").join("GEMINI.md").exists(),
        "Gemini guidance must be created"
    );
    assert!(
        !home.path().join(".codex").join("config.toml").exists(),
        "Codex config must not be created for gemini-only init"
    );
    assert!(
        !home.path().join(".claude.json").exists(),
        "Claude config must not be created for gemini-only init"
    );
    assert!(
        !cwd.path().join(".kilocode").join("mcp.json").exists(),
        "Kilo config must not be created for gemini-only init"
    );
}

#[test]
fn test_run_init_gemini_writes_full_symforge_guidance() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Gemini, home.path(), cwd.path(), &binary_path)
        .expect("gemini init must succeed");

    let guidance_path = home.path().join(".gemini").join("GEMINI.md");
    let raw = read_text(&guidance_path);

    assert!(
        raw.contains("| Task | Call |"),
        "Gemini guidance must carry the task-to-tool map: {raw}"
    );
    assert!(
        raw.contains("validate_file_syntax"),
        "Gemini guidance must include config validation guidance: {raw}"
    );
    assert!(
        raw.contains("Raw reads (`get_file_content`/Read) remain correct"),
        "Gemini guidance must keep the raw-read fallback rule: {raw}"
    );
    assert!(
        !raw.contains("## Agent Directives: Mechanical Overrides"),
        "Gemini guidance must not include Claude/Codex-only mechanical overrides: {raw}"
    );
}

#[test]
fn test_run_init_kilo_only_updates_kilo_files() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::KiloCode, home.path(), cwd.path(), &binary_path)
        .expect("kilo init must succeed");

    assert!(
        cwd.path().join(".kilo").join("kilo.jsonc").exists(),
        "Kilo project config must be created"
    );
    assert!(
        home.path()
            .join(".config")
            .join("kilo")
            .join("kilo.jsonc")
            .exists(),
        "Kilo user config must be created"
    );
    assert!(
        cwd.path()
            .join(".kilocode")
            .join("rules")
            .join("symforge.md")
            .exists(),
        "Kilo guidance rules must be created"
    );
    assert!(
        !home.path().join(".codex").join("config.toml").exists(),
        "Codex config must not be created for kilo-only init"
    );
    assert!(
        !home.path().join(".gemini").join("settings.json").exists(),
        "Gemini config must not be created for kilo-only init"
    );
}

#[test]
fn test_run_init_kilo_writes_symforge_rules_guidance() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::KiloCode, home.path(), cwd.path(), &binary_path)
        .expect("kilo init must succeed");

    let rules_path = cwd
        .path()
        .join(".kilocode")
        .join("rules")
        .join("symforge.md");
    let raw = read_text(&rules_path);

    assert!(
        raw.contains("SymForge MCP"),
        "Kilo rules guidance must mention SymForge MCP: {raw}"
    );
    assert!(
        raw.contains("| Task | Call |"),
        "Kilo rules guidance must carry the task-to-tool map: {raw}"
    );
    assert!(
        raw.contains("validate_file_syntax"),
        "Kilo rules guidance must include config validation guidance: {raw}"
    );
    assert!(
        !raw.contains("## Agent Directives: Mechanical Overrides"),
        "Kilo guidance must not include Claude/Codex-only mechanical overrides: {raw}"
    );
}

/// Generated command+args from each stdio writer, then a real MCP session.
/// Config files are temp copies. This does not read or write the runner's
/// home directory (Grok Bot uses `~/.cursor/mcp.json`; the cursor case below
/// is a temp stand-in for that file).
#[test]
fn generated_launch_pair_completes_mcp_initialize_tools_and_health() {
    let binary = std::path::PathBuf::from(env!("CARGO_BIN_EXE_symforge"));
    let binary_arg = binary.display().to_string();
    let expected = if cfg!(windows) {
        binary_arg.replace('/', "\\")
    } else {
        binary_arg.clone()
    };

    let mut launches = Vec::new();
    launches.push(codex_launch(&binary_arg));
    launches.push(json_launch("claude", &binary_arg, |path, bin| {
        symforge::cli::init::register_mcp_server(path, bin)
    }));
    launches.push(json_launch("cursor", &binary_arg, |path, bin| {
        symforge::cli::init::register_cursor_mcp_server(path, bin)
    }));
    launches.push(json_launch("gemini", &binary_arg, |path, bin| {
        symforge::cli::init::register_gemini_mcp_server(path, bin)
    }));
    launches.push(kilo_launch(&binary_arg));
    launches.push(json_launch("omp", &binary_arg, |path, bin| {
        symforge::cli::init::register_omp_mcp_server(path, bin)
    }));
    launches.push(grok_launch(&binary_arg));
    launches.push(desktop_launch(&binary_arg));

    for (name, command, args) in &launches {
        if *name == "claude-desktop" && cfg!(windows) {
            assert!(args.is_empty(), "{name}: {args:?}");
            assert!(std::path::Path::new(command).is_file(), "{command}");
        } else {
            assert_eq!(command, &expected, "{name}");
            assert!(args.is_empty(), "{name}: {args:?}");
        }
        drive_stdio_mcp(name, command, args);
    }
}

fn toml_arg_list(item: &toml_edit::Item) -> Vec<String> {
    item.as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn codex_launch(binary: &str) -> (&'static str, String, Vec<String>) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[mcp_servers.other]
command = "other.exe"

[mcp_servers.symforge]
command = '/fixture/wrapper'
args = ['/fixture/program files/old/symforge']
required = true
enabled = false
"#,
    )
    .unwrap();
    register_codex_mcp_server(&path, binary).unwrap();
    let doc = read_text(&path).parse::<toml_edit::DocumentMut>().unwrap();
    assert_eq!(
        doc["mcp_servers"]["other"]["command"].as_str(),
        Some("other.exe")
    );
    assert_eq!(
        doc["mcp_servers"]["symforge"]["required"].as_bool(),
        Some(true)
    );
    assert_eq!(
        doc["mcp_servers"]["symforge"]["enabled"].as_bool(),
        Some(false)
    );
    let server = &doc["mcp_servers"]["symforge"];
    (
        "codex",
        server["command"].as_str().unwrap().to_string(),
        toml_arg_list(&server["args"]),
    )
}

fn json_launch(
    name: &'static str,
    binary: &str,
    register: fn(&std::path::Path, &str) -> anyhow::Result<()>,
) -> (&'static str, String, Vec<String>) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("mcp.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {
                "other": {"command": "other-binary"},
                "symforge": {
                    "command": "/fixture/wrapper",
                    "args": ["/fixture/program files/old/symforge"],
                    "env": {"KEEP_ENV": "sentinel"}
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    register(&path, binary).unwrap();
    let config: serde_json::Value = serde_json::from_str(&read_text(&path)).unwrap();
    assert_eq!(
        config["mcpServers"]["other"]["command"].as_str(),
        Some("other-binary")
    );
    assert_eq!(
        config["mcpServers"]["symforge"]["env"]["KEEP_ENV"].as_str(),
        Some("sentinel")
    );
    let entry = &config["mcpServers"]["symforge"];
    let args = entry["args"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    (name, entry["command"].as_str().unwrap().to_string(), args)
}

fn kilo_launch(binary: &str) -> (&'static str, String, Vec<String>) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("kilo.jsonc");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "mcp": {
                "symforge": {
                    "type": "local",
                    "command": ["npx", "-y", "symforge"]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    symforge::cli::init::register_kilo_mcp_server(&path, binary).unwrap();
    let config: serde_json::Value = serde_json::from_str(&read_text(&path)).unwrap();
    let command = config["mcp"]["symforge"]["command"].as_array().unwrap();
    let executable = command[0].as_str().unwrap().to_string();
    let args = command
        .iter()
        .skip(1)
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    ("kilo", executable, args)
}

fn grok_launch(binary: &str) -> (&'static str, String, Vec<String>) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[mcp_servers.symforge]\ncommand = '/fixture/wrapper'\nargs = ['/fixture/old/symforge']\n",
    )
    .unwrap();
    symforge::cli::init::register_grok_mcp_server(&path, binary).unwrap();
    let doc = read_text(&path).parse::<toml_edit::DocumentMut>().unwrap();
    let server = &doc["mcp_servers"]["symforge"];
    (
        "grok",
        server["command"].as_str().unwrap().to_string(),
        toml_arg_list(&server["args"]),
    )
}

fn desktop_launch(binary: &str) -> (&'static str, String, Vec<String>) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("claude_desktop_config.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {
                "symforge": {
                    "command": "/fixture/wrapper",
                    "args": ["/fixture/old/symforge"]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    // Public entry resolves home only to reject a temp binary. The config
    // path is the temp file above, not the runner's Claude Desktop config.
    symforge::cli::init::register_claude_desktop_mcp_server(&path, binary).unwrap();
    let config: serde_json::Value = serde_json::from_str(&read_text(&path)).unwrap();
    let entry = &config["mcpServers"]["symforge"];
    let args = entry["args"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    (
        "claude-desktop",
        entry["command"].as_str().unwrap().to_string(),
        args,
    )
}

fn drive_stdio_mcp(name: &str, command: &str, args: &[String]) {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let cwd = TempDir::new().unwrap();
    let mut child = symforge::process_util::hidden_command(command)
        .args(args)
        .current_dir(cwd.path())
        .env("SYMFORGE_AUTO_INDEX", "false")
        .env_remove("SYMFORGE_WORKSPACE_ROOT")
        .env_remove("CLAUDE_PROJECT_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("{name}: spawn {command}: {err}"));

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut text);
        text
    });

    let fail = |child: &mut std::process::Child, why: String| -> ! {
        let _ = child.kill();
        let _ = child.wait();
        panic!("{name}: {why}");
    };

    let write_line = |stdin: &mut std::process::ChildStdin, line: &str| {
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    };
    write_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"symforge-launch-pair","version":"1"}}}"#,
    );
    let init = recv_id(&rx, 1, Duration::from_secs(20));
    if init
        .get("result")
        .and_then(|result| result.get("protocolVersion"))
        .is_none()
    {
        fail(&mut child, format!("initialize failed: {init}"));
    }
    write_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    );
    write_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    let tools = recv_id(&rx, 2, Duration::from_secs(20));
    if tools["result"]["tools"].as_array().is_none() {
        fail(&mut child, format!("tools/list failed: {tools}"));
    }
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        names.contains(&"health"),
        "{name}: health missing from {names:?}"
    );
    write_line(
        &mut stdin,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"health","arguments":{}}}"#,
    );
    let health = recv_id(&rx, 3, Duration::from_secs(20));
    if health.get("error").is_some() || health["result"]["isError"].as_bool() == Some(true) {
        fail(&mut child, format!("health failed: {health}"));
    }

    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                fail(
                    &mut child,
                    "server did not exit after stdin closed".to_string(),
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(err) => fail(&mut child, format!("wait: {err}")),
        }
    }
    let _ = stderr_thread.join();
}

fn recv_id(
    rx: &std::sync::mpsc::Receiver<String>,
    id: u64,
    timeout: std::time::Duration,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + timeout;
    let mut seen = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("timed out waiting for json-rpc id {id}; saw {seen:?}");
        }
        match rx.recv_timeout(remaining) {
            Ok(line) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line)
                    && value.get("id").and_then(|value| value.as_u64()) == Some(id)
                {
                    return value;
                }
                seen.push(line);
            }
            Err(_) => panic!("stdio closed waiting for id {id}; saw {seen:?}"),
        }
    }
}
