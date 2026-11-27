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
