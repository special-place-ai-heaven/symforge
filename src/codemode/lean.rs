use serde_json::{Map, Value};

const FIELD_SEPARATOR: &str = "\t";
const RECORD_MARKER: &str = "@";
const GROUP_COLUMN: &str = "path";
const FREE_TEXT_COLUMNS: [&str; 2] = ["text", "preview"];

pub fn render_lean(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => render_array(items),
        Value::Object(map) if !map.is_empty() => {
            let mut lines = Vec::new();
            push_sections(&mut lines, "", map);
            lines.join("\n")
        }
        other => compact_json(other),
    }
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

fn cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => compact_json(other),
    }
}

fn push_sections(lines: &mut Vec<String>, prefix: &str, map: &Map<String, Value>) {
    for (key, child) in map {
        let label = format!("{prefix}{key}");
        match child {
            Value::Object(inner) if !inner.is_empty() => {
                push_sections(lines, &format!("{label}."), inner);
            }
            Value::String(s) if s.contains('\n') => {
                lines.push(format!("{label}:"));
                lines.push(s.clone());
            }
            Value::String(s) => lines.push(format!("{label}: {s}")),
            scalar if is_scalar(scalar) => lines.push(format!("{label}: {}", compact_json(scalar))),
            composite => {
                lines.push(format!("{label}:"));
                lines.push(render_lean(composite));
            }
        }
    }
}

fn render_array(items: &[Value]) -> String {
    let rows: Option<Vec<&Map<String, Value>>> = items.iter().map(Value::as_object).collect();
    match rows {
        Some(rows) if !rows.is_empty() => table_columns(&rows)
            .map_or_else(|| render_records(&rows), |cols| render_table(&rows, &cols)),
        _ => compact_json(&Value::Array(items.to_vec())),
    }
}

fn table_columns(rows: &[&Map<String, Value>]) -> Option<Vec<String>> {
    let first = rows[0];
    if first.is_empty() {
        return None;
    }
    let same_keys = rows
        .iter()
        .all(|row| row.len() == first.len() && first.keys().all(|k| row.contains_key(k)));
    if !same_keys {
        return None;
    }
    let (mut cols, free): (Vec<String>, Vec<String>) = first
        .keys()
        .cloned()
        .partition(|k| !FREE_TEXT_COLUMNS.contains(&k.as_str()));
    cols.extend(free);
    let last = cols.len() - 1;
    let fits = rows.iter().all(|row| {
        cols.iter().enumerate().all(|(i, col)| match &row[col] {
            Value::String(s) => !s.contains('\n') && (i == last || !s.contains('\t')),
            other => is_scalar(other),
        })
    });
    fits.then_some(cols)
}

fn render_table(rows: &[&Map<String, Value>], cols: &[String]) -> String {
    let grouped = cols.len() > 1 && cols.iter().any(|c| c == GROUP_COLUMN);
    let shown: Vec<&String> = cols
        .iter()
        .filter(|c| !(grouped && *c == GROUP_COLUMN))
        .collect();
    let mut lines = vec![
        shown
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(FIELD_SEPARATOR),
    ];
    let mut current_group: Option<String> = None;
    for row in rows {
        if grouped {
            let group = cell(&row[GROUP_COLUMN]);
            if current_group.as_deref() != Some(group.as_str()) {
                lines.push(format!("{RECORD_MARKER} {group}"));
                current_group = Some(group);
            }
        }
        let cells: Vec<String> = shown.iter().map(|c| cell(&row[c.as_str()])).collect();
        lines.push(cells.join(FIELD_SEPARATOR));
    }
    lines.join("\n")
}

