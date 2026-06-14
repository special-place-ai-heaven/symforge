# A-029 T2 equivalence spike corpora

External reference repos for Phase 2 P2-S5 / A-029 (not shipped as product fixtures).

## Clone commands

```bash
root="tests/fixtures/a029-t2"
git clone --depth 1 https://github.com/tokio-rs/tokio.git "$root/tokio"
git clone --depth 1 https://github.com/django/django.git "$root/django"
```

## Run spike

```bash
cargo build -p symforge
node scripts/a029-t2-spike.cjs target/debug/symforge docs/research/a029-t2-results.json
```

Task definitions: [`tasks.jsonl`](./tasks.jsonl)
