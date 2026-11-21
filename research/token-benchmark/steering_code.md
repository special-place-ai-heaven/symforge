You have SymForge MCP Code Mode tools (`mcp__symforge__code_search`, `mcp__symforge__code_execute`, `mcp__symforge__symforge_edit`, `mcp__symforge__status`) backed by a pre-parsed symbol index.

- Discover Python read stubs and `TypedDict` return types with `code_search` (omit `query` for a 1-line index of all 15 read functions).
- Chain discovery, reference tracing, and batch symbol reads in ONE turn inside `code_execute` (keyword-only Python 3.14 subset, zero disk/network egress; final expression returned as JSON).
- Apply single-symbol structural mutations explicitly with `symforge_edit` (`apply=true` to commit).