fn render_records(rows: &[&Map<String, Value>]) -> String {
    let mut lines = Vec::new();
    for row in rows {
        let mut header = Vec::new();
        let mut bodies = Vec::new();
        for (key, value) in *row {
            match value {
                Value::String(s) if s.contains('\n') || s.contains('\t') => bodies.push((key, s)),
                scalar if is_scalar(scalar) => header.push(format!("{key}={}", cell(scalar))),
                composite => header.push(format!("{key}={}", compact_json(composite))),
            }
        }
        lines.push(format!("{RECORD_MARKER} {}", header.join(FIELD_SEPARATOR)));
        for (key, body) in bodies {
            lines.push(format!("{key}:"));
            lines.push(body.clone());
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strings_print_raw_and_other_scalars_print_as_json() {
        assert_eq!(render_lean(&json!("a\n\"b\"")), "a\n\"b\"");
        assert_eq!(render_lean(&json!(3)), "3");
        assert_eq!(render_lean(&json!(true)), "true");
        assert_eq!(render_lean(&Value::Null), "null");
        assert_eq!(render_lean(&json!([])), "[]");
        assert_eq!(render_lean(&json!({})), "{}");
    }

    #[test]
    fn uniform_rows_print_as_a_table_grouped_by_path_with_null_cells_empty() {
        let value = json!([
            {"enclosing_symbol": "main", "line": 3, "path": "src/a.rs", "text": "let x = f(1);"},
            {"enclosing_symbol": null, "line": 9, "path": "src/a.rs", "text": "f(2)"},
            {"enclosing_symbol": "g", "line": 1, "path": "src/b.rs", "text": "f(3)"},
        ]);
        assert_eq!(
            render_lean(&value),
            "enclosing_symbol\tline\ttext\n@ src/a.rs\nmain\t3\tlet x = f(1);\n\t9\tf(2)\n@ src/b.rs\ng\t1\tf(3)"
        );
    }

    #[test]
    fn a_path_returning_to_an_earlier_value_starts_a_new_group() {
        let value = json!([
            {"n": 1, "path": "a"},
            {"n": 2, "path": "b"},
            {"n": 3, "path": "a"},
        ]);
        assert_eq!(render_lean(&value), "n\n@ a\n1\n@ b\n2\n@ a\n3");
    }

    #[test]
    fn rows_without_a_path_column_print_every_column() {
        let value = json!([{"kind": "fn", "name": "x"}, {"kind": "struct", "name": "y"}]);
        assert_eq!(render_lean(&value), "kind\tname\nfn\tx\nstruct\ty");
    }

    #[test]
    fn rows_with_multi_line_values_print_as_records_with_raw_bodies() {
        let value = json!([{
            "name": "f",
            "path": "src/a.rs",
            "refusal": null,
            "source": "fn f() {\n    \"q\"\n}",
            "start_line": 1,
        }]);
        assert_eq!(
            render_lean(&value),
            "@ name=f\tpath=src/a.rs\trefusal=\tstart_line=1\nsource:\nfn f() {\n    \"q\"\n}"
        );
    }

    #[test]
    fn rows_with_differing_keys_print_as_records() {
        let value = json!([{"a": 1}, {"b": [1, 2]}]);
        assert_eq!(render_lean(&value), "@ a=1\n@ b=[1,2]");
    }

    #[test]
    fn objects_print_labelled_sections_recursively() {
        let value = json!({
            "meta": {"count": 2, "inner": {"ok": true}},
            "note": "two\nlines",
            "refs": [{"line": 1, "path": "p"}],
            "syms": [{"kind": "fn", "name": "s"}],
            "word": "plain",
        });
        assert_eq!(
            render_lean(&value),
            "meta.count: 2\nmeta.inner.ok: true\nnote:\ntwo\nlines\nrefs:\nline\n@ p\n1\nsyms:\nkind\tname\nfn\ts\nword: plain"
        );
    }

    #[test]
    fn mixed_and_scalar_lists_fall_back_to_compact_json() {
        assert_eq!(
            render_lean(&json!([1, "a", {"b": 2}])),
            r#"[1,"a",{"b":2}]"#
        );
        assert_eq!(render_lean(&json!({"xs": [1, 2]})), "xs:\n[1,2]");
    }

    fn next(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *seed >> 33
    }

    fn generate(seed: &mut u64, depth: u32) -> Value {
        const WORDS: [&str; 6] = [
            "alpha",
            "two words",
            "line\nbreak",
            "tab\there",
            "x\"q\"",
            "",
        ];
        let pick = if depth == 0 {
            next(seed) % 3
        } else {
            next(seed) % 6
        };
        match pick {
            0 => json!(next(seed) % 1000),
            1 => json!(WORDS[(next(seed) % WORDS.len() as u64) as usize]),
            2 => json!(next(seed).is_multiple_of(2)),
            3 => {
                let keys = ["path", "name", "text", "line"];
                let width = 1 + (next(seed) % 4) as usize;
                let uniform = next(seed).is_multiple_of(2);
                let len = 1 + (next(seed) % 4) as usize;
                Value::Array(
                    (0..len)
                        .map(|_| {
                            let start = if uniform {
                                0
                            } else {
                                (next(seed) % 2) as usize
                            };
                            let mut row = Map::new();
                            for key in &keys[start..(start + width).min(keys.len())] {
                                row.insert((*key).to_owned(), generate(seed, depth - 1));
                            }
                            Value::Object(row)
                        })
                        .collect(),
                )
            }
            4 => {
                let mut map = Map::new();
                for i in 0..(1 + next(seed) % 3) {
                    map.insert(format!("k{i}"), generate(seed, depth - 1));
                }
                Value::Object(map)
            }
            _ => Value::Array(
                (0..(next(seed) % 3))
                    .map(|_| generate(seed, depth - 1))
                    .collect(),
            ),
        }
    }

    fn scalar_leaves(value: &Value, out: &mut Vec<Value>) {
        match value {
            Value::Array(items) => items.iter().for_each(|v| scalar_leaves(v, out)),
            Value::Object(map) => map.values().for_each(|v| scalar_leaves(v, out)),
            scalar => out.push(scalar.clone()),
        }
    }

    #[test]
    fn every_scalar_leaf_of_the_input_appears_in_the_rendering() {
        let mut seed = 0x5eed_u64;
        for _ in 0..500 {
            let value = generate(&mut seed, 3);
            let text = render_lean(&value);
            let mut leaves = Vec::new();
            scalar_leaves(&value, &mut leaves);
            for leaf in leaves {
                let present = match &leaf {
                    Value::Null => true,
                    Value::String(s) => {
                        let escaped = compact_json(&leaf);
                        text.contains(s.as_str()) || text.contains(&escaped[1..escaped.len() - 1])
                    }
                    other => text.contains(&compact_json(other)),
                };
                assert!(
                    present,
                    "leaf {leaf} missing from rendering of {value}:\n{text}"
                );
            }
        }
    }
}
