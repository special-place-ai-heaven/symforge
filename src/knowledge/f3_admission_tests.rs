use super::{DetectorFailure, SECRET_SCAN_MAX_BYTES, SecretScan, SecretSpansScan};
use super::{scan_secret_bytes, scan_secret_spans};

#[test]
fn python_lookup_without_binding_proof_is_withheld() {
    let path = "src/registration.py";
    let key = "password";
    for (receiver, indent, quote, end) in [
        // pallets/flask 3.1.2, examples/tutorial/flaskr/auth.py:55,89.
        ("request.form", "        ", '"', "\n"),
        ("request.form", "\t", '\'', "\r\n"),
        ("request.form", "", '"', ""),
        ("local.mapping", "", '"', "\n"),
    ] {
        let input = format!("{indent}{key} = {receiver}[{quote}{key}{quote}]{end}");
        let input = input.as_bytes();
        assert!(
            matches!(scan_secret_bytes(path, input), SecretScan::Sensitive { ref rule_ids, .. } if rule_ids.contains(&"secret.context-assignment"))
        );
        assert!(
            matches!(scan_secret_spans(path, input), SecretSpansScan::Spans(spans) if spans.iter().any(|span| span.rule_id == "secret.context-assignment"))
        );
    }
}

#[test]
fn literal_credentials_and_indeterminate_inputs_still_block() {
    let canary = ["F3", "Canary", "9284"].concat();
    let key = "password";
    let lookup = format!("request.form[\"{key}\"]");
    let cases = [
        ("src/registration.py", format!("{key} = \"{canary}\"")),
        (
            "src/registration.py",
            format!("{key} = request.form[\"{canary}\"]"),
        ),
        (
            "src/registration.py",
            format!("{key} = {lookup} or \"{canary}\""),
        ),
        (
            "src/registration.py",
            format!("{key} = {lookup} or (\n    \"{canary}\"\n)"),
        ),
        (
            "src/registration.py",
            format!("{key} = {lookup}\n    + \"{canary}\""),
        ),
        (
            "src/registration.py",
            format!("{key} = {lookup} or \"word word word word\""),
        ),
        (
            "src/registration.py",
            format!("{key} = request.form[\"{key}\""),
        ),
        ("app.toml", format!("{key} = {lookup}")),
    ];
    for (index, (path, body)) in cases.iter().enumerate() {
        assert!(
            matches!(scan_secret_bytes(path, body.as_bytes()), SecretScan::Sensitive { ref rule_ids, .. } if rule_ids.contains(&"secret.context-assignment")),
            "control {index}"
        );
        assert!(
            matches!(scan_secret_spans(path, body.as_bytes()), SecretSpansScan::Spans(spans) if spans.iter().any(|span| span.rule_id == "secret.context-assignment")),
            "control {index}"
        );
    }

    let oversized = vec![b'x'; SECRET_SCAN_MAX_BYTES + 1];
    assert_eq!(
        scan_secret_bytes("src/registration.py", &oversized),
        SecretScan::Indeterminate {
            reason: DetectorFailure::ResourceLimit
        }
    );
    assert_eq!(
        scan_secret_spans("src/registration.py", &oversized),
        SecretSpansScan::Indeterminate {
            reason: DetectorFailure::ResourceLimit
        }
    );
}

