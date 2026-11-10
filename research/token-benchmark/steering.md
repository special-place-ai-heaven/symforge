You have SymForge MCP tools (names start with mcp__symforge__) that answer questions about this repository from a symbol index.

- Orientation: get_repo_map, then get_file_context before reading a whole file.
- Finding code: search_symbols for a named function or type, get_symbol for its exact source, search_text for text.
- Usage and impact: find_references for callers, get_symbol_context for a deeper view.
- Edits: edit_plan, then replace_symbol_body, edit_within_symbol or batch_edit; batch_rename for renames.
- Read a raw file only when you need exact text the tools above do not give you.
