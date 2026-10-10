//! Pure prompt instructions shared by MCP and embedded hosts.

const SURFACE_NOTE: &str = "> Surface note: the tool names in these steps are the full-surface \
spellings, directly callable on the default surface. If you have opted into the compact surface \
(`SYMFORGE_SURFACE=compact`) you have only three tools — \
`symforge` (read/explore: pass a natural-language `query` such as \"find symbol X\", \
\"who calls X\", or \"what changed\", optionally with an `intent` hint of \
orient/find/read/trace/impact/meta/auto), `symforge_edit` (structural edits), and \
`status` (which names your active surface). When on compact, route each step below through \
`symforge`/`symforge_edit`.\n";

pub(crate) fn build_knowledge_hygiene_instructions(
    project_name: &str,
    path_prefix: Option<&str>,
) -> String {
    if path_prefix.is_some_and(|path| crate::knowledge::guard_query(path).is_err()) {
        return "Knowledge hygiene prompt rejected by repository safety policy. No review, proposal, cache entry, or mutation workflow was created."
            .to_string();
    }
    let scope = path_prefix
        .map(|path| format!("path_prefix=\"{path}\""))
        .unwrap_or_else(|| "repository scope".to_string());
    format!(
        "Review repository knowledge hygiene for project '{project_name}' in {scope}.\n\n\
         1. Call review_knowledge(mode=\"summary\", source_scope=\"current\") and preserve its source identity, complete-plan review_hash, top_result_hash, and coverage.\n\
         2. Use review_knowledge(mode=\"remediation\") for bounded proposals. Inspect exact document dossiers with mode=\"document\" and corroborate code anchors through search_knowledge or context sections=[\"knowledge\"].\n\
         3. Treat lifecycle, authority domain, aggregate code evidence, and voice as independent axes. Never infer ownership, implementation status, conflict, or deletion from age, contributors, lexical similarity, or prose judgment.\n\
         4. Keep every proposal advisory. Report protected roles, unique-content checks, inbound live links, source-local ownership evidence, temporal coverage, and every unmet precondition. Deletion_candidate is evidence-only; this workflow never moves, edits, or deletes a document.\n\
         5. Present the smallest supported action and confidence basis, then STOP for explicit user approval. This prompt cannot approve an action or mutate policy. Only after approval may a separate curate_knowledge preview be requested; preview must precede any apply.\n\
         6. Do not copy raw secret-positive content into findings, plans, diagnostics, or follow-up prompts."
    )
}

pub(crate) fn build_admin_instructions(project_name: &str, dashboard_url: Option<&str>) -> String {
    match dashboard_url {
        Some(url) => format!(
            "## SymForge Operator Dashboard for '{project_name}'\n\
             \n\
             The operator dashboard is running. Open it here:\n\
             \n\
             {url}\n\
             \n\
             This is the live SymForge admin view (index health, harness attach status, \
             change activity). If the link does not open in your environment, copy the URL \
             into a browser."
        ),
        None => format!(
            "## SymForge Operator Dashboard for '{project_name}'\n\
             \n\
             No operator dashboard is currently running for this project.\n\
             \n\
             Start it by running the CLI verb in a terminal at the project root:\n\
             \n\
             ```\n\
             symforge admin\n\
             ```\n\
             \n\
             `symforge admin` prints the dashboard URL. If it STARTS a new server (nothing \
             was running on the remembered port) it stays in the foreground and keeps serving \
             until you stop it (Ctrl-C); if it REUSES a server already running, it returns \
             immediately (the dashboard stays up because another process owns it). This prompt \
             only reports an already-running dashboard — it does not start the server itself."
        ),
    }
}