#[test]
fn rust_match_arm_context_keeps_literal_and_comment_protection() {
    let kind = "Token";
    let key = "password";
    let canary = ["F3", "Canary", "9284"].concat();
    let source = |body| {
        format!(
            "fn demo(x: {kind}) {{ match x {{\n {kind}::RecursivePrefix => {{\n {body}\n }},\n _ => (),\n }} }}"
        )
    };
    let clean = source("emit(1);");
    let path = "src/glob.rs";
    assert_eq!(scan_secret_bytes(path, clean.as_bytes()), SecretScan::Clean);
    assert!(
        matches!(scan_secret_spans(path, clean.as_bytes()), SecretSpansScan::Spans(spans) if spans.is_empty())
    );

    let assignment = format!("{key} = \"{canary}\"");
    let literal = format!("emit(\"{canary}\");");
    let direct = format!("let {assignment};");
    let comment = format!("// `{canary}`\n emit(1);");
    let cases = [
        (path, source(&literal)),
        (path, source(&direct)),
        (path, source(&comment)),
        ("settings/glob.rs", source(&direct)),
        ("app.toml", assignment),
    ];
    for (index, (path, body)) in cases.iter().enumerate() {
        assert!(
            matches!(scan_secret_bytes(path, body.as_bytes()), SecretScan::Sensitive { ref rule_ids, .. } if rule_ids.contains(&"secret.context-assignment")),
            "control {index}"
        );
        assert!(
            matches!(scan_secret_spans(path, body.as_bytes()), SecretSpansScan::Spans(spans) if spans.iter().any(|span| span.rule_id == "secret.context-assignment")),
            "control {index}"
        );
    }
}

#[test]
fn rust_literal_free_declarations_end_before_unrelated_statements() {
    let key = "token";
    let kind = "Token";
    let long_type = "LongType".repeat(80);
    let long_arm = "current_value; ".repeat(50);
    for body in [
        format!("fn demo() {{ let {key} = current_value; log(\"operation_completed\"); }}"),
        format!("fn demo() {{ let {key} = state.current_value; log(\"operation_completed\"); }}"),
        format!("struct Demo {{ {key}: Arc<{long_type}>, }}"),
        format!(
            "fn demo(x: {kind}) {{ match x {{ {kind}::RecursivePrefix => {{ {long_arm} }}, _ => (), }} }}"
        ),
    ] {
        assert_eq!(
            scan_secret_bytes("src/demo.rs", body.as_bytes()),
            SecretScan::Clean
        );
        assert!(
            matches!(scan_secret_spans("src/demo.rs", body.as_bytes()), SecretSpansScan::Spans(spans) if spans.is_empty())
        );
    }
}

#[test]
fn rust_syntax_proof_rejects_numeric_array_call_and_macro_values() {
    let key = "token";
    for value in [
        "123456789",
        "[12345678]",
        "current_value()",
        "make_value!()",
    ] {
        let body = format!("fn demo() {{ let {key} = {value}; log(\"operation_completed\"); }}");
        let match_start = body.find(key).expect("binding key");
        let value_start = body.find(" = ").expect("binding separator") + 3;
        assert!(!super::syntax_proves_no_payload(
            "src/demo.rs",
            body.as_bytes(),
            match_start,
            value_start,
            value_start + value.len(),
            &std::cell::OnceCell::new(),
        ));
    }
}

#[test]
fn rust_scoped_calls_use_their_whole_statement_boundary() {
    let kind = "TypeToken";
    let canary = ["F3", "Canary", "9284"].concat();
    for body in [
        format!(
            "fn demo() {{ let result = {kind}::convert(current_value); emit(\"operation_completed\"); }}"
        ),
        format!("fn demo() {{ {kind}::convert(current_value); emit(\"operation_completed\"); }}"),
    ] {
        assert_eq!(
            scan_secret_bytes("src/demo.rs", body.as_bytes()),
            SecretScan::Clean
        );
        assert!(
            matches!(scan_secret_spans("src/demo.rs", body.as_bytes()), SecretSpansScan::Spans(spans) if spans.is_empty())
        );
    }
    for argument in [
        format!("\"{canary}\""),
        "[12345678]".to_owned(),
        "make_value!()".to_owned(),
        format!("current_value /* \"{canary}\" */"),
    ] {
        let body = format!(
            "fn demo() {{ let result = {kind}::convert({argument}); emit(\"operation_completed\"); }}"
        );
        assert!(matches!(
            scan_secret_bytes("src/demo.rs", body.as_bytes()),
            SecretScan::Sensitive { .. }
        ));
        assert!(
            matches!(scan_secret_spans("src/demo.rs", body.as_bytes()), SecretSpansScan::Spans(spans) if !spans.is_empty())
        );
    }
    let chained = format!(
        "fn demo() {{ let result = {kind}::convert(current_value).or_else(|| \"{canary}\"); }}"
    );
    assert!(matches!(
        scan_secret_bytes("src/demo.rs", chained.as_bytes()),
        SecretScan::Sensitive { .. }
    ));
    assert!(
        matches!(scan_secret_spans("src/demo.rs", chained.as_bytes()), SecretSpansScan::Spans(spans) if !spans.is_empty())
    );
}

