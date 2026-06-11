use std::collections::HashMap;

use crate::domain::{SymbolKind, SymbolRecord};

use super::{MAX_ARRAY_ITEMS, byte_to_line, join_array_index, join_key_path};

const FACT_PREFIX: &str = "ci.workflow";
const MAX_FACT_VALUE_CHARS: usize = 160;

pub(super) fn append_ci_yaml_facts(
    content: &[u8],
    line_starts: &[u32],
    root: &serde_yml::Value,
    symbols: &mut Vec<SymbolRecord>,
    sort_order: &mut u32,
) {
    let serde_yml::Value::Mapping(root_map) = root else {
        return;
    };
    if !is_github_actions_workflow(root_map) {
        return;
    }

    let spans = symbols
        .iter()
        .map(|symbol| (symbol.name.clone(), symbol.clone()))
        .collect::<HashMap<_, _>>();
    let mut builder = FactBuilder {
        content,
        line_starts,
        spans: &spans,
        symbols,
        sort_order,
    };

    if let Some(name) = mapping_get(root_map, "name").and_then(scalar_fact_value) {
        builder.push_from_path("name", format!("{FACT_PREFIX}.name={name}"));
    }

    if let Some(on_value) = mapping_get(root_map, "on") {
        append_trigger_facts(&mut builder, on_value);
    }
    if let Some(permissions) = mapping_get(root_map, "permissions").and_then(as_mapping) {
        append_scalar_map_facts(
            &mut builder,
            permissions,
            "permissions",
            &format!("{FACT_PREFIX}.permission"),
        );
    }
    if let Some(env) = mapping_get(root_map, "env").and_then(as_mapping) {
        append_scalar_map_facts(&mut builder, env, "env", &format!("{FACT_PREFIX}.env"));
    }
    if let Some(jobs) = mapping_get(root_map, "jobs").and_then(as_mapping) {
        append_job_facts(&mut builder, jobs);
    }
}

fn append_trigger_facts(builder: &mut FactBuilder<'_>, on_value: &serde_yml::Value) {
    match on_value {
        serde_yml::Value::Mapping(triggers) => {
            for (trigger, _value) in string_entries(triggers).into_iter().take(MAX_ARRAY_ITEMS) {
                let source_path = join_key_path("on", &trigger);
                builder.push_from_path(
                    &source_path,
                    format!("{FACT_PREFIX}.trigger.{}", fact_segment(&trigger)),
                );
            }
        }
        serde_yml::Value::Sequence(triggers) => {
            for (index, trigger) in triggers.iter().take(MAX_ARRAY_ITEMS).enumerate() {
                let Some(trigger) = scalar_fact_value(trigger) else {
                    continue;
                };
                let source_path = join_array_index("on", index);
                builder.push_from_path(
                    &source_path,
                    format!("{FACT_PREFIX}.trigger.{}", fact_segment(&trigger)),
                );
            }
        }
        _ => {
            if let Some(trigger) = scalar_fact_value(on_value) {
                builder.push_from_path(
                    "on",
                    format!("{FACT_PREFIX}.trigger.{}", fact_segment(&trigger)),
                );
            }
        }
    }
}

fn append_scalar_map_facts(
    builder: &mut FactBuilder<'_>,
    map: &serde_yml::Mapping,
    source_parent: &str,
    fact_parent: &str,
) {
    for (key, value) in string_entries(map).into_iter().take(MAX_ARRAY_ITEMS) {
        let Some(value) = scalar_fact_value(value) else {
            continue;
        };
        let source_path = join_key_path(source_parent, &key);
        builder.push_from_path(
            &source_path,
            format!("{fact_parent}.{}={value}", fact_segment(&key)),
        );
    }
}

fn append_job_facts(builder: &mut FactBuilder<'_>, jobs: &serde_yml::Mapping) {
    for (job_name, job_value) in string_entries(jobs).into_iter().take(MAX_ARRAY_ITEMS) {
        let Some(job) = as_mapping(job_value) else {
            continue;
        };
        let job_path = join_key_path("jobs", &job_name);
        let job_fact = format!("{FACT_PREFIX}.job.{}", fact_segment(&job_name));
        builder.push_from_path(&job_path, job_fact.clone());

        append_job_scalar_or_sequence_fact(
            builder,
            job,
            &job_path,
            &job_fact,
            "needs",
            "needs",
        );
        append_job_scalar_or_sequence_fact(
            builder,
            job,
            &job_path,
            &job_fact,
            "runs-on",
            "runs-on",
        );
        append_strategy_facts(builder, job, &job_path, &job_fact);
        append_step_facts(builder, job, &job_path, &job_fact);
    }
}

