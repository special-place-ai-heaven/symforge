> **SYNTHETIC DATA.** Every number below comes from fake_client.py replaying scripted transcripts
> against a bundled fixture repository. Nothing here is a measurement of SymForge or of any model.

# Token benchmark report: dryrun-synthetic

- Model: `synthetic-fake-client`
- Headline pair: a vs c
- Rule outcome: **REJECT**; whole-task token claim: **NOT ADMISSIBLE (synthetic data)**
- Cell status counts: {"invalid_allowlist": 1, "valid": 239}
- Cells whose recorded hook/rule configuration does not match their mode: 0 of 240

## Decision rule results

| mode | pair | total saving | median saving | pairs won | excluded pairs | pass counts | valid fraction | verdict | losing categories |
|---|---|---|---|---|---|---|---|---|---|
| off | a vs c (headline) | -5.4% | -4.4% | 3/8 | 0 | {'a': 36, 'c': 34} | 0.99 | REJECT | debugging, single-file-edit, cross-module-edit, rename-refactor, review |
| off | a vs b | +6.4% | +3.1% | 6/8 | 0 | {'a': 36, 'b': 39} | 0.99 | ACCEPT | rename-refactor, review |
| on | a vs c (headline) | +1.5% | +9.6% | 6/8 | 0 | {'a': 36, 'c': 35} | 1.00 | REJECT | debugging, rename-refactor |
| on | a vs b | +11.7% | +14.0% | 7/8 | 0 | {'a': 36, 'b': 35} | 1.00 | REJECT | rename-refactor |

Saving is native minus SymForge arm over native, on median total tokens processed (input + cache write + cache read + output) per pair.

### Per category: a vs c, mode off

| category | pairs | won | median saving |
|---|---|---|---|
| discovery | 1 | 1 | +21.3% |
| tracing | 1 | 1 | +16.2% |
| unfamiliar-module | 1 | 1 | +3.3% |
| debugging | 1 | 0 | -5.7% |
| single-file-edit | 1 | 0 | -10.6% |
| cross-module-edit | 1 | 0 | -3.1% |
| rename-refactor | 1 | 0 | -36.6% |
| review | 1 | 0 | -12.2% |

### Per category: a vs b, mode off

| category | pairs | won | median saving |
|---|---|---|---|
| discovery | 1 | 1 | +36.5% |
| tracing | 1 | 1 | +46.9% |
| unfamiliar-module | 1 | 1 | +15.4% |
| debugging | 1 | 1 | +2.6% |
| single-file-edit | 1 | 1 | +0.9% |
| cross-module-edit | 1 | 1 | +3.6% |
| rename-refactor | 1 | 0 | -24.4% |
| review | 1 | 0 | -8.8% |

### Per category: a vs c, mode on

| category | pairs | won | median saving |
|---|---|---|---|
| discovery | 1 | 1 | +10.6% |
| tracing | 1 | 1 | +10.8% |
| unfamiliar-module | 1 | 1 | +9.6% |
| debugging | 1 | 0 | -15.6% |
| single-file-edit | 1 | 1 | +0.7% |
| cross-module-edit | 1 | 1 | +20.9% |
| rename-refactor | 1 | 0 | -36.0% |
| review | 1 | 1 | +9.5% |

### Per category: a vs b, mode on

| category | pairs | won | median saving |
|---|---|---|---|
| discovery | 1 | 1 | +30.7% |
| tracing | 1 | 1 | +39.4% |
| unfamiliar-module | 1 | 1 | +18.9% |
| debugging | 1 | 1 | +9.1% |
| single-file-edit | 1 | 1 | +0.4% |
| cross-module-edit | 1 | 1 | +23.3% |
| rename-refactor | 1 | 0 | -26.6% |
| review | 1 | 1 | +4.6% |

## Per-category medians, mode off (oracle-passing valid runs)