#[test]
fn rust_scoped_patterns_do_not_treat_prose_or_regex_punctuation_as_credentials() {
    let kind = "Token";
    let key = "password";
    let long_arm = "current_value; ".repeat(45);
    let canary = ["F3", "Canary", "9284"].concat();
    let source = |body: &str| {
        format!(
            "fn demo(x: {kind}) {{ match x {{ {kind}::RecursivePrefix => {{\n {body}\n }}, _ => (), }} }}"
        )
    };
    for body in [
        format!("// This branch's progress follows the current state.\n {long_arm}"),
        "emit(\"(?:/|.*/)\");".to_owned(),
        "emit(r#\"(?:/|.*/)\"#);".to_owned(),
        format!("emit(b'/'); {long_arm}"),
    ] {
        let body = source(&body);
        assert_eq!(
            scan_secret_bytes("src/demo.rs", body.as_bytes()),
            SecretScan::Clean
        );
        assert!(
            matches!(scan_secret_spans("src/demo.rs", body.as_bytes()), SecretSpansScan::Spans(spans) if spans.is_empty())
        );
    }
    for body in [
        format!("// `{canary}`\n {long_arm}"),
        format!("// {canary}\n {long_arm}"),
        format!("// \"word word word word\"\n {long_arm}"),
        format!("// a'word word word word'b\n {long_arm}"),
        format!("emit(\"{canary}\");"),
        "emit(\"!@$%^&*()-+\");".to_owned(),
        format!("let {key} = \"!@$%^&*()-+\";"),
        format!("emit(unknown_macro!({canary})); {long_arm}"),
    ] {
        let body = source(&body);
        assert!(matches!(
            scan_secret_bytes("src/demo.rs", body.as_bytes()),
            SecretScan::Sensitive { .. }
        ));
        assert!(
            matches!(scan_secret_spans("src/demo.rs", body.as_bytes()), SecretSpansScan::Spans(spans) if !spans.is_empty())
        );
    }
}

#[test]
fn rust_scoped_pattern_macros_require_closed_short_argument_material() {
    let kind = "Token";
    let long_arm = "current_value; ".repeat(45);
    let canary = ["F3", "Canary", "9284"].concat();
    let source = |body: &str| {
        format!(
            "fn demo(x: {kind}) {{ match x {{ {kind}::RecursivePrefix(v) => {{\n {body} {long_arm}\n }}, _ => (), }} }}"
        )
    };
    for body in ["assemble!(a, b);", "assemble![];"] {
        let clean = source(body);
        assert_eq!(
            scan_secret_bytes("src/demo.rs", clean.as_bytes()),
            SecretScan::Clean
        );
        assert!(
            matches!(scan_secret_spans("src/demo.rs", clean.as_bytes()), SecretSpansScan::Spans(spans) if spans.is_empty())
        );
    }
    for body in [
        format!("unknown_macro!({canary});"),
        format!("assemble!(\"{canary}\");"),
        "assemble!(abcd, efgh);".to_owned(),
        "assemble!([b'/', b'*']);".to_owned(),
        "assemble!(123456789);".to_owned(),
    ] {
        let body = source(&body);
        assert!(matches!(
            scan_secret_bytes("src/demo.rs", body.as_bytes()),
            SecretScan::Sensitive { .. }
        ));
        assert!(
            matches!(scan_secret_spans("src/demo.rs", body.as_bytes()), SecretSpansScan::Spans(spans) if !spans.is_empty())
        );
    }
}

