# Feature 020 V11 Refreeze Manifest

This machine-verifiable manifest binds the complete Feature 020 corpus, its V11 amendment mappings, the public Rust API contract, the lifecycle-prevention design, and the repository context to exact Git-object bytes. The manifest is internal evidence only; it does not grant implementation approval.

<!-- SYMFORGE FEATURE020 REFREEZE V11 JSON START -->
```json
{
  "amendment_set_id": "4e44bfef7dbf4aa4b7c67641c6e2bfb7323261036e0e67d68270ff0362b7c0db",
  "amendments": [
    {
      "amendment_id": "F020-V11-A01",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T053",
        "T063"
      ],
      "regression_ids": [
        "F020-V11-R01"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A01-BASE-01",
          "end_line": 481,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "add96f0719014ad803fec4ed1290e7ccb51c173fd770fd2c4600b109aa5b17da",
          "source": "baseline",
          "start_line": 478
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A01-TARGET-01",
          "end_line": 76,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "225bb1fdadba0658860dff519fdf17bbaae7491e1899b8f6f4504d50380b7a21",
          "source": "target",
          "start_line": 74
        }
      ],
      "requirement_ids": [
        "F020-V11-A01",
        "FR-009"
      ]
    },
    {
      "amendment_id": "F020-V11-A02",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T056"
      ],
      "regression_ids": [
        "F020-V11-R02"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A02-BASE-01",
          "end_line": 456,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "7f6c70d5f209d27d1acdf8d2b85a44cefb810d2ef495c7694f9150993cb674ad",
          "source": "baseline",
          "start_line": 451
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A02-TARGET-01",
          "end_line": 78,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "3e594b7c7a13224f5947c6fc5ee8ff69ca0f6246a29d945a16cb7c38a5c8fe62",
          "source": "target",
          "start_line": 77
        }
      ],
      "requirement_ids": [
        "F020-V11-A02",
        "FR-004"
      ]
    },
    {
      "amendment_id": "F020-V11-A03",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T041",
        "T063"
      ],
      "regression_ids": [
        "F020-V11-R03"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A03-BASE-01",
          "end_line": 521,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "1ad5eff5829aeed32bbe806cdf767380b0443174bc202a1e035043de17c4b3ed",
          "source": "baseline",
          "start_line": 517
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A03-TARGET-01",
          "end_line": 84,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "e779bf068e16ad704c5abf811faa99ba3d94ae07f893287f003d2c87f4e46e9e",
          "source": "target",
          "start_line": 79
        }
      ],
      "requirement_ids": [
        "F020-V11-A03",
        "FR-017"
      ]
    },
    {
      "amendment_id": "F020-V11-A04",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T053"
      ],
      "regression_ids": [
        "F020-V11-R04"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A04-BASE-01",
          "end_line": 108,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "5673635cccf503deb3bb244a88fe881d12e0e1d21a814c3b8d6ca01301d28f95",
          "source": "baseline",
          "start_line": 96
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A04-TARGET-01",
          "end_line": 87,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "27212099b051b38e4963265f3f21ca75ef7ec1d0c7256e4e377027de23b2cf32",
          "source": "target",
          "start_line": 85
        }
      ],
      "requirement_ids": [
        "F020-V11-A04",
        "FR-003",
        "FR-004",
        "FR-011"
      ]
    },
    {
      "amendment_id": "F020-V11-A05",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T053"
      ],
      "regression_ids": [
        "F020-V11-R05"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A05-BASE-01",
          "end_line": 550,
          "path": "specs/020-repository-knowledge-index/data-model.md",
          "sha256": "bbd111de9f9f03dcc2afdf9021b0154b99c7ac6beb3445b290c700db0765bac2",
          "source": "baseline",
          "start_line": 546
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A05-TARGET-01",
          "end_line": 90,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "201dad54a677d302132dffc5c45621d8d5ada377bd94d798c9aedb4517f90a7c",
          "source": "target",
          "start_line": 88
        }
      ],
      "requirement_ids": [
        "F020-V11-A05",
        "FR-039"
      ]
    },
    {
      "amendment_id": "F020-V11-A06",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T053",
        "T059"
      ],
      "regression_ids": [
        "F020-V11-R06"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A06-BASE-01",
          "end_line": 468,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "32086debd06470c586de387d5c70133ce1644c095121fab370cbbceb55260109",
          "source": "baseline",
          "start_line": 464
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A06-TARGET-01",
          "end_line": 92,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "9b2cb975edbdbdc493893e18815e8fbb54d08d98e305c085fa145a00ae2ef225",
          "source": "target",
          "start_line": 91
        }
      ],
      "requirement_ids": [
        "F020-V11-A06",
        "FR-007"
      ]
    },
    {
      "amendment_id": "F020-V11-A07",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T053",
        "T063"
      ],
      "regression_ids": [
        "F020-V11-R07"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A07-BASE-01",
          "end_line": 764,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "28c456a9f5ac6bb5f0044790d12bbb74256044254af9d613d84f7effc6a9908f",
          "source": "baseline",
          "start_line": 763
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A07-TARGET-01",
          "end_line": 95,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "36d54d0ec75d84b08688264c5af3f3a0b8dbe3cd9a026283a41e91408a40aa6a",
          "source": "target",
          "start_line": 93
        }
      ],
      "requirement_ids": [
        "F020-V11-A07",
        "FR-007"
      ]
    },
    {
      "amendment_id": "F020-V11-A08",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T056",
        "T059",
        "T063"
      ],
      "regression_ids": [
        "F020-V11-R08"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A08-BASE-01",
          "end_line": 535,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "ff13aa9b19399b384bf07fced5d2b5139904c126bfb83a6d1d8e974b0ba05278",
          "source": "baseline",
          "start_line": 534
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A08-TARGET-01",
          "end_line": 98,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "e5002b7a81f66d285cc67136f3a27c2422e5f99361e27d54c9a703b5819341cf",
          "source": "target",
          "start_line": 96
        }
      ],
      "requirement_ids": [
        "F020-V11-A08",
        "FR-021"
      ]
    },
    {
      "amendment_id": "F020-V11-A09",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T053"
      ],
      "regression_ids": [
        "F020-V11-R09"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A09-BASE-01",
          "end_line": 537,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "8027205fa230da330f63d3656ab01f6df50b47ee35b691ef7028ca75dc1f8605",
          "source": "baseline",
          "start_line": 536
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A09-TARGET-01",
          "end_line": 101,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "ff86741b950f079fc4a7538fd3af3ef215e6701ae2c2387f3f530fb0cc023310",
          "source": "target",
          "start_line": 99
        }
      ],
      "requirement_ids": [
        "F020-V11-A09",
        "FR-022"
      ]
    },
    {
      "amendment_id": "F020-V11-A10",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T053",
        "T060",
        "T064"
      ],
      "regression_ids": [
        "F020-V11-R10"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A10-BASE-01",
          "end_line": 574,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "ca4e0b79b3217bf0fe6e897e5ad5e9586337f605df5db781320df7023cffad9d",
          "source": "baseline",
          "start_line": 564
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A10-TARGET-01",
          "end_line": 104,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "a65834f78006ec070d2070cfdd62dd55904972bc6f1489a28ab0b12201f381e8",
          "source": "target",
          "start_line": 102
        }
      ],
      "requirement_ids": [
        "F020-V11-A10",
        "FR-031",
        "FR-039"
      ]
    },
    {
      "amendment_id": "F020-V11-A11",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T041",
        "T053",
        "T056"
      ],
      "regression_ids": [
        "F020-V11-R11"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A11-BASE-01",
          "end_line": 641,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "cb7ea21f9d3376853d5736ebf56bf65a9ac32c255d1cadee18d2b51b886cfa91",
          "source": "baseline",
          "start_line": 635
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A11-TARGET-01",
          "end_line": 108,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "78a3ffe9be8cfebed8e5a2d6d949ed5de1827845d261a795dfe64d31e8fc2b16",
          "source": "target",
          "start_line": 105
        }
      ],
      "requirement_ids": [
        "F020-V11-A11",
        "FR-039"
      ]
    },
    {
      "amendment_id": "F020-V11-A12",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T030",
        "T056"
      ],
      "regression_ids": [
        "F020-V11-R12"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A12-BASE-01",
          "end_line": 843,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "6b904e46ee976eb2f5df668dd96e2473460873227eb0ecabbf512b1728e23b3a",
          "source": "baseline",
          "start_line": 837
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A12-TARGET-01",
          "end_line": 111,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "fc8f549b5ba4b40b6b26ef9295372536f7278935c837129de5414652f25e10b7",
          "source": "target",
          "start_line": 109
        }
      ],
      "requirement_ids": [
        "F020-V11-A12",
        "SC-019"
      ]
    },
    {
      "amendment_id": "F020-V11-A13",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/search-knowledge.md#v11-lifecycle-acquisition"
      ],
      "plan_task_ids": [
        "T003",
        "T056",
        "T063"
      ],
      "regression_ids": [
        "F020-V11-R13"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A13-BASE-01",
          "end_line": 188,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "c129b963ed6079a6d1252964465f4d6a996c063ed9d0963a49d9162f7c8e4c88",
          "source": "baseline",
          "start_line": 187
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A13-TARGET-01",
          "end_line": 114,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "b0f8b282442103fb2addeaa298b7629cabd750cd3f3ad122f018050a40654d97",
          "source": "target",
          "start_line": 112
        }
      ],
      "requirement_ids": [
        "F020-V11-A13",
        "FR-017"
      ]
    },
    {
      "amendment_id": "F020-V11-A14",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/repository-mental-model.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T056",
        "T063"
      ],
      "regression_ids": [
        "F020-V11-R14"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A14-BASE-01",
          "end_line": 805,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "092350911f3b97b176f4ed522584a24bb585d014cd823c9605ea36620704ca9d",
          "source": "baseline",
          "start_line": 803
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A14-TARGET-01",
          "end_line": 117,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "81b73cb828758e5c137761bb28c941b73ee0d90538dd691a3de56ca4892d0597",
          "source": "target",
          "start_line": 115
        }
      ],
      "requirement_ids": [
        "F020-V11-A14",
        "SC-011"
      ]
    },
    {
      "amendment_id": "F020-V11-A15",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T056",
        "T063"
      ],
      "regression_ids": [
        "F020-V11-R15"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A15-BASE-01",
          "end_line": 783,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "e239c00fd40a6b7c7b8e3537e8f0ac732cb16e9fe3fd73b9ee27180c1fc15d2f",
          "source": "baseline",
          "start_line": 782
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A15-TARGET-01",
          "end_line": 120,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "38c46bbba96c479413c7a44af35d1ea1e5f63f168129cde3e2ede8773a00b39f",
          "source": "target",
          "start_line": 118
        }
      ],
      "requirement_ids": [
        "F020-V11-A15",
        "SC-002"
      ]
    },
    {
      "amendment_id": "F020-V11-A16",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T056",
        "T059",
        "T063"
      ],
      "regression_ids": [
        "F020-V11-R16"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A16-BASE-01",
          "end_line": 665,
          "path": "specs/020-repository-knowledge-index/data-model.md",
          "sha256": "70430131566266acc34eb1a2a098c70ec25fc352d4c092ac79fca1f73db6fbfa",
          "source": "baseline",
          "start_line": 663
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A16-TARGET-01",
          "end_line": 123,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "e91436304e1dfde7643f80f141443371b808dad60a45942657ab88b26077edb0",
          "source": "target",
          "start_line": 121
        }
      ],
      "requirement_ids": [
        "F020-V11-A16",
        "FR-021"
      ]
    },
    {
      "amendment_id": "F020-V11-A17",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T016",
        "T064"
      ],
      "regression_ids": [
        "F020-V11-R17"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A17-BASE-01",
          "end_line": 713,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "33614faa2825a2611fb1ebe096a1308aea3656d23122f722cc2910a813d263c1",
          "source": "baseline",
          "start_line": 710
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A17-TARGET-01",
          "end_line": 126,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "72c962f5da1494f5671720012482f337c44b74dc3bf54b9f2209e811e3a98f7e",
          "source": "target",
          "start_line": 124
        }
      ],
      "requirement_ids": [
        "F020-V11-A17",
        "FR-049"
      ]
    },
    {
      "amendment_id": "F020-V11-A18",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/lifecycle-oracle-traceability-v11.md#lifecycle-oracle-traceability-contract-v11"
      ],
      "plan_task_ids": [
        "T003",
        "T068",
        "T069",
        "T070"
      ],
      "regression_ids": [
        "F020-V11-R18A",
        "F020-V11-R18B",
        "F020-V11-R18C"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A18-BASE-01",
          "end_line": 768,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "67a859233d0be42c956dcb84cebd8cc0215402a6301c0272d9cd2f4940107d1b",
          "source": "baseline",
          "start_line": 767
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A18-TARGET-01",
          "end_line": 129,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "7be95c65b0ec32bf7adda1c39e3cf0b0545f493f3eedb6c26e60368aeb6d8aa0",
          "source": "target",
          "start_line": 127
        }
      ],
      "requirement_ids": [
        "F020-V11-A18",
        "SC-024"
      ]
    },
    {
      "amendment_id": "F020-V11-A19",
      "contract_clause_ids": [
        "contracts/knowledge-authority-hygiene.md#v11-lifecycle-acquisition-and-voice-filtering",
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/repository-mental-model.md#v11-lifecycle-amendment",
        "contracts/search-knowledge.md#v11-lifecycle-acquisition",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T041",
        "T063",
        "T084"
      ],
      "regression_ids": [
        "F020-V11-R19A",
        "F020-V11-R19B"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A19-BASE-01",
          "end_line": 100,
          "path": "specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md",
          "sha256": "bbeffdee5dd1ae02cf6025406f9322f79a7c4dada35ab2b9c585ed8adb9582e6",
          "source": "baseline",
          "start_line": 84
        },
        {
          "clause_id": "F020-V11-A19-BASE-02",
          "end_line": 159,
          "path": "specs/020-repository-knowledge-index/contracts/repository-mental-model.md",
          "sha256": "9a162f3b4e70bdb0d2c4c7b95b573737bbbffb55033f9e31e082e42086aff3a7",
          "source": "baseline",
          "start_line": 154
        },
        {
          "clause_id": "F020-V11-A19-BASE-03",
          "end_line": 53,
          "path": "specs/020-repository-knowledge-index/contracts/search-knowledge.md",
          "sha256": "7389365b8fdf93e6d8011648ad466aaa7ddf5a75335c76fd25b0f3fb1ab319d5",
          "source": "baseline",
          "start_line": 49
        },
        {
          "clause_id": "F020-V11-A19-BASE-04",
          "end_line": 138,
          "path": "specs/020-repository-knowledge-index/contracts/search-knowledge.md",
          "sha256": "389aa16bfa073ad0431c9321d99257869ce32a4c5515d647a982caab8fec0a3a",
          "source": "baseline",
          "start_line": 135
        },
        {
          "clause_id": "F020-V11-A19-BASE-05",
          "end_line": 177,
          "path": "specs/020-repository-knowledge-index/contracts/source-binding-and-state.md",
          "sha256": "9f3e5a696b25046d6126ee5d30adfdcb906f9ed5de1def1952a73e90d713bc79",
          "source": "baseline",
          "start_line": 176
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A19-TARGET-01",
          "end_line": 1179,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "a96f61287d65eff1f9efb409b82640dced1fbef1051d0c93af7122f4f84c4be5",
          "source": "target",
          "start_line": 130
        },
        {
          "clause_id": "F020-V11-A19-TARGET-02",
          "end_line": 48,
          "path": "specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md",
          "sha256": "d89683e42bd1d320de0af41cad70032108c7f3897255f31c8a9d5b43f6b63fd2",
          "source": "target",
          "start_line": 27
        },
        {
          "clause_id": "F020-V11-A19-TARGET-03",
          "end_line": 31,
          "path": "specs/020-repository-knowledge-index/contracts/repository-mental-model.md",
          "sha256": "72215d1bac7c0f1f2d879647a0b1114548a141a78c84f8e68e91596848797485",
          "source": "target",
          "start_line": 18
        },
        {
          "clause_id": "F020-V11-A19-TARGET-04",
          "end_line": 77,
          "path": "specs/020-repository-knowledge-index/contracts/search-knowledge.md",
          "sha256": "8a9ff3aa5f10d2f28e46b024c71b6a81b9fa16d73fa28df237dafc74e05f4494",
          "source": "target",
          "start_line": 57
        },
        {
          "clause_id": "F020-V11-A19-TARGET-05",
          "end_line": 52,
          "path": "specs/020-repository-knowledge-index/contracts/source-binding-and-state.md",
          "sha256": "a902a5be7bdb75accb15ff84dcd3beb3ddbd1e6be3753bf49c396774a673f110",
          "source": "target",
          "start_line": 18
        }
      ],
      "requirement_ids": [
        "F020-V11-A19",
        "FR-017",
        "FR-033"
      ]
    },
    {
      "amendment_id": "F020-V11-A20",
      "contract_clause_ids": [
        "contracts/lifecycle-acceptance-oracles-v11.md#lifecycle-acceptance-oracles-v11",
        "contracts/source-binding-and-state.md#v11-lifecycle-amendment"
      ],
      "plan_task_ids": [
        "T003",
        "T024",
        "T027",
        "T060"
      ],
      "regression_ids": [
        "F020-V11-R20A",
        "F020-V11-R20B"
      ],
      "replaced": [
        {
          "clause_id": "F020-V11-A20-BASE-01",
          "end_line": 222,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "4fe19262e992a9fbeb475f68a1c0ae14740bf1b88e41fe45085673f749778c91",
          "source": "baseline",
          "start_line": 221
        }
      ],
      "replacements": [
        {
          "clause_id": "F020-V11-A20-TARGET-01",
          "end_line": 17,
          "path": "specs/020-repository-knowledge-index/spec.md",
          "sha256": "c756f555a189b3b236541f584601d650eea1afaabf8c3af9401235a4b2e67572",
          "source": "target",
          "start_line": 14
        },
        {
          "clause_id": "F020-V11-A20-TARGET-02",
          "end_line": 1549,
          "path": "specs/020-repository-knowledge-index/data-model.md",
          "sha256": "18efa7bd4880800fb64c227a1d0a6f5a5b9dde6d0c5adfc15bbef8853f85a964",
          "source": "target",
          "start_line": 1539
        }
      ],
      "requirement_ids": [
        "F020-V11-A20",
        "FR-037",
        "FR-043",
        "FR-051"
      ]
    }
  ],
  "baseline": {
    "commit": "1521abb0197dac16e046a2b0b20a66a70c3a909b",
    "tree": "c26043df97571dd079681291d2621a4e06438d8d"
  },
  "context": {
    "path": "CONTEXT.md",
    "sha256": "ea7fca771e080b20ae38c0fd15db97fafe111d536e59c0eff31c062e6762fb26"
  },
  "design": {
    "path": "docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md",
    "sha256": "9b0a1b79b20bc70197a438409e8484e74319888ee5ded5bba39452b6b301bf5b"
  },
  "detached_attestation_path": "docs/reviews/FEATURE-020-REFREEZE-ATTESTATION-v11.md",
  "feature_root": "specs/020-repository-knowledge-index",
  "inventory": [
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "CONTEXT.md",
      "scope": "bound",
      "sha256": "ea7fca771e080b20ae38c0fd15db97fafe111d536e59c0eff31c062e6762fb26",
      "superseded_by": []
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/CURSOR-REVIEW-PROMPT-2.md",
      "scope": "feature",
      "sha256": "5153c0b37ce7b7c451bb4207a1287f5a063bd555d6f98ff582f39c104bed369c",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "supporting_evidence",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/CURSOR-REVIEW-PROMPT.md",
      "scope": "feature",
      "sha256": "85641252c20186aa4cb5bfe120ea67a24b4969b6d8076858c24286d58ebac4e4",
      "superseded_by": []
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/GATE-L-REVIEW.md",
      "scope": "feature",
      "sha256": "a08e3c340694458e1eedb3b753d245868b8d205f038b45524ac07d5133a272fd",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/GATE-M-REVIEW.md",
      "scope": "feature",
      "sha256": "231cb19a9183c85c964b9535d27210ff7b80dd7d06455b6d4047fb6b2ce0611d",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/GOAL.md",
      "scope": "feature",
      "sha256": "0a624397a76b28d7cc545d60f765d205366ffaf6d00585fad665f4ccaa8cf382",
      "superseded_by": []
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/HANDOVER-2026-07-16.md",
      "scope": "feature",
      "sha256": "5f564385706f55d5316acfb913be7f4336bb6b464d667407d8f167fc2564f4a4",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "historical",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/HANDOVER-2026-07-22.md",
      "scope": "feature",
      "sha256": "79bbec1ddd653ce289126d3957c409f219715f8a24a1eac8eb092851b311539f",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "self_excluded",
      "path": "specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md",
      "scope": "feature",
      "sha256": null,
      "superseded_by": []
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/adversarial-review-2026-07-16.md",
      "scope": "feature",
      "sha256": "9d4dd7e1efc2e95e3523b7b22952d90ac4da17a3f39d204930e200826921c94e",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "supporting_evidence",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/advice-request-fable-knowledge-authority.md",
      "scope": "feature",
      "sha256": "0df6052d0db51e0481a0a541b62350058285bb5dd7645f97b96439745fead24d",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/checklists/requirements.md",
      "scope": "feature",
      "sha256": "8eb291c9237f1d31f2ab96c33984b07713944ce2453d1acfb2770cc180201b1b",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md",
      "scope": "feature",
      "sha256": "e0a62835817617be8f7511e0e992c92d4fdc8d9b920443ca4859ff460fdcf208",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/contracts/lifecycle-acceptance-oracles-v11.md",
      "scope": "feature",
      "sha256": "dec6debd34ea49490385016a154186f04bad304222665e36eb722be3a213de2c",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/contracts/lifecycle-oracle-traceability-v11.md",
      "scope": "feature",
      "sha256": "32021e8d8ec441dbedef42c797187e0fa16b3ed9e77b6b3a83bbca10cd3d43a3",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/contracts/public-api-v11.json",
      "scope": "feature",
      "sha256": "5e5b47b110b27f57f5cb83506130131be772047b03a0f4cacc3412e60718f5a9",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/contracts/repository-mental-model.md",
      "scope": "feature",
      "sha256": "dac8e1746fda84c5e609b85b9d8d28a21cf486d9a676a461d8618430584254ed",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/contracts/search-knowledge.md",
      "scope": "feature",
      "sha256": "64140285df26b6e5cf9b267eab1858d14980410ab5f63f43c8905fab18a31387",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/contracts/source-binding-and-state.md",
      "scope": "feature",
      "sha256": "c35d610d09476b1b6edc8a146406ceeadcda77e1959432bba7798e604f8c1a54",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md",
      "scope": "feature",
      "sha256": "709ac695457804dd21647f4899f48710e80e90d84da93784e4f5cdb61e9793e9",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/data-model.md",
      "scope": "feature",
      "sha256": "634612ecbd64797c9e4934670ea803cd7561c1fea70bd4d0e882b02a29aeb2fa",
      "superseded_by": []
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/fable-adversarial-review-2026-07-17.md",
      "scope": "feature",
      "sha256": "bcceb2fda689a54dd6507f43632f5440b16fd0854d3052ef6e60854f163edea9",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/fable-focused-rereview-2026-07-17.md",
      "scope": "feature",
      "sha256": "85643e7fb1c448383d1a0f49ecda368a2b4f6bac82c707e64776076f35eee22c",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/fable-focused-rereview-request-2026-07-17.md",
      "scope": "feature",
      "sha256": "d4fbb71909132dc2e0d5408594b6110e17fe9878ee4a77ba924ed370711a1b43",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/fable-gate-d-review-2026-07-21.md",
      "scope": "feature",
      "sha256": "0030a31a337b25ff5f1ee642d9b9ba7d8b239b1d375353ff971c38c83824199f",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/fable-gate-d-review-request-2026-07-21.md",
      "scope": "feature",
      "sha256": "5365f6c9d792a305725a120ba2dd619b9c34145d48dea382c648214a8673c5fb",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "historical",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/fable-scoped-delta-verification-2026-07-17.md",
      "scope": "feature",
      "sha256": "7356cedd9e02495b3844ac8fe4e8ec817cc3dccd2deba7b01b190c24c2f5b5e9",
      "superseded_by": []
    },
    {
      "classification": "supporting_evidence",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/fable-scoped-delta-verification-request-2026-07-17.md",
      "scope": "feature",
      "sha256": "a1bba3361c78e484eb2437934513830a1854bf107c9d3c3617a69e4b5d8fdebb",
      "superseded_by": []
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/gate-l-review-kimi-2026-07-24.md",
      "scope": "feature",
      "sha256": "bb10979e3cea9f8e1890bf686af701d8f9d80c681717f762eee819b1e9f93848",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/gate-m-review-cursor-2026-07-24.md",
      "scope": "feature",
      "sha256": "5bee4a0a1b7beb2c6359ac8b6ab6932efe778699b6d2f11e35e57b6c81deef8d",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/gate-m-review-kimi-k3-2026-07-24.md",
      "scope": "feature",
      "sha256": "eb1c8816ead5ef6516d2e72834d728987f999043549212da441437342f23f430",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/plan.md",
      "scope": "feature",
      "sha256": "8c1027033b5c6a66c327f75fd8d8b5091921cfa619a43e2f4f53dc486cea5a9a",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/quickstart.md",
      "scope": "feature",
      "sha256": "e19fe87148bb354d0307ba17a4c345ece575f8f365b6895de997f3e5fd8c5991",
      "superseded_by": []
    },
    {
      "classification": "historical",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/research.md",
      "scope": "feature",
      "sha256": "845e3f69d66dd0e094eedb54b1043875b657d09be9100655bf0279955f9bd4a7",
      "superseded_by": []
    },
    {
      "classification": "superseded",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/review-request-fable.md",
      "scope": "feature",
      "sha256": "6fd0ab2df6cb714aec9700bc4bc8c1d81a1687aaef71b6469d909474b26d8706",
      "superseded_by": [
        "F020-V11-A01",
        "F020-V11-A02",
        "F020-V11-A03",
        "F020-V11-A04",
        "F020-V11-A05",
        "F020-V11-A06",
        "F020-V11-A07",
        "F020-V11-A08",
        "F020-V11-A09",
        "F020-V11-A10",
        "F020-V11-A11",
        "F020-V11-A12",
        "F020-V11-A13",
        "F020-V11-A14",
        "F020-V11-A15",
        "F020-V11-A16",
        "F020-V11-A17",
        "F020-V11-A18",
        "F020-V11-A19"
      ]
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/spec.md",
      "scope": "feature",
      "sha256": "44141690ff74b9be2eb8bdf4862c42eb7c856b79cb5682a4f234a89823ab533a",
      "superseded_by": []
    },
    {
      "classification": "normative",
      "hash_policy": "raw_bytes",
      "path": "specs/020-repository-knowledge-index/tasks.md",
      "scope": "feature",
      "sha256": "2d67dfe7cd4a0ae7b48da5253ebc440679dd7c4954d5349b3edbd259950b7518",
      "superseded_by": []
    }
  ],
  "kind": "symforge-feature-020-refreeze",
  "public_api": {
    "canonical_sha256": "c45f3cd3f77e5690ad1dcd2e5fc7e39e30d52df38fa564d7b663e1c95823a7da",
    "path": "specs/020-repository-knowledge-index/contracts/public-api-v11.json",
    "raw_sha256": "5e5b47b110b27f57f5cb83506130131be772047b03a0f4cacc3412e60718f5a9"
  },
  "required_normative_paths": [
    "CONTEXT.md",
    "specs/020-repository-knowledge-index/GOAL.md",
    "specs/020-repository-knowledge-index/checklists/requirements.md",
    "specs/020-repository-knowledge-index/contracts/knowledge-authority-hygiene.md",
    "specs/020-repository-knowledge-index/contracts/lifecycle-acceptance-oracles-v11.md",
    "specs/020-repository-knowledge-index/contracts/lifecycle-oracle-traceability-v11.md",
    "specs/020-repository-knowledge-index/contracts/public-api-v11.json",
    "specs/020-repository-knowledge-index/contracts/repository-mental-model.md",
    "specs/020-repository-knowledge-index/contracts/search-knowledge.md",
    "specs/020-repository-knowledge-index/contracts/source-binding-and-state.md",
    "specs/020-repository-knowledge-index/contracts/v10-authority-retirement-v11.md",
    "specs/020-repository-knowledge-index/data-model.md",
    "specs/020-repository-knowledge-index/plan.md",
    "specs/020-repository-knowledge-index/quickstart.md",
    "specs/020-repository-knowledge-index/spec.md",
    "specs/020-repository-knowledge-index/tasks.md"
  ],
  "schema_version": 1,
  "self_path": "specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md"
}
```
<!-- SYMFORGE FEATURE020 REFREEZE V11 JSON END -->