fn append_job_scalar_or_sequence_fact(
    builder: &mut FactBuilder<'_>,
    job: &serde_yml::Mapping,
    job_path: &str,
    job_fact: &str,
    key: &str,
    fact_key: &str,
) {
    let Some(value) = mapping_get(job, key) else {
        return;
    };
    let source_path = join_key_path(job_path, key);
    match value {
        serde_yml::Value::Sequence(values) => {
            for (index, value) in values.iter().take(MAX_ARRAY_ITEMS).enumerate() {
                let Some(value) = scalar_fact_value(value) else {
                    continue;
                };
                let item_path = join_array_index(&source_path, index);
                builder.push_from_path(&item_path, format!("{job_fact}.{fact_key}[{index}]={value}"));
            }
        }
        _ => {
            if let Some(value) = scalar_fact_value(value) {
                builder.push_from_child_key(
                    job_path,
                    key,
                    format!("{job_fact}.{fact_key}={value}"),
                );
            }
        }
    }
}

fn append_strategy_facts(
    builder: &mut FactBuilder<'_>,
    job: &serde_yml::Mapping,
    job_path: &str,
    job_fact: &str,
) {
    let Some(strategy) = mapping_get(job, "strategy").and_then(as_mapping) else {
        return;
    };
    let strategy_path = join_key_path(job_path, "strategy");

    if let Some(fail_fast) = mapping_get(strategy, "fail-fast").and_then(scalar_fact_value) {
        builder.push_from_child_key(
            &strategy_path,
            "fail-fast",
            format!("{job_fact}.strategy.fail-fast={fail_fast}"),
        );
    }

    let Some(matrix) = mapping_get(strategy, "matrix").and_then(as_mapping) else {
        return;
    };
    let matrix_path = join_key_path(&strategy_path, "matrix");
    let Some(include) = mapping_get(matrix, "include").and_then(as_sequence) else {
        return;
    };
    let include_path = join_key_path(&matrix_path, "include");

    for (index, item) in include.iter().take(MAX_ARRAY_ITEMS).enumerate() {
        let Some(item_map) = as_mapping(item) else {
            continue;
        };
        let item_path = join_array_index(&include_path, index);
        for key in ["os", "rust", "target", "artifact"] {
            let Some(value) = mapping_get(item_map, key).and_then(scalar_fact_value) else {
                continue;
            };
            builder.push_from_child_key(
                &item_path,
                key,
                format!(
                    "{job_fact}.strategy.matrix.include[{index}].{}={value}",
                    fact_segment(key)
                ),
            );
        }
    }
}

fn append_step_facts(
    builder: &mut FactBuilder<'_>,
    job: &serde_yml::Mapping,
    job_path: &str,
    job_fact: &str,
) {
    let Some(steps) = mapping_get(job, "steps").and_then(as_sequence) else {
        return;
    };
    let steps_path = join_key_path(job_path, "steps");

    for (index, step) in steps.iter().take(MAX_ARRAY_ITEMS).enumerate() {
        let Some(step_map) = as_mapping(step) else {
            continue;
        };
        let step_path = join_array_index(&steps_path, index);
        for key in ["name", "uses", "run", "working-directory"] {
            let Some(value) = mapping_get(step_map, key).and_then(scalar_fact_value) else {
                continue;
            };
            builder.push_from_child_key(
                &step_path,
                key,
                format!("{job_fact}.step[{index}].{}={value}", fact_segment(key)),
            );
        }
    }
}

struct FactBuilder<'a> {
    content: &'a [u8],
    line_starts: &'a [u32],
    spans: &'a HashMap<String, SymbolRecord>,
    symbols: &'a mut Vec<SymbolRecord>,
    sort_order: &'a mut u32,
}