#[test]
fn rust_source_proofs_do_not_consume_an_independent_credential_assignment() {
    let key = "token";
    let other_key = "password";
    let kind = "TypeToken";
    let canary = ["Boundary", "Canary", "9284"].concat();
    let regex_fragment = "(?:/|.*/)";
    for body in [
        format!("fn demo() {{let {key}=current_value;{other_key}=\"{canary}\";}}"),
        format!("fn demo() {{{kind}::convert(current_value);{other_key}=\"{canary}\";}}"),
        format!(
            "fn demo(x: {kind}) {{match x {{{kind}::RecursivePrefix=>{{{other_key}=\"{regex_fragment}\";}},_=>(),}}}}"
        ),
    ] {
        assert!(matches!(
            scan_secret_bytes("src/demo.rs", body.as_bytes()),
            SecretScan::Sensitive { .. }
        ));
        assert!(
            matches!(scan_secret_spans("src/demo.rs", body.as_bytes()), SecretSpansScan::Spans(spans) if !spans.is_empty())
        );
    }
}

#[test]
fn rust_declaration_proof_does_not_admit_literals_comments_or_parse_errors() {
    let key = "token";
    let kind = "Token";
    let canary = ["F3", "Canary", "9284"].concat();
    let cases = [
        (
            "src/demo.rs",
            format!("fn demo() {{ let {key} = \"{canary}\"; }}"),
        ),
        (
            "src/demo.rs",
            format!("fn demo() {{ let {key} = current_value.or_else(|| \"{canary}\"); }}"),
        ),
        (
            "src/demo.rs",
            format!("fn demo() {{ let {key} = 123456789; log(\"{canary}\"); }}"),
        ),
        (
            "src/demo.rs",
            format!("fn demo() {{ let {key} = [\"{canary}\"]; }}"),
        ),
        (
            "src/demo.rs",
            format!("fn demo() {{ let {key} = make_value!(); log(\"{canary}\"); }}"),
        ),
        (
            "src/demo.rs",
            format!(
                "fn demo(x: {kind}) {{ match x {{ {kind}::RecursivePrefix => current_value, {kind}::RecursiveSuffix => emit(\"{canary}\"), }} }}"
            ),
        ),
        (
            "src/demo.rs",
            format!("fn demo() {{ let {key} = current_value /* \"{canary}\" */; }}"),
        ),
        (
            "src/demo.rs",
            format!("/* let {key} = current_value; log(\"{canary}\"); */"),
        ),
        (
            "src/demo.rs",
            format!("fn demo() {{ log(r#\"{key} = current_value; log(\"{canary}\")\"#); }}"),
        ),
        (
            "src/demo.rs",
            format!("fn demo() {{ let {key} = current_value; log(\"{canary}\");"),
        ),
        (
            "settings/demo.rs",
            format!("fn demo() {{ let {key} = current_value; log(\"{canary}\"); }}"),
        ),
        ("app.toml", format!("{key} = current_value; \"{canary}\"")),
    ];
    for (index, (path, body)) in cases.iter().enumerate() {
        assert!(
            matches!(scan_secret_bytes(path, body.as_bytes()), SecretScan::Sensitive { ref rule_ids, .. } if rule_ids.contains(&"secret.context-assignment")),
            "control {index}"
        );
        assert!(
            matches!(scan_secret_spans(path, body.as_bytes()), SecretSpansScan::Spans(spans) if spans.iter().any(|span| span.rule_id == "secret.context-assignment")),
            "control {index}"
        );
    }
}
