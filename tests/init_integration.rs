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
        config_toml.contains("startup_timeout_sec"),
        "config must tune Codex MCP startup timeout: {config_toml}"
    );
    assert!(
        config_toml.contains("tool_timeout_sec"),
        "config must tune Codex MCP tool timeout: {config_toml}"
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
fn test_init_registers_kilo_mcp_server() {
    let dir = TempDir::new().unwrap();
    let kilo_config_path = dir.path().join(".kilocode").join("mcp.json");
    let binary_path = r"C:\\Users\\user\\.symforge\\bin\\symforge.exe";

    register_kilo_mcp_server(&kilo_config_path, binary_path)
        .expect("register_kilo_mcp_server must succeed");

    let config_json = std::fs::read_to_string(&kilo_config_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap();

    let symforge = &config["mcpServers"]["symforge"];
    assert_eq!(symforge["command"], binary_path);
    assert_eq!(symforge["disabled"], false, "disabled must be false");
    assert_eq!(
        symforge["args"],
        serde_json::json!([]),
        "args must be empty"
    );

    let always_allow = symforge["alwaysAllow"]
        .as_array()
        .expect("alwaysAllow must be an array");
    assert!(
        always_allow.iter().any(|v| v.as_str() == Some("health")),
        "alwaysAllow must include health"
    );
    assert!(
        always_allow
            .iter()
            .any(|v| v.as_str() == Some("batch_rename")),
        "alwaysAllow must include batch_rename"
    );
}

#[test]
fn test_init_kilo_registration_preserves_other_servers() {
    let dir = TempDir::new().unwrap();
    let kilo_config_path = dir.path().join(".kilocode").join("mcp.json");

    let initial = serde_json::json!({
        "mcpServers": {
            "other-server": {
                "command": "other-binary",
                "args": ["stdio"]
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
        config["mcpServers"]["other-server"].is_object(),
        "other MCP servers must be preserved"
    );
    assert!(
        config["mcpServers"]["symforge"].is_object(),
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
    // Npm installs may execute from a temporary extraction path, but init must
    // register the durable home binary for global harnesses.
    let bin_dir = TempDir::new().unwrap();
    let binary_path = bin_dir.path().join("symforge");
    let home_binary = home
        .path()
        .join(".symforge")
        .join("bin")
        .join(if cfg!(windows) {
            "symforge.exe"
        } else {
            "symforge"
        });
    std::fs::create_dir_all(home_binary.parent().unwrap()).unwrap();
    std::fs::write(&home_binary, b"").unwrap();
    std::fs::write(&binary_path, b"").unwrap();

    run_init_with_context(InitClient::All, home.path(), cwd.path(), &binary_path)
        .expect("all-client init must succeed");

    assert!(
        home.path().join(".codex").join("config.toml").exists(),
        "Codex config must be created"
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
        cwd.path().join(".kilocode").join("mcp.json").exists(),
        "Kilo config must be created"
    );
    assert!(
        cwd.path()
            .join(".kilocode")
            .join("rules")
            .join("symforge.md")
            .exists(),
        "Kilo guidance rules must be created"
    );

    let codex_config = read_text(&home.path().join(".codex").join("config.toml"));
    assert!(
        codex_config.contains(&home_binary.display().to_string()),
        "Codex config must use durable home binary: {codex_config}"
    );
    assert!(
        !codex_config.contains(&binary_path.display().to_string()),
        "Codex config must not persist temporary binary path: {codex_config}"
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
        raw.contains("Do not default to broad raw file reads"),
        "Codex AGENTS guidance must encode the stronger source-inspection rule: {raw}"
    );
    assert!(
        raw.contains("## Agent Directives: Mechanical Overrides"),
        "Codex AGENTS guidance must include the mechanical overrides block: {raw}"
    );
    assert!(
        raw.contains("THE \"STEP 0\" RULE"),
        "Codex AGENTS guidance must include the structural cleanup directive: {raw}"
    );
    assert!(
        raw.contains("FORCED VERIFICATION"),
        "Codex AGENTS guidance must include the forced verification directive: {raw}"
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
fn test_run_init_codex_skips_mechanical_overrides_when_already_present_externally() {
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
        raw.contains("Tooling Preference"),
        "Claude memory guidance must include the Tooling Preference section: {raw}"
    );
    assert!(
        raw.contains("validate_file_syntax"),
        "Claude memory guidance must include config validation guidance: {raw}"
    );
    assert!(
        raw.contains("## Agent Directives: Mechanical Overrides"),
        "Claude memory guidance must include the mechanical overrides block: {raw}"
    );
    assert!(
        raw.contains("SUB-AGENT SWARMING"),
        "Claude memory guidance must include the context management directives: {raw}"
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
fn test_run_init_claude_skips_mechanical_overrides_when_already_present_externally() {
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
        raw.contains("Tooling Preference"),
        "Gemini guidance must include the full tooling preference section: {raw}"
    );
    assert!(
        raw.contains("validate_file_syntax"),
        "Gemini guidance must include config validation guidance: {raw}"
    );
    assert!(
        raw.contains("Do not default to broad raw file reads"),
        "Gemini guidance must encode the stronger source-inspection rule: {raw}"
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
        cwd.path().join(".kilocode").join("mcp.json").exists(),
        "Kilo config must be created"
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
        raw.contains("Tooling Preference"),
        "Kilo rules guidance must include the tooling preference section: {raw}"
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