| category | arm | runs | pass rate | median total tokens | median round trips | median tool-result tokens (chars/4) | median wall s |
|---|---|---|---|---|---|---|---|
| discovery | a | 4 | 1.00 | 127,248 | 5 | 1,758 | 131.6 |
| discovery | b | 5 | 1.00 | 80,788 | 2 | 467 | 162.2 |
| discovery | c | 5 | 1.00 | 100,138 | 2 | 414 | 147.5 |
| tracing | a | 5 | 1.00 | 159,019 | 7 | 2,151 | 110.4 |
| tracing | b | 5 | 1.00 | 84,384 | 2 | 517 | 150.5 |
| tracing | c | 5 | 0.80 | 133,244 | 4 | 1,196 | 106.8 |
| unfamiliar-module | a | 5 | 1.00 | 168,304 | 4 | 1,313 | 156.9 |
| unfamiliar-module | b | 5 | 1.00 | 142,449 | 3 | 749 | 116.2 |
| unfamiliar-module | c | 5 | 0.80 | 162,806 | 4 | 844 | 108.7 |
| debugging | a | 5 | 0.80 | 264,195 | 6 | 1,940 | 179.1 |
| debugging | b | 5 | 1.00 | 257,196 | 6 | 1,893 | 105.6 |
| debugging | c | 5 | 0.80 | 279,321 | 6 | 2,302 | 185.1 |
| single-file-edit | a | 5 | 0.80 | 219,252 | 4 | 1,138 | 76.0 |
| single-file-edit | b | 5 | 1.00 | 217,356 | 3 | 713 | 164.6 |
| single-file-edit | c | 5 | 0.80 | 242,414 | 4 | 1,482 | 112.6 |
| cross-module-edit | a | 5 | 0.80 | 282,510 | 7 | 2,484 | 74.1 |
| cross-module-edit | b | 5 | 1.00 | 272,260 | 5 | 1,326 | 136.6 |
| cross-module-edit | c | 5 | 0.80 | 291,312 | 5 | 1,656 | 146.9 |
| rename-refactor | a | 5 | 1.00 | 213,845 | 5 | 1,694 | 63.9 |
| rename-refactor | b | 5 | 0.80 | 265,985 | 5 | 1,508 | 132.0 |
| rename-refactor | c | 5 | 0.80 | 292,140 | 5 | 1,480 | 111.5 |
| review | a | 5 | 1.00 | 146,355 | 3 | 989 | 138.5 |
| review | b | 5 | 1.00 | 159,192 | 3 | 806 | 167.7 |
| review | c | 5 | 1.00 | 164,145 | 3 | 834 | 193.1 |

## Per-category medians, mode on (oracle-passing valid runs)

| category | arm | runs | pass rate | median total tokens | median round trips | median tool-result tokens (chars/4) | median wall s |
|---|---|---|---|---|---|---|---|
| discovery | a | 5 | 1.00 | 118,680 | 5 | 1,729 | 153.5 |
| discovery | b | 5 | 0.60 | 82,230 | 2 | 225 | 224.8 |
| discovery | c | 5 | 1.00 | 106,122 | 5 | 1,510 | 96.2 |
| tracing | a | 5 | 0.80 | 148,186 | 7 | 2,339 | 103.3 |
| tracing | b | 5 | 0.80 | 89,845 | 2 | 366 | 138.6 |
| tracing | c | 5 | 1.00 | 132,111 | 7 | 2,232 | 132.0 |
| unfamiliar-module | a | 5 | 0.60 | 200,380 | 4 | 1,383 | 95.0 |
| unfamiliar-module | b | 5 | 1.00 | 162,600 | 3 | 797 | 167.9 |
| unfamiliar-module | c | 5 | 0.60 | 181,140 | 4 | 1,568 | 33.0 |
| debugging | a | 5 | 1.00 | 283,800 | 6 | 2,131 | 50.1 |
| debugging | b | 5 | 1.00 | 257,940 | 6 | 1,861 | 167.5 |
| debugging | c | 5 | 0.80 | 328,143 | 6 | 1,601 | 136.1 |
| single-file-edit | a | 5 | 0.80 | 228,662 | 4 | 1,136 | 172.3 |
| single-file-edit | b | 5 | 1.00 | 227,859 | 3 | 664 | 199.2 |
| single-file-edit | c | 5 | 0.60 | 227,012 | 4 | 668 | 180.5 |
| cross-module-edit | a | 5 | 1.00 | 372,757 | 7 | 2,356 | 59.5 |
| cross-module-edit | b | 5 | 0.80 | 285,810 | 5 | 1,372 | 143.1 |
| cross-module-edit | c | 5 | 1.00 | 294,765 | 5 | 1,771 | 101.4 |
| rename-refactor | a | 5 | 1.00 | 200,050 | 5 | 1,527 | 90.2 |
| rename-refactor | b | 5 | 0.80 | 253,230 | 5 | 1,804 | 96.5 |
| rename-refactor | c | 5 | 1.00 | 272,045 | 5 | 1,753 | 184.8 |
| review | a | 5 | 1.00 | 152,907 | 3 | 699 | 109.8 |
| review | b | 5 | 1.00 | 145,833 | 3 | 1,042 | 212.3 |
| review | c | 5 | 1.00 | 138,375 | 3 | 946 | 143.2 |

## Hook and rule configuration per arm and mode

Every cell record carries its own configuration (`config` in cells.jsonl); this table summarises it.

| arm/mode | cells | client hooks installed | path-scoped rules present | cells where rule text was injected | median rule-sentinel hits | median price USD (assumed rates) |
|---|---|---|---|---|---|---|
| a/off | 40 | False | False | 0 | 0 | 0.1214 |
| b/off | 40 | False | False | 0 | 0 | 0.1301 |
| c/off | 40 | False | False | 0 | 0 | 0.1383 |
| a/on | 40 | False | True | 40 | 1 | 0.1374 |
| b/on | 40 | True | True | 0 | 0 | 0.1251 |
| c/on | 40 | True | True | 23 | 1 | 0.1390 |
