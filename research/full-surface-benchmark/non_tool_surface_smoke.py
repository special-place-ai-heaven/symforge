# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "tiktoken==0.13.0",
# ]
# ///
"""Smoke-test SymForge resources, resource templates, and prompts over stdio."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
import tempfile
from dataclasses import dataclass
from typing import Any
from urllib.parse import parse_qs, urlencode, urlparse

from mcp_harness import (
    MCP_PROTOCOL_VERSION,
    REDACTED,
    CaptureConfig,
    HarnessError,
    JsonlWriter,
    Sanitizer,
    StdioMcpClient,
    TokenCounter,
    base_record,
    canonical_json,
    capture_record,
    child_environment,
    executable_sha256,
    is_within,
    list_all,
    resolve_server,
    validate_paths,
    wire_json,
)


STATIC_RESOURCES = {
    "symforge://repo/health": "repo-health",
    "symforge://repo/outline": "repo-outline",
    "symforge://repo/map": "repo-map",
    "symforge://repo/changes/uncommitted": "repo-changes-uncommitted",
    "symforge://tools/catalog": "tools-catalog",
    "symforge://glossary": "glossary",
}
RESOURCE_TEMPLATES = {
    "symforge://file/context?path={path}&max_tokens={max_tokens}": "file-context",
    (
        "symforge://file/content?path={path}&start_line={start_line}&end_line={end_line}"
        "&around_line={around_line}&around_match={around_match}"
        "&match_occurrence={match_occurrence}&context_lines={context_lines}"
        "&show_line_numbers={show_line_numbers}&header={header}"
    ): "file-content",
    "symforge://symbol/detail?path={path}&name={name}&kind={kind}": "symbol-detail",
    "symforge://symbol/context?name={name}&file={file}": "symbol-context",
}
PROMPT_ARGUMENTS = {
    "symforge-review": {"path": False, "focus": False},
    "symforge-architecture": {"area": False},
    "symforge-triage": {"symptom": True, "path": False},
    "symforge-onboard": {"area": False},
    "symforge-refactor": {"goal": True, "target": False},
    "symforge-debug": {"error": True, "path": False},
    "symforge-admin": {},
}


@dataclass
class SmokeConfig:
    repo: pathlib.Path
    output: pathlib.Path
    fixture_path: str
    fixture_symbol: str
    fixture_kind: str
    server: str
    server_args: list[str]
    timeout: float
    run_id: str
    case_id: str


def normalize_repo_path(value: str) -> str:
    normalized = value.replace("\\", "/").casefold()
    return normalized.removeprefix("//?/")


def validate_fixture(config: SmokeConfig) -> None:
    relative = pathlib.PurePosixPath(config.fixture_path.replace("\\", "/"))
    if relative.is_absolute() or ".." in relative.parts or not relative.parts:
        raise HarnessError("--fixture-path must be a safe repository-relative path")
    target = config.repo.joinpath(*relative.parts).resolve()
    if not target.is_file() or not is_within(target, config.repo):
        raise HarnessError("--fixture-path must identify a file inside --repo")
    if not re.fullmatch(r"[\w:.$<>-]{1,200}", config.fixture_symbol, re.UNICODE):
        raise HarnessError("--fixture-symbol contains unsupported characters")
    if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]{0,31}", config.fixture_kind):
        raise HarnessError("--fixture-kind contains unsupported characters")


def validate_external_repo(repo: pathlib.Path) -> None:
    project_root = pathlib.Path(__file__).resolve().parents[2]
    if is_within(repo, project_root):
        raise HarnessError(
            "the SymForge checkout cannot be used as the smoke repository"
        )


def validate_static_resources(resources: list[Any]) -> None:
    if len(resources) != len(STATIC_RESOURCES):
        raise HarnessError("resources/list returned the wrong static resource count")
    identities: dict[str, str] = {}
    for resource in resources:
        if not isinstance(resource, dict):
            raise HarnessError("resources/list contained a non-object identity")
        uri = resource.get("uri")
        name = resource.get("name")
        if not isinstance(uri, str) or not isinstance(name, str):
            raise HarnessError("resources/list contained an invalid identity")
        if resource.get("mimeType") != "text/markdown":
            raise HarnessError("static resource MIME type drifted")
        identities[uri] = name
    if identities != STATIC_RESOURCES:
        raise HarnessError(
            "static resource identities drifted from the frozen contract"
        )


def validate_resource_templates(templates: list[Any]) -> None:
    if len(templates) != len(RESOURCE_TEMPLATES):
        raise HarnessError("resources/templates/list returned the wrong count")
    identities: dict[str, str] = {}
    for template in templates:
        if not isinstance(template, dict):
            raise HarnessError("resource template list contained a non-object")
        uri = template.get("uriTemplate")
        name = template.get("name")
        if not isinstance(uri, str) or not isinstance(name, str):
            raise HarnessError("resource template list contained an invalid identity")
        if template.get("mimeType") != "text/markdown":
            raise HarnessError("resource template MIME type drifted")
        identities[uri] = name
    if identities != RESOURCE_TEMPLATES:
        raise HarnessError(
            "resource template identities drifted from the frozen contract"
        )


def validate_prompts(prompts: list[Any]) -> None:
    if len(prompts) != len(PROMPT_ARGUMENTS):
        raise HarnessError("prompts/list returned the wrong prompt count")
    observed: dict[str, dict[str, bool]] = {}
    for prompt in prompts:
        if not isinstance(prompt, dict) or not isinstance(prompt.get("name"), str):
            raise HarnessError("prompts/list contained an invalid prompt identity")
        arguments = prompt.get("arguments", [])
        if not isinstance(arguments, list):
            raise HarnessError("prompt argument schema was not a list")
        schema: dict[str, bool] = {}
        for argument in arguments:
            if not isinstance(argument, dict) or not isinstance(
                argument.get("name"), str
            ):
                raise HarnessError("prompt argument schema contained an invalid entry")
            schema[argument["name"]] = bool(argument.get("required", False))
        observed[prompt["name"]] = schema
    if observed != PROMPT_ARGUMENTS:
        raise HarnessError("prompt identities or argument schemas drifted")


def template_instances(config: SmokeConfig) -> dict[str, tuple[str, str]]:
    path = config.fixture_path.replace("\\", "/")
    symbol = config.fixture_symbol
    return {
        "file-context": (
            "symforge://file/context?" + urlencode({"path": path, "max_tokens": 400}),
            path,
        ),
        "file-content": (
            "symforge://file/content?"
            + urlencode(
                {
                    "path": path,
                    "start_line": 1,
                    "end_line": 20,
                    "show_line_numbers": "true",
                    "header": "true",
                }
            ),
            path,
        ),
        "symbol-detail": (
            "symforge://symbol/detail?"
            + urlencode({"path": path, "name": symbol, "kind": config.fixture_kind}),
            symbol,
        ),
        "symbol-context": (
            "symforge://symbol/context?" + urlencode({"name": symbol, "file": path}),
            symbol,
        ),
    }


def prompt_requests(config: SmokeConfig) -> dict[str, dict[str, str]]:
    path = config.fixture_path.replace("\\", "/")
    symbol = config.fixture_symbol
    return {
        "symforge-review": {"path": path, "focus": f"correctness near {symbol}"},
        "symforge-architecture": {"area": symbol},
        "symforge-triage": {
            "symptom": f"controlled symptom near {symbol}",
            "path": path,
        },
        "symforge-onboard": {"area": symbol},
        "symforge-refactor": {
            "goal": f"review controlled refactor of {symbol}",
            "target": path,
        },
        "symforge-debug": {
            "error": f"controlled error near {symbol}",
            "path": path,
        },
        "symforge-admin": {},
    }


def successful_result(capture: Any, method: str) -> dict[str, Any]:
    response = capture.response_raw
    if not isinstance(response, dict) or "error" in response:
        raise HarnessError(f"{method} did not return a successful response")
    result = response.get("result")
    if not isinstance(result, dict):
        raise HarnessError(f"{method} result was not an object")
    return result


def resource_text(result: dict[str, Any], requested_uri: str) -> str:
    contents = result.get("contents")
    if not isinstance(contents, list) or not contents:
        raise HarnessError("resources/read returned no contents")
    texts: list[str] = []
    for content in contents:
        if not isinstance(content, dict):
            raise HarnessError("resources/read content was not an object")
        if content.get("uri") != requested_uri:
            raise HarnessError("resources/read returned content for a different URI")
        if content.get("mimeType") != "text/markdown":
            raise HarnessError("resources/read returned an unexpected MIME type")
        text = content.get("text")
        if not isinstance(text, str) or not text:
            raise HarnessError("resources/read returned empty non-text content")
        texts.append(text)
    return "\n".join(texts)


def prompt_text(result: dict[str, Any]) -> str:
    messages = result.get("messages")
    if not isinstance(messages, list) or not messages:
        raise HarnessError("prompts/get returned no messages")
    texts: list[str] = []
    for message in messages:
        if not isinstance(message, dict):
            raise HarnessError("prompts/get returned an invalid message")
        content = message.get("content")
        if isinstance(content, dict) and isinstance(content.get("text"), str):
            texts.append(content["text"])
    if not texts:
        raise HarnessError("prompts/get returned no text message")
    return "\n".join(texts)


def write_call(
    writer: JsonlWriter,
    capture_config: CaptureConfig,
    capture: Any,
    counter: TokenCounter,
    kind: str,
    identity: str,
    project_evidence: bool | None,
) -> None:
    record = capture_record(capture_config, capture)
    component = "messages" if kind == "prompt" else "contents"
    raw_result = (capture.response_raw or {}).get("result", {})
    safe_result = (capture.response_safe or {}).get("result", {})
    raw_payload = raw_result.get(component)
    safe_payload = safe_result.get(component)
    if not isinstance(raw_payload, list) or not isinstance(safe_payload, list):
        raise HarnessError("non-tool payload component was not a list")
    raw_counts = counter.metrics(canonical_json(raw_payload))
    safe_counts = counter.metrics(canonical_json(safe_payload))
    record.update(
        {
            "record_type": "non_tool_surface_call",
            "non_tool_kind": kind,
            "identity": identity,
            "project_evidence": project_evidence,
            "schema_free_non_tool_payload_component": f"result.{component}",
            "schema_free_non_tool_payload_basis": (
                f"one canonical JSON serialization of result.{component}; "
                "no per-block summation"
            ),
            "schema_free_non_tool_payload_utf8_bytes": raw_counts["utf8_bytes"],
            "schema_free_non_tool_payload_cl100k": raw_counts["cl100k"],
            "schema_free_non_tool_payload_o200k": raw_counts["o200k"],
            "sanitized_schema_free_non_tool_payload_utf8_bytes": safe_counts[
                "utf8_bytes"
            ],
            "sanitized_schema_free_non_tool_payload_cl100k": safe_counts["cl100k"],
            "sanitized_schema_free_non_tool_payload_o200k": safe_counts["o200k"],
        }
    )
    writer.write(record)


def run_smoke(config: SmokeConfig) -> int:
    config.repo = config.repo.resolve()
    config.output = config.output.resolve()
    validate_paths(config.repo, config.output)
    validate_external_repo(config.repo)
    validate_fixture(config)
    if config.timeout <= 0:
        raise HarnessError("--timeout must be greater than zero")

    server_path = resolve_server(config.server)
    sanitizer = Sanitizer()
    counter = TokenCounter()
    writer = JsonlWriter(config.output, sanitizer, append=False)
    env = child_environment("full", True)
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    capture_config = CaptureConfig(
        mode="manifest",
        repo=config.repo,
        output=config.output,
        server=config.server,
        server_args=config.server_args,
        surface="full",
        auto_index=True,
        timeout=config.timeout,
        protocol_version=MCP_PROTOCOL_VERSION,
        run_id=config.run_id,
        case_id=config.case_id,
        append=False,
    )
    client: StdioMcpClient | None = None
    pending: BaseException | None = None

    try:
        writer.write(
            {
                **base_record(capture_config),
                "record_type": "non_tool_run_metadata",
                "server_executable_sha256": executable_sha256(server_path),
                "server_argument_count": len(config.server_args),
                "tokenizer": counter.metadata(),
                "expected_counts": {"resources": 6, "templates": 4, "prompts": 7},
            }
        )
        client = StdioMcpClient(
            [str(server_path), *config.server_args],
            config.repo,
            env,
            config.timeout,
            sanitizer,
            counter,
        )
        initialize = client.rpc(
            "initialize",
            {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "symforge-non-tool-smoke", "version": "1.0"},
            },
        )
        writer.write(capture_record(capture_config, initialize))
        successful_result(initialize, "initialize")
        initialized = client.notify("notifications/initialized", {})
        writer.write(capture_record(capture_config, initialized))

        resources, resource_metrics = list_all(
            client, writer, capture_config, "resources/list", "resources"
        )
        templates, template_metrics = list_all(
            client,
            writer,
            capture_config,
            "resources/templates/list",
            "resourceTemplates",
        )
        prompts, prompt_metrics = list_all(
            client, writer, capture_config, "prompts/list", "prompts"
        )
        validate_static_resources(resources)
        validate_resource_templates(templates)
        validate_prompts(prompts)

        health_evidence = False
        for uri in STATIC_RESOURCES:
            capture = client.rpc("resources/read", {"uri": uri})
            result = successful_result(capture, "resources/read")
            text = resource_text(result, uri)
            evidence: bool | None = None
            if uri == "symforge://repo/health":
                health_evidence = normalize_repo_path(
                    str(config.repo)
                ) in normalize_repo_path(text)
                if not health_evidence:
                    raise HarnessError(
                        "health resource did not prove the bound project"
                    )
                evidence = True
            write_call(
                writer,
                capture_config,
                capture,
                counter,
                "static_resource",
                uri,
                evidence,
            )

        for name, (uri, anchor) in template_instances(config).items():
            capture = client.rpc("resources/read", {"uri": uri})
            result = successful_result(capture, "resources/read")
            text = resource_text(result, uri)
            if anchor not in text:
                raise HarnessError(
                    "templated resource omitted its path or symbol evidence"
                )
            write_call(
                writer,
                capture_config,
                capture,
                counter,
                "resource_template",
                name,
                True,
            )

        project_name = config.repo.name.casefold()
        prompt_evidence = 0
        for name, arguments in prompt_requests(config).items():
            if set(arguments) != set(PROMPT_ARGUMENTS[name]):
                raise HarnessError(
                    "prompt request did not use its exact declared arguments"
                )
            capture = client.rpc("prompts/get", {"name": name, "arguments": arguments})
            result = successful_result(capture, "prompts/get")
            text = prompt_text(result)
            evidence = project_name in text.casefold()
            if not evidence:
                raise HarnessError("prompt text did not prove the bound project")
            prompt_evidence += 1
            write_call(writer, capture_config, capture, counter, "prompt", name, True)

        writer.write(
            {
                **base_record(capture_config),
                "record_type": "non_tool_summary",
                "status": "ok",
                "static_resources_read": len(STATIC_RESOURCES),
                "resource_templates_read": len(RESOURCE_TEMPLATES),
                "prompts_fetched": len(PROMPT_ARGUMENTS),
                "health_project_evidence": health_evidence,
                "prompt_project_evidence_count": prompt_evidence,
                "list_metrics": {
                    "resources/list": resource_metrics,
                    "resources/templates/list": template_metrics,
                    "prompts/list": prompt_metrics,
                },
            }
        )
    except BaseException as exc:
        pending = exc
        writer.write(
            {
                **base_record(capture_config),
                "record_type": "harness_error",
                "error_type": type(exc).__name__,
                "message": str(exc)
                if isinstance(exc, HarnessError)
                else type(exc).__name__,
            }
        )
    finally:
        if client is not None:
            writer.write(
                {
                    **base_record(capture_config),
                    "record_type": "process_lifecycle",
                    "process_start_ms": client.process_start_ms,
                    **client.close(),
                }
            )
        rows = writer.rows
        writer.close()

    if pending is not None:
        if isinstance(pending, HarnessError):
            raise pending
        raise HarnessError(
            f"unexpected smoke-runner failure ({type(pending).__name__})"
        ) from pending
    return rows


def prompt_definitions() -> list[dict[str, Any]]:
    return [
        {
            "name": name,
            "description": "Harmless fake prompt.",
            "arguments": [
                {"name": argument, "required": required}
                for argument, required in arguments.items()
            ],
        }
        for name, arguments in PROMPT_ARGUMENTS.items()
    ]


def fake_server() -> int:
    secret = "unit" + "-sensitive" + "-surface"
    project = pathlib.Path.cwd()
    for raw_line in sys.stdin.buffer:
        try:
            request = json.loads(raw_line)
        except json.JSONDecodeError:
            continue
        if "id" not in request:
            continue
        method = request.get("method")
        params = request.get("params", {})
        if method == "initialize":
            result: Any = {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {"resources": {}, "prompts": {}},
                "serverInfo": {"name": "non-tool-fake", "version": "0"},
            }
        elif method == "resources/list":
            result = {
                "resources": [
                    {
                        "uri": uri,
                        "name": name,
                        "description": "Harmless fake resource.",
                        "mimeType": "text/markdown",
                    }
                    for uri, name in STATIC_RESOURCES.items()
                ]
            }
        elif method == "resources/templates/list":
            result = {
                "resourceTemplates": [
                    {
                        "uriTemplate": uri,
                        "name": name,
                        "description": "Harmless fake template.",
                        "mimeType": "text/markdown",
                    }
                    for uri, name in RESOURCE_TEMPLATES.items()
                ]
            }
        elif method == "prompts/list":
            result = {"prompts": prompt_definitions()}
        elif method == "resources/read":
            uri = str(params.get("uri", ""))
            parsed = urlparse(uri)
            query = parse_qs(parsed.query)
            anchor = (
                query.get("name", query.get("path", [project.name]))[0]
                if query
                else project.name
            )
            text = f"project_root={project.as_posix()}\nanchor={anchor}"
            if uri == "symforge://glossary":
                text += f'\nAPI_TOKEN="{secret}"'
            result = {
                "contents": [{"uri": uri, "mimeType": "text/markdown", "text": text}]
            }
        elif method == "prompts/get":
            name = str(params.get("name", ""))
            result = {
                "description": "Harmless fake prompt result.",
                "messages": [
                    {
                        "role": "user",
                        "content": {
                            "type": "text",
                            "text": f"Project {project.name}; prompt {name}",
                        },
                    }
                ],
            }
        else:
            response = {
                "jsonrpc": "2.0",
                "id": request["id"],
                "error": {"code": -32601, "message": "Method not found"},
            }
            sys.stdout.buffer.write(wire_json(response).encode("utf-8"))
            sys.stdout.buffer.flush()
            continue
        response = {"jsonrpc": "2.0", "id": request["id"], "result": result}
        sys.stdout.buffer.write(wire_json(response).encode("utf-8"))
        sys.stdout.buffer.flush()
    return 0


def self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="sfbench-non-tool-test-") as root_text:
        root = pathlib.Path(root_text)
        repo = root / "repo"
        repo.mkdir()
        fixture = repo / "fixture.py"
        fixture.write_text("def fixture_symbol():\n    return 1\n", encoding="utf-8")
        output = root / "non-tool.jsonl"
        config = SmokeConfig(
            repo=repo,
            output=output,
            fixture_path="fixture.py",
            fixture_symbol="fixture_symbol",
            fixture_kind="fn",
            server=sys.executable,
            server_args=[str(pathlib.Path(__file__).resolve()), "--_fake-server"],
            timeout=10,
            run_id="self-test",
            case_id="non-tool",
        )
        run_smoke(config)
        persisted = output.read_text(encoding="utf-8")
        if "unit" + "-sensitive" + "-surface" in persisted:
            raise HarnessError("self-test secret sanitizer invariant failed")
        if REDACTED not in persisted or "--_fake-server" in persisted:
            raise HarnessError("self-test fail-closed artifact invariant failed")
        rows = [json.loads(line) for line in persisted.splitlines()]
        calls = [
            row for row in rows if row.get("record_type") == "non_tool_surface_call"
        ]
        if len(calls) != 17 or any(row.get("status") != "ok" for row in calls):
            raise HarnessError("self-test non-tool call coverage invariant failed")
        if sum(row.get("method") == "resources/read" for row in calls) != 10:
            raise HarnessError("self-test resource read count invariant failed")
        if sum(row.get("method") == "prompts/get" for row in calls) != 7:
            raise HarnessError("self-test prompt count invariant failed")
        counter = TokenCounter()
        fake_secret = "unit" + "-sensitive" + "-surface"
        for row in calls:
            component = row["schema_free_non_tool_payload_component"].split(".", 1)[1]
            safe_payload = row["response"]["result"][component]
            safe_expected = counter.metrics(canonical_json(safe_payload))
            raw_payload = json.loads(canonical_json(safe_payload))
            if row["identity"] == "symforge://glossary":
                raw_payload[0]["text"] = raw_payload[0]["text"].replace(
                    REDACTED, fake_secret
                )
            raw_expected = counter.metrics(canonical_json(raw_payload))
            for suffix in ("utf8_bytes", "cl100k", "o200k"):
                if (
                    row[f"schema_free_non_tool_payload_{suffix}"]
                    != raw_expected[suffix]
                ):
                    raise HarnessError("self-test raw non-tool payload count drifted")
                if (
                    row[f"sanitized_schema_free_non_tool_payload_{suffix}"]
                    != safe_expected[suffix]
                ):
                    raise HarnessError(
                        "self-test sanitized non-tool payload count drifted"
                    )
            if (
                row["schema_free_non_tool_payload_cl100k"] <= 0
                or row["schema_free_non_tool_payload_o200k"] <= 0
            ):
                raise HarnessError("self-test non-tool payload count was zero")
        if any(
            not isinstance(row.get("response_cl100k"), int)
            or row["response_cl100k"] <= 0
            or row.get("rpc_ms") is None
            for row in calls
        ):
            raise HarnessError("self-test call measurement invariant failed")
        summary = next(
            row for row in rows if row.get("record_type") == "non_tool_summary"
        )
        if (
            not summary["health_project_evidence"]
            or summary["prompt_project_evidence_count"] != 7
        ):
            raise HarnessError("self-test project evidence invariant failed")
        try:
            validate_static_resources([{"uri": "wrong"}])
        except HarnessError:
            pass
        else:
            raise HarnessError("self-test identity rejection invariant failed")
    print("self-test: PASS")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Smoke-test all SymForge resources, templates, and prompts in one "
            "isolated stdio session. HTTP is intentionally unsupported."
        )
    )
    subparsers = parser.add_subparsers(dest="mode", required=True)
    run = subparsers.add_parser("run", help="Run the sanitized non-tool smoke suite.")
    run.add_argument("--repo", required=True)
    run.add_argument("--output", required=True)
    run.add_argument("--fixture-path", required=True)
    run.add_argument("--fixture-symbol", required=True)
    run.add_argument("--fixture-kind", default="fn")
    run.add_argument("--server", default="symforge")
    run.add_argument(
        "--server-arg",
        action="append",
        default=[],
        help="Repeat for server arguments; values are never persisted.",
    )
    run.add_argument("--timeout", type=float, default=120.0)
    run.add_argument("--run-id", required=True)
    run.add_argument("--case-id", required=True)
    subparsers.add_parser("self-test", help="Run against an internal fake MCP server.")
    return parser


def main(argv: list[str] | None = None) -> int:
    arguments = sys.argv[1:] if argv is None else argv
    if arguments == ["--_fake-server"]:
        return fake_server()
    args = build_parser().parse_args(arguments)
    if args.mode == "self-test":
        return self_test()
    config = SmokeConfig(
        repo=pathlib.Path(args.repo),
        output=pathlib.Path(args.output),
        fixture_path=args.fixture_path,
        fixture_symbol=args.fixture_symbol,
        fixture_kind=args.fixture_kind,
        server=args.server,
        server_args=list(args.server_arg),
        timeout=args.timeout,
        run_id=args.run_id,
        case_id=args.case_id,
    )
    rows = run_smoke(config)
    print(f"non-tool smoke complete: {rows} sanitized JSONL rows")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except HarnessError as exc:
        print(f"non_tool_surface_smoke: {exc}", file=sys.stderr)
        raise SystemExit(2) from None