pub(crate) fn build_code_review_instructions(
    project_name: &str,
    path: Option<&str>,
    focus: Option<&str>,
) -> String {
    let target = path.map_or(
        "Start from `what_changed(uncommitted=true, code_only=true)` to find all modified files \
         (the path list caps at 200 — narrow with `path_prefix` if it truncates)."
            .to_string(),
        |p| format!("Start with the target path '{p}'."),
    );
    let focus = focus.map_or(String::new(), |f| {
        format!("\n\nPay special attention to: {f}.")
    });

    format!(
        "## Code Review Workflow for '{project_name}'\n\
         \n\
         {SURFACE_NOTE}\
         \n\
         ### Step 1: Scope the Review\n\
         {target}\n\
         - Call `diff_symbols(code_only=true)` to see which symbols changed\n\
         - Call `detect_impact(scope=\"symbols\")` for the change's blast radius vs the base branch (origin/main by default; each list caps at 200 with {{total,returned,truncated}} pagination). Surface caveat: through the compact `symforge` facade `intent=impact` yields the file-scoped impact only — `scope=\"symbols\"` needs the full surface\n\
         - If > 20 symbols changed, use `diff_symbols(compact=true)` first for overview\n\
         \n\
         ### Step 2: Prioritize by Risk\n\
         For each changed symbol:\n\
         - Call `find_references(name=\"<sym>\", compact=true)` — symbols with >5 callers are HIGH RISK\n\
         - Call `get_symbol_context(name=\"<sym>\", verbosity=\"signature\")` — check for signature changes\n\
         \n\
         ### Step 3: Deep Review (high-risk symbols only)\n\
         - Call `get_symbol_context(name=\"<sym>\", bundle=true)` to see the symbol + all type deps\n\
         - Check: Are all callers still compatible with the change?\n\
         - Check: Did any type dependency change shape?\n\
         - Check: Are there missing error-handling paths?\n\
         \n\
         ### Step 4: Check for Missing Tests\n\
         - For each modified symbol, search for corresponding test functions\n\
         - Call `search_symbols(query=\"test_<sym_name>\", include_tests=true)`\n\
         - Flag any changed symbols without test coverage\n\
         \n\
         ### Step 5: Report\n\
         Summarize: what changed, risk assessment per symbol, broken contracts, missing tests.{focus}"
    )
}

pub(crate) fn build_architecture_map_instructions(
    project_name: &str,
    area: Option<&str>,
) -> String {
    let area_note = area.map_or(String::new(), |a| {
        format!("\n\nFocus area: '{a}'. Prioritize this subsystem and its connections.")
    });

    format!(
        "## Architecture Mapping Workflow for '{project_name}'\n\
         \n\
         {SURFACE_NOTE}\
         \n\
         ### Step 1: Get the Big Picture\n\
         - Read the repo map resource (attached) for directory structure and key types\n\
         - Doctrine: the map orients; the tools prove. Treat the repo map as a ranked starting point, not an inventory.\n\
         - Absence from the map is not absence from the repo - confirm with `search_symbols` / `search_text` before concluding something is missing.\n\
         - Call `get_repo_map(detail=\"tree\", depth=2)` to see the file tree with symbol counts\n\
         - Identify the top-level modules/crates/packages\n\
         \n\
         ### Step 2: Map Subsystem Boundaries\n\
         For each major directory:\n\
         - Call `get_file_context(path=\"<dir>/mod.rs\", sections=[\"outline\",\"imports\"])` (or equivalent entry file)\n\
         - Note: which modules import which? What are the public exports?\n\
         - Call `find_dependents(path=\"<key_file>\", compact=true)` for core files\n\
         \n\
         ### Step 3: Identify the Core Types\n\
         - Call `search_symbols(kind=\"struct\", limit=20)` to find the main data structures\n\
         - For each core struct: `get_symbol_context(name=\"<struct>\", verbosity=\"signature\")` to see its shape\n\
         - Call `find_references(name=\"<struct>\", mode=\"implementations\")` to find trait impls\n\
         \n\
         ### Step 4: Trace Data Flow\n\
         - Pick 2-3 key entry points (main, handlers, API routes)\n\
         - Call `get_symbol_context(name=\"<entry>\", sections=[])` for full trace: callers → callees → types\n\
         - Follow the call chain 2-3 levels deep to map the hot path\n\
         \n\
         ### Step 5: Report\n\
         Produce: subsystem diagram, ownership boundaries, key data flows, and which files/symbols to investigate next.{area_note}"
    )
}

