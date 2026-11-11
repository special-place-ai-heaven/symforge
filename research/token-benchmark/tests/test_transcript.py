import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import transcript  # noqa: E402

TOOLS = {"native": ["Bash", "Read", "Grep", "Glob", "Edit", "Write"], "symforge_pattern": "mcp__symforge__*",
         "client_meta": ["ToolSearch", "TodoWrite"], "disallowed": ["WebFetch"]}


def stream(*events):
    return [json.dumps(e) for e in events]


class TranscriptTests(unittest.TestCase):
    def test_usage_is_counted_once_per_message_id(self):
        u = {"input_tokens": 10, "cache_creation_input_tokens": 100, "cache_read_input_tokens": 1000, "output_tokens": 5}
        lines = stream(
            {"type": "system", "subtype": "init", "model": "m", "claude_code_version": "9.9", "tools": ["Read"]},
            {"type": "assistant", "message": {"id": "a", "usage": u, "content": [{"type": "tool_use", "id": "t1", "name": "Read"}]}},
            {"type": "assistant", "message": {"id": "a", "usage": u, "content": [{"type": "tool_use", "id": "t2", "name": "Grep"}]}},
            {"type": "user", "message": {"content": [{"type": "tool_result", "content": "abcd" * 10}]}},
            {"type": "assistant", "message": {"id": "b", "usage": u, "content": [{"type": "text", "text": "ANSWER: x"}]}},
            {"type": "result", "subtype": "success", "duration_ms": 1234, "result": "ANSWER: final"},
        )
        m = transcript.parse_stream(lines)
        self.assertEqual(m["round_trips"], 2)
        self.assertEqual(m["tool_calls"], 2)
        self.assertEqual(m["total_tokens"], 2 * 1115)
        self.assertEqual(m["cache_read_tokens"], 2000)
        self.assertEqual(m["tool_result_chars"], 40)
        self.assertEqual(m["answer"], "ANSWER: final")
        self.assertEqual(m["client_version"], "9.9")
        self.assertTrue(m["has_result"])

    def test_rule_sentinel_counted_outside_assistant_text_only(self):
        lines = stream(
            {"type": "assistant", "message": {"id": "a", "content": [{"type": "text", "text": "SENT"}]}},
            {"type": "user", "message": {"content": [{"type": "tool_result", "content": "rule SENT here"}]}},
            {"type": "system", "subtype": "hook_response", "text": "SENT"},
        )
        m = transcript.parse_stream(lines, "SENT")
        self.assertEqual(m["rule_sentinel_hits"], 2)
        self.assertEqual(m["hook_events"], 1)

    def test_malformed_lines_and_missing_result_are_reported(self):
        m = transcript.parse_stream(["not json", ""])
        self.assertEqual(m["malformed_lines"], 1)
        self.assertFalse(m["has_result"])


class AllowListTests(unittest.TestCase):
    def test_native_arm_rejects_symforge_and_web_tools(self):
        pats = transcript.allowed_patterns({"mcp": False}, TOOLS)
        v = transcript.allowlist_violations({"Read": 2, "mcp__symforge__search_text": 1, "WebFetch": 1, "ToolSearch": 1}, pats)
        self.assertEqual(v, ["WebFetch", "mcp__symforge__search_text"])

    def test_symforge_arm_admits_symforge_tools_only_by_prefix(self):
        pats = transcript.allowed_patterns({"mcp": True}, TOOLS)
        self.assertEqual(transcript.allowlist_violations({"mcp__symforge__get_symbol": 1, "Bash": 1}, pats), [])
        self.assertEqual(transcript.allowlist_violations({"mcp__other__x": 1, "Task": 1}, pats), ["Task", "mcp__other__x"])


if __name__ == "__main__":
    unittest.main()