impl FactBuilder<'_> {
    fn push_from_path(&mut self, source_path: &str, fact_name: String) {
        let Some(source) = self.spans.get(source_path) else {
            return;
        };
        self.push_with_range(fact_name, source.byte_range, source.item_byte_range);
    }

    fn push_from_child_key(&mut self, parent_path: &str, child_key: &str, fact_name: String) {
        let Some(parent) = self.spans.get(parent_path) else {
            return;
        };
        let byte_range = find_child_key_line_range(self.content, parent.byte_range, child_key)
            .unwrap_or(parent.byte_range);
        self.push_with_range(fact_name, byte_range, Some(byte_range));
    }

    fn push_with_range(
        &mut self,
        fact_name: String,
        byte_range: (u32, u32),
        item_byte_range: Option<(u32, u32)>,
    ) {
        let start_line = byte_to_line(self.line_starts, byte_range.0);
        let end_line = byte_to_line(
            self.line_starts,
            byte_range.1.saturating_sub(1).max(byte_range.0),
        );
        self.symbols.push(SymbolRecord {
            name: fact_name,
            kind: SymbolKind::Key,
            depth: 0,
            sort_order: *self.sort_order,
            byte_range,
            item_byte_range,
            line_range: (start_line, end_line),
            doc_byte_range: None,
        });
        *self.sort_order += 1;
    }
}

fn find_child_key_line_range(
    content: &[u8],
    parent_range: (u32, u32),
    child_key: &str,
) -> Option<(u32, u32)> {
    let start = parent_range.0 as usize;
    let end = (parent_range.1 as usize).min(content.len());
    let needle = format!("{child_key}:");

    let mut cursor = start;
    while cursor < end {
        let line_start = cursor;
        let line_end = content[cursor..end]
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(end, |offset| cursor + offset + 1);
        cursor = line_end;

        let line = &content[line_start..line_end];
        let mut key_start = line
            .iter()
            .take_while(|&&byte| byte == b' ' || byte == b'\t')
            .count();
        let mut rest = &line[key_start..];
        if rest.starts_with(b"-") {
            rest = &rest[1..];
            key_start += 1;
            let whitespace = rest
                .iter()
                .take_while(|&&byte| byte == b' ' || byte == b'\t')
                .count();
            rest = &rest[whitespace..];
            key_start += whitespace;
        }

        if rest.starts_with(needle.as_bytes()) {
            let value_end = line
                .iter()
                .rposition(|&byte| byte != b'\n' && byte != b'\r')
                .map_or(line_start + key_start, |offset| line_start + offset + 1);
            return Some(((line_start + key_start) as u32, value_end as u32));
        }
    }

    None
}

fn is_github_actions_workflow(map: &serde_yml::Mapping) -> bool {
    mapping_get(map, "jobs").is_some_and(|value| matches!(value, serde_yml::Value::Mapping(_)))
        && mapping_get(map, "on").is_some()
}

fn mapping_get<'a>(map: &'a serde_yml::Mapping, key: &str) -> Option<&'a serde_yml::Value> {
    // serde_yaml_ng's `Mapping::get` accepts a `&str` index and matches scalar
    // string keys, so the native string-keyed lookup is exactly what we want.
    map.get(key)
}

fn string_entries(map: &serde_yml::Mapping) -> Vec<(String, &serde_yml::Value)> {
    // serde_yaml_ng keys mappings by `Value`; keep only scalar string keys,
    // which is what every CI-fact lookup here expects.
    map.iter()
        .filter_map(|(key, value)| match key {
            serde_yml::Value::String(key) => Some((key.clone(), value)),
            _ => None,
        })
        .collect()
}

fn value_as_string(value: &serde_yml::Value) -> Option<String> {
    match value {
        serde_yml::Value::String(value) => Some(value.clone()),
        serde_yml::Value::Number(value) => Some(value.to_string()),
        serde_yml::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn scalar_fact_value(value: &serde_yml::Value) -> Option<String> {
    let value = value_as_string(value)?;
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    Some(truncate_chars(&normalized, MAX_FACT_VALUE_CHARS))
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn fact_segment(segment: &str) -> String {
    join_key_path("", segment)
}

fn as_mapping(value: &serde_yml::Value) -> Option<&serde_yml::Mapping> {
    match value {
        serde_yml::Value::Mapping(map) => Some(map),
        _ => None,
    }
}

fn as_sequence(value: &serde_yml::Value) -> Option<&Vec<serde_yml::Value>> {
    match value {
        serde_yml::Value::Sequence(sequence) => Some(sequence),
        _ => None,
    }
}