pub(crate) fn build_failure_triage_instructions(
    project_name: &str,
    symptom: &str,
    path: Option<&str>,
) -> String {
    let hotspot = path.map_or(String::new(), |p| {
        format!("\n\nInitial hotspot: '{p}'. Start investigation here.")
    });

    format!(
        "## Failure Triage Workflow for '{project_name}'\n\
         Symptom: {symptom}\n\
         \n\
         {SURFACE_NOTE}\
         \n\
         ### Step 1: Check Recent Changes\n\
         - Call `what_changed(uncommitted=true, code_only=true, include_symbol_diff=true)` (path list caps at 200 — narrow with `path_prefix` if it truncates)\n\
         - If the symptom appeared recently, the bug is likely in uncommitted changes\n\
         - For a suspected regression, `detect_impact` shows what changed vs the base branch (origin/main by default) and its blast radius\n\
         - Check `diff_symbols()` — which functions were modified?\n\
         \n\
         ### Step 2: Locate the Failure Point\n\
         - Extract key terms from the symptom (error message, function name, file path)\n\
         - Call `search_text(query=\"<error_text>\")` to find where the error originates\n\
         - Call `inspect_match(path=\"<file>\", line=<N>)` to see the enclosing symbol and context\n\
         \n\
         ### Step 3: Trace the Call Chain\n\
         - Call `get_symbol_context(name=\"<failing_fn>\", sections=[])` for full trace\n\
         - Follow callers: who invokes this function? What inputs does it receive?\n\
         - Follow callees: what dependencies could be causing the failure?\n\
         \n\
         ### Step 4: Check Type Contracts\n\
         - Call `get_symbol_context(name=\"<failing_fn>\", bundle=true)` to see all type deps\n\
         - Did any struct/enum change shape recently? (compare with diff_symbols output)\n\
         - Are there None/null paths not handled?\n\
         \n\
         ### Step 5: Narrow to Root Cause\n\
         - If in changed code: the diff is the likely root cause\n\
         - If in unchanged code: look for dependency changes or environment issues\n\
         - Call `search_text(query=\"<suspect_pattern>\", follow_refs=true)` to check impact\n\
         \n\
         ### Step 6: Report\n\
         Root cause, affected scope (how many callers), suggested fix, and verification steps.{hotspot}",
        symptom = symptom
    )
}

pub(crate) fn build_onboard_instructions(project_name: &str, area: Option<&str>) -> String {
    let area_note = area.map_or(String::new(), |a| {
        format!("\n\nStart with the '{a}' area before expanding to the rest.")
    });

    format!(
        "## Codebase Onboarding Workflow for '{project_name}'\n\
         \n\
         {SURFACE_NOTE}\
         \n\
         ### Step 1: Project Overview (2 minutes)\n\
         - Read the repo map resource (attached) for structure and languages\n\
         - Doctrine: the map orients; the tools prove. Treat the repo map as a ranked starting point, not an inventory.\n\
         - Absence from the map is not absence from the repo - confirm with `search_symbols` / `search_text` before concluding something is missing.\n\
         - Call `get_repo_map(detail=\"tree\", depth=2)` to see directory layout\n\
         - Identify: What language? How many modules? What's the entry point?\n\
         \n\
         ### Step 2: Understand the Architecture (5 minutes)\n\
         - Call `explore(query=\"main entry point\", depth=2)` to find the main function/handler\n\
         - For each top-level directory, call `get_file_context(path=\"<dir>/mod.rs\", sections=[\"outline\"])`\n\
         - Map: what are the 3-5 core modules and what does each do?\n\
         \n\
         ### Step 3: Find the Core Types (3 minutes)\n\
         - Call `search_symbols(kind=\"struct\", limit=15)` to find main data structures\n\
         - Call `search_symbols(kind=\"trait\", limit=10)` to find key abstractions\n\
         - For the top 3 types: `get_symbol(name=\"<type>\")` to read their definition\n\
         \n\
         ### Step 4: Trace a Key Flow (5 minutes)\n\
         - Pick the most important entry point (main, handle_request, process, etc.)\n\
         - Call `get_symbol_context(name=\"<entry>\", sections=[\"dependents\",\"siblings\"])` for full trace\n\
         - Follow 2-3 levels of callees to understand the hot path\n\
         \n\
         ### Step 5: Check Test Patterns (2 minutes)\n\
         - Call `search_files(query=\"test\")` to find test files\n\
         - Call `get_file_context(path=\"<test_file>\", sections=[\"outline\"])` on one test file\n\
         - Note: how are tests organized? What patterns are used?\n\
         \n\
         ### Step 6: Summary\n\
         Produce a concise mental model: purpose, architecture, core types, data flow, test approach.{area_note}"
    )
}

