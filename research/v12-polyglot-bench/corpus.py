"""Pinned 6-language polyglot corpus and 30 auto-graded tasks for SymForge 12.0.0 vs RTK."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class RepoSpec:
    repo_id: str
    language: str
    display_name: str
    git_url: str
    commit_sha: str
    indexed_files: int
    large_file: str
    target_symbol: str
    target_line: int
    target_fact: str
    caller_symbol: str
    top_callers: tuple[tuple[str, str, int], ...]
    subsystem_dir: str
    subsystem_top3: tuple[str, ...]
    edit_prep_symbol: str
    edit_prep_file: str
    edit_prep_line: int
    edit_prep_facts: tuple[str, ...]
    literal_token: str


REPOS: tuple[RepoSpec, ...] = (
    RepoSpec(
        repo_id="rust-tokio",
        language="Rust",
        display_name="tokio-rs/tokio",
        git_url="https://github.com/tokio-rs/tokio.git",
        commit_sha="8bdf1e9dfdaa85808bdbe6ac9b0173100fde9951",
        indexed_files=890,
        large_file="tokio/src/runtime/builder.rs",
        target_symbol="new_multi_thread",
        target_line=293,
        target_fact="Builder::new(Kind::MultiThread, 61)",
        caller_symbol="new_multi_thread",
        top_callers=(
            ("benches/fs.rs", "rt", 15),
            ("tokio-util/tests/context.rs", "tokio_context_with_another_runtime", 11),
            ("benches/net_accept.rs", "rt_multi_thread", 58),
        ),
        subsystem_dir="tokio/src/sync/mpsc",
        subsystem_top3=("new", "Chan", "read"),
        edit_prep_symbol="new_multi_thread",
        edit_prep_file="tokio/src/runtime/builder.rs",
        edit_prep_line=293,
        edit_prep_facts=("Builder", "Kind"),
        literal_token="new_multi_thread",
    ),
    RepoSpec(
        repo_id="csharp-polly",
        language="C#",
        display_name="App-vNext/Polly",
        git_url="https://github.com/App-vNext/Polly.git",
        commit_sha="ad7d87bf1c1cee65f4a90f1afb192094111df6eb",
        indexed_files=986,
        large_file="src/Polly/Retry/AsyncRetryTResultSyntax.cs",
        target_symbol="WaitAndRetryAsync",
        target_line=527,
        target_fact="sleepDurationsEnumerable",
        caller_symbol="ResiliencePipelineBuilder",
        top_callers=(
            ("bench/Polly.Core.Benchmarks/TelemetryBenchmark.cs", "Prepare", 15),
            ("bench/Polly.Core.Benchmarks/TelemetryBenchmark.cs", "Build", 40),
            (
                "test/Polly.Core.Tests/CircuitBreaker/CircuitBreakerResiliencePipelineBuilderTests.cs",
                "AddCircuitBreaker_Configure",
                32,
            ),
        ),
        subsystem_dir="src/Polly.Core/CircuitBreaker",
        subsystem_top3=(
            "CircuitStateController",
            "CircuitBreakerStrategyOptions",
            "CircuitBreakerManualControl",
        ),
        edit_prep_symbol="ExecuteAsync",
        edit_prep_file="src/Polly.Core/ResiliencePipeline.Async.cs",
        edit_prep_line=17,
        edit_prep_facts=("ConfigureAwait", "ExecuteCore"),
        literal_token="ResiliencePipelineBuilder",
    ),
    RepoSpec(
        repo_id="ts-tanstack-query",
        language="TypeScript",
        display_name="TanStack/query",
        git_url="https://github.com/TanStack/query.git",
        commit_sha="0827cf2cb22e2c81ef210271c25fc50e9f1e7328",
        indexed_files=5594,
        large_file="packages/query-core/src/queryClient.ts",
        target_symbol="fetchQuery",
        target_line=651,
        target_fact="query.fetch(defaultedOptions)",
        caller_symbol="hashKey",
        top_callers=(
            ("packages/query-core/src/mutationObserver.ts", "setOptions", 123),
            ("packages/preact-query/src/__tests__/fine-grained-persister.test.tsx", "hash", 28),
            ("packages/query-core/src/queryClient.ts", "setQueryDefaults", 953),
        ),
        subsystem_dir="packages/query-core/src",
        subsystem_top3=("QueryClient", "QueryObserver", "Query"),
        edit_prep_symbol="fetchQuery",
        edit_prep_file="packages/query-core/src/queryClient.ts",
        edit_prep_line=651,
        edit_prep_facts=("defaultQueryOptions", "FetchQueryOptions"),
        literal_token="hashKey",
    ),
    RepoSpec(
        repo_id="python-fastapi",
        language="Python",
        display_name="fastapi/fastapi",
        git_url="https://github.com/fastapi/fastapi.git",
        commit_sha="05e6df7e906c3decba377c37548019973f40bf15",
        indexed_files=3003,
        large_file="fastapi/routing.py",
        target_symbol="run_endpoint_function",
        target_line=372,
        target_fact="run_in_threadpool",
        caller_symbol="get_dependant",
        top_callers=(
            ("fastapi/dependencies/utils.py", "get_parameterless_sub_dependant", 155),
            ("fastapi/dependencies/utils.py", "solve_dependencies", 717),
            (
                "tests/test_parameter_extraction.py",
                "test_parameter_model_extraction_has_linear_scan_cost",
                46,
            ),
        ),
        subsystem_dir="fastapi/dependencies",
        subsystem_top3=("analyze_param", "solve_dependencies", "_get_body_field"),
        edit_prep_symbol="run_endpoint_function",
        edit_prep_file="fastapi/routing.py",
        edit_prep_line=372,
        edit_prep_facts=("run_in_threadpool", "Dependant"),
        literal_token="get_dependant",
    ),
    RepoSpec(
        repo_id="java-gson",
        language="Java",
        display_name="google/gson",
        git_url="https://github.com/google/gson.git",
        commit_sha="e5f02ab6c24567d3ca5200afc4076337746b3349",
        indexed_files=315,
        large_file="gson/src/main/java/com/google/gson/Gson.java",
        target_symbol="getDelegateAdapter",
        target_line=472,
        target_fact="getAdapter(type)",
        caller_symbol="getDelegateAdapter",
        top_callers=(
            (
                "extras/src/main/java/com/google/gson/interceptors/InterceptorFactory.java",
                "create",
                36,
            ),
            ("gson/src/test/java/com/google/gson/GsonTest.java", "testGetDelegateAdapter", 404),
            (
                "extras/src/main/java/com/google/gson/typeadapters/PostConstructAdapterFactory.java",
                "create",
                40,
            ),
        ),
        subsystem_dir="gson/src/main/java/com/google/gson/internal/bind",
        subsystem_top3=("ReflectiveTypeAdapterFactory", "JsonTreeReader", "JsonTreeWriter"),
        edit_prep_symbol="getDelegateAdapter",
        edit_prep_file="gson/src/main/java/com/google/gson/Gson.java",
        edit_prep_line=472,
        edit_prep_facts=("requireNonNull", "TypeAdapter"),
        literal_token="getDelegateAdapter",
    ),
    RepoSpec(
        repo_id="go-gh-cli",
        language="Go",
        display_name="cli/cli",
        git_url="https://github.com/cli/cli.git",
        commit_sha="de06af2c2ea536d5722fda35dc2001565f9b9f64",
        indexed_files=1601,
        large_file="pkg/cmd/pr/create/create.go",
        target_symbol="submitPR",
        target_line=1096,
        target_fact="PrintListenHint",
        caller_symbol="NewCmdCreate",
        top_callers=(
            ("pkg/cmd/agent-task/agent_task.go", "NewCmdAgentTask", 64),
            ("pkg/cmd/agent-task/create/create_test.go", "TestNewCmdCreate", 100),
            ("pkg/cmd/discussion/discussion.go", "NewCmdDiscussion", 40),
        ),
        subsystem_dir="pkg/cmd/pr/create",
        subsystem_top3=("NewCreateContext", "createRun", "NewCmdCreate"),
        edit_prep_symbol="submitPR",
        edit_prep_file="pkg/cmd/pr/create/create.go",
        edit_prep_line=1096,
        edit_prep_facts=("AddMetadataToIssueParams", "CreateOptions"),
        literal_token="NewCmdCreate",
    ),
)


def build_tasks() -> list[dict[str, Any]]:
    """Build all 30 deterministic auto-graded tasks (5 archetypes x 6 languages)."""
    tasks: list[dict[str, Any]] = []
    for repo in REPOS:
        tasks.extend(
            [
                {
                    "task_id": f"{repo.repo_id}__T1_symbol_slice",
                    "repo_id": repo.repo_id,
                    "language": repo.language,
                    "archetype": "T1_symbol_slice",
                    "prompt": (
                        f"Inspect `{repo.target_symbol}` (around line {repo.target_line}) in "
                        f"`{repo.large_file}` and report its implementation body details."
                    ),
                    "required_substrings": [repo.target_symbol, repo.target_fact],
                },
                {
                    "task_id": f"{repo.repo_id}__T2_caller_chain",
                    "repo_id": repo.repo_id,
                    "language": repo.language,
                    "archetype": "T2_caller_chain",
                    "prompt": (
                        f"Find call sites of `{repo.caller_symbol}` across the repository and "
                        f"retrieve the bodies of 3 distinct enclosing caller functions."
                    ),
                    "required_substrings": [c[1] for c in repo.top_callers[:2]],
                },
                {
                    "task_id": f"{repo.repo_id}__T3_subsystem_orient",
                    "repo_id": repo.repo_id,
                    "language": repo.language,
                    "archetype": "T3_subsystem_orient",
                    "prompt": (
                        f"Survey `{repo.subsystem_dir}` and identify the 3 largest symbols "
                        f"(functions or classes/structs) with their line spans."
                    ),
                    "required_substrings": list(repo.subsystem_top3[:2]),
                },
                {
                    "task_id": f"{repo.repo_id}__T4_edit_prep",
                    "repo_id": repo.repo_id,
                    "language": repo.language,
                    "archetype": "T4_edit_prep",
                    "prompt": (
                        f"Prepare a refactor bundle for `{repo.edit_prep_symbol}` in "
                        f"`{repo.edit_prep_file}` (line {repo.edit_prep_line}): gather its "
                        f"direct callers, callees, and referenced type definitions."
                    ),
                    "required_substrings": list(repo.edit_prep_facts),
                },
                {
                    "task_id": f"{repo.repo_id}__T5_literal_git_hybrid",
                    "repo_id": repo.repo_id,
                    "language": repo.language,
                    "archetype": "T5_literal_git_hybrid",
                    "prompt": (
                        f"Count exact literal occurrences of `{repo.literal_token}` across the "
                        f"repository and summarize recent git commit history."
                    ),
                    "required_substrings": [repo.commit_sha[:7]],
                },
            ]
        )
    return tasks


def grade_output(payload_text: str, required_substrings: list[str]) -> dict[str, Any]:
    """Deterministic zero-LLM oracle verifying every required ground-truth fact."""
    missing = [needle for needle in required_substrings if needle not in payload_text]
    return {
        "passed": len(missing) == 0,
        "matched": len(required_substrings) - len(missing),
        "total": len(required_substrings),
        "missing": missing,
    }
