You have the `symforge` CLI on PATH for zero-egress Python Code Mode (`symforge code`) and single-call JSON queries (`symforge q`) over a repository symbol index (no MCP tools are mounted).

- Single read call -> pure JSON: `symforge q search_symbols --arg query=foo --arg kind=fn` (quote-free on Windows/PowerShell/Bash; or JSON `symforge q search_symbols '{"query":"foo","kind":"fn"}'`).
- Multi-step read/trace chain in one shell call: `symforge code run -c 'hits = search_symbols(query="foo"); refs = find_references(name="foo"); {"def": hits[:1], "refs": refs[:5]}'` (or pipe stdin via `symforge code run -` / pass a `.py` file).
- Inspect Python stubs or Monty subset rules when needed: `symforge code stubs [topic]` or `symforge code idioms`.