pub(crate) fn build_refactor_instructions(
    project_name: &str,
    goal: &str,
    target: Option<&str>,
) -> String {
    let target_note = target.map_or(String::new(), |t| format!("\n\nStarting point: '{t}'."));

    format!(
        "## Refactoring Workflow for '{project_name}'\n\
         Goal: {goal}\n\
         \n\
         {SURFACE_NOTE}\
         \n\
         ### Step 1: Understand Current State\n\
         - Call `search_symbols(query=\"<target>\")` to find the symbol(s) involved\n\
         - Call `get_symbol_context(name=\"<sym>\", bundle=true)` to see the full definition + type deps\n\
         - Call `find_references(name=\"<sym>\", compact=true)` to count all usage sites\n\
         \n\
         ### Step 2: Assess Impact Radius\n\
         - How many files reference this symbol? (from find_references)\n\
         - Call `find_dependents(path=\"<file>\", compact=true)` to see file-level impact\n\
         - Are there >10 callers? If so, this is a high-risk refactor — plan carefully\n\
         \n\
         ### Step 3: Plan the Edit Sequence\n\
         Based on the refactor type:\n\
         - **Rename**: Use `batch_rename(dry_run=true)` to preview all changes\n\
         - **Extract**: Identify the code to extract with `get_symbol`, then `edit_within_symbol` + `insert_symbol`\n\
         - **Restructure**: Use `batch_edit(dry_run=true)` for multi-symbol changes\n\
         - **Delete**: Check `find_references` first — ensure zero callers before `delete_symbol`\n\
         \n\
         ### Step 4: Execute with dry_run First\n\
         - Always run with `dry_run=true` first to preview\n\
         - Review the preview for unintended changes\n\
         - Then execute without dry_run\n\
         \n\
         ### Step 5: Verify\n\
         - Call `analyze_file_impact(path=\"<changed_file>\")` for each modified file\n\
         - Call `detect_impact(scope=\"symbols\")` for the whole change's blast radius vs the base branch (origin/main by default; each list caps at 200 with {{total,returned,truncated}} pagination)\n\
         - Check for stale references in the impact report\n\
         - Search for any remaining old names: `search_text(query=\"<old_name>\")`{target_note}",
        goal = goal
    )
}

pub(crate) fn build_debug_instructions(
    project_name: &str,
    error: &str,
    path: Option<&str>,
) -> String {
    let path_note = path.map_or(String::new(), |p| format!("\n\nSuspected file: '{p}'."));

    format!(
        "## Debugging Workflow for '{project_name}'\n\
         Error: {error}\n\
         \n\
         {SURFACE_NOTE}\
         \n\
         ### Step 1: Find the Error Origin\n\
         - Extract the key error text or pattern from the error message\n\
         - Call `search_text(query=\"<error_pattern>\")` to find where it's generated\n\
         - If it's a function name: `search_symbols(query=\"<fn_name>\")`\n\
         - Call `inspect_match(path=\"<file>\", line=<N>)` on the match for full context\n\
         \n\
         ### Step 2: Understand the Failing Function\n\
         - Call `get_symbol(name=\"<fn>\")` to read the full body (`path` is optional — name alone resolves; add `path`/`symbol_line` only to disambiguate a shared name)\n\
         - Call `get_symbol_context(name=\"<fn>\", sections=[])` for callers + callees + types\n\
         - Map the data flow: what goes in, what comes out, what can fail?\n\
         \n\
         ### Step 3: Check Recent Changes (is this a regression?)\n\
         - Call `what_changed(uncommitted=true, include_symbol_diff=true)` (path list caps at 200 — narrow with `path_prefix` if it truncates)\n\
         - Call `detect_impact` to see what changed vs the base branch (origin/main by default) and whether the blast radius reaches the failing function\n\
         - Did the failing function change recently?\n\
         - Did any of its dependencies change? Check `diff_symbols()`\n\
         \n\
         ### Step 4: Check Callers\n\
         - Call `find_references(name=\"<fn>\", compact=true)` to find all call sites\n\
         - Are callers passing unexpected inputs? Check their code with `get_symbol`\n\
         - Look for pattern mismatches (wrong types, missing error handling)\n\
         \n\
         ### Step 5: Check Dependencies\n\
         - Call `get_symbol_context(name=\"<fn>\", bundle=true)` for all type deps\n\
         - Did any struct/enum change shape? Are there Option/Result paths not handled?\n\
         - Call `search_text(query=\"unwrap()\", glob=\"<suspect_file>\")` to find panic points\n\
         \n\
         ### Step 6: Root Cause Report\n\
         Root cause, evidence trail, affected scope, suggested fix, and how to verify.{path_note}",
        error = error
    )
}
