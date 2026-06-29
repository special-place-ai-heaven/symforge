# cbm_resolver_rust fixture

Benchmark for SP-0C / SC-004. Manifest schema:

```json
{
  "version": 1,
  "cases": [
    {
      "id": "same_file_direct_call",
      "file": "src/lib.rs",
      "caller_symbol": "main",
      "call_text": "helper()",
      "expected_callee": "helper",
      "expected_strategy": "SameFile",
      "min_confidence": 0.9
    }
  ]
}
```

20 cases: 5 same-file, 3 method, 4 use-path, 3 crate::, 3 negative, 2 macro-unresolved.  
S0 GO ≥60%; S3 ship ≥80%.
