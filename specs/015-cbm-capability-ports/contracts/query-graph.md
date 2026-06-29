# Contract: query_graph (subset)

**Feature**: 015 · **Sprint**: S2 · **US**: US7

## Supported (v1)

- Clauses: `MATCH`, `WHERE`, `RETURN`, `LIMIT`, `DISTINCT`
- Patterns: single labeled node, single relationship hop `(a)-[:CALLS]->(b)`
- WHERE: `=`, `<>`, `<`, `>`, `AND`, `OR`, `NOT`, `IS NULL`, `EXISTS { ... }` (single hop)
- Aggregates: `count(*)`, `count(DISTINCT x)`
- Properties: symbol metadata fields exposed as `n.prop` (e.g. `in_degree`)

## Unsupported (MUST error)

- `MERGE`, `CREATE`, `DELETE`, `SET`
- Variable-length paths `[*1..3]`
- `UNWIND`, `UNION`, subqueries
- Parameters `$param`

Error text MUST start with `unsupported:` (CBM pattern).

## Row limit

Default 1000; hard ceiling 100_000 (CBM-aligned).

## Examples

Dead code:
```cypher
MATCH (f:Function)
WHERE NOT EXISTS { (f)<-[:CALLS]-() }
RETURN f.name, f.path
LIMIT 50
```

Hot paths (when complexity metrics land S5+):
```cypher
MATCH (f:Function)
WHERE f.in_degree > 10
RETURN f.name ORDER BY f.in_degree DESC
LIMIT 20
```
