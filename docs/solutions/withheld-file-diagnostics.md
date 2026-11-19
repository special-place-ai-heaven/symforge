# Withheld file diagnostics

Every tool that refuses or skips a file withheld by the admission policy gives the same reason: path, rule id(s), match count and the matched line numbers. Matched text is never shown.

## Where lines come from

The manifest records the verdict (rule ids, count) but not where the findings sit; persisting lines would change the snapshot schema. A tool that renders the refusal therefore re-reads the file at refusal time, scans it, keeps only the line numbers and drops the bytes.

- The read is bounded: the shared repository-path resolver must admit the spelling as the file's own name beneath the indexed root (no link on any component, no alias, no escape), the file must be a regular file, and no more than the scan budget (`SECRET_SCAN_MAX_BYTES` plus one byte) is read from the one opened handle.
- Lines are used only when the fresh scan names exactly the recorded rules. If the file moved on, the refusal keeps the recorded verdict without lines rather than pairing it with positions from different content.
- Any doubt yields no lines. The refusal itself never depends on this read.
- `get_symbol` batch computes the refusal once per distinct checked path in a request, and the target count is capped (`MAX_SYMBOL_TARGETS`, shared with the embed lane), so a request cannot multiply the scan cost.
- The embed lane and the sidecar pass their own reader into `refuse_by_policy_naming_lines_via`; the embed lane reads through its `ProjectSourceAuthority`, never through the ambient index root.

## Dropped stance: existence indistinguishability

The degradation view used to be a pure function of the requested path and the manifest, so a demoted file that exists and one that was deleted answered identically. Naming lines needs a read, so a present file now shows lines and an unreadable or deleted one shows the recorded verdict without them. The file's presence in the manifest was already visible; the new difference only reveals whether it is still readable on disk.

## Known follow-up

`read_regular_file_limited` checks the path with `symlink_metadata` and then opens it with `open_for_gate_read`, which follows links. A swap between the two calls is possible. It is shared by every gated read and predates this change; opening without following links (`O_NOFOLLOW` on Unix, `FILE_FLAG_OPEN_REPARSE_POINT` on Windows) or comparing the opened handle's identity with the pre-check needs a platform-specific change and tests on each platform.
