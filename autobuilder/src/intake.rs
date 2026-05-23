//! Stage 1 — Intake. Validates an `intent-card.json` against the
//! `autobuilder.intent_card.v1` shape.
//!
//! The conversational 5-Whys interview itself lives in the skill's prompt
//! (`prompts/prd-intake-5whys.md`); this subcommand only enforces the
//! schema contract on the JSON the interview produces. The schema is
//! hand-validated (not via a general JSON Schema engine) so we don't have
//! to drag in a heavyweight validator + its rustc-1.86 transitive deps.
//!
//! These checks mirror the JSON Schema at
//! `~/.claude/skills/autobuilder/schemas/intent-card.schema.json`. The
//! coverage includes: required fields, types, enum values, length and
//! pattern constraints on strings, `additionalProperties: false` at every
//! object level, `format: date-time` on `created_at`, the
//! `ambiguities_resolved` shape, and `msrv` / `max_deps` constraints.
//! Drift from the schema is the explicit failure mode this comment
//! protects against — keep them in sync.

use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

use crate::receipt;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Path to the intent-card.json to validate.
    #[arg(long)]
    pub validate: PathBuf,

    /// When set, after successful validation also write the intent-card
    /// (digest-bound) to `<project>/target/autobuilder/receipts/intake.json`
    /// so the gate sees it without a manual `cp`. When unset, intake is
    /// validation-only and leaves no files behind.
    #[arg(long)]
    pub project: Option<PathBuf>,
}

// Required top-level fields per schema.
const REQUIRED: &[&str] = &[
    "schema",
    "prd_source",
    "root_motivation",
    "user_persona",
    "unfakeable_metric",
    "acceptance_criteria",
    "scope",
    "non_goals",
    "hard_constraints",
    "five_whys_trace",
    "created_at",
];

// All allowed top-level fields (required ∪ optional). Anything else is an
// additionalProperties violation.
const ALLOWED_TOP: &[&str] = &[
    "schema",
    "prd_source",
    "root_motivation",
    "user_persona",
    "unfakeable_metric",
    "acceptance_criteria",
    "scope",
    "non_goals",
    "hard_constraints",
    "five_whys_trace",
    "created_at",
    "intent_slug",
    "ambiguities_resolved",
];

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
#[allow(clippy::too_many_lines)] // single linear schema-validation pipeline
pub(crate) fn run(args: Args) -> Result<()> {
    let text = fs::read_to_string(&args.validate)
        .with_context(|| format!("missing intent-card at {}", args.validate.display()))?;
    let card: Value = serde_json::from_str(&text).context("intent-card is not valid JSON")?;

    let mut errs: Vec<String> = Vec::new();
    let Some(obj) = card.as_object() else {
        return Err(anyhow!("intent-card must be a JSON object, got {}", kind(&card)));
    };

    for f in REQUIRED {
        if !obj.contains_key(*f) {
            errs.push(format!("/: missing required field `{f}`"));
        }
    }
    check_additional_properties(obj, ALLOWED_TOP, "/", &mut errs);

    if let Some(v) = obj.get("schema") {
        if v.as_str() != Some("autobuilder.intent_card.v1") {
            errs.push(format!(
                "/schema: expected \"autobuilder.intent_card.v1\", got {v}"
            ));
        }
    }

    check_string_len(obj.get("root_motivation"), "/root_motivation", 1, 1000, &mut errs);
    check_string_len(obj.get("user_persona"), "/user_persona", 1, 500, &mut errs);
    check_string_len(obj.get("prd_source"), "/prd_source", 1, usize::MAX, &mut errs);

    if let Some(m) = obj.get("unfakeable_metric") {
        check_unfakeable_metric(m, &mut errs);
    }

    if let Some(acs) = obj.get("acceptance_criteria") {
        check_acceptance_criteria(acs, &mut errs);
    }

    check_string_array(obj.get("scope"), "/scope", &mut errs);
    check_string_array(obj.get("non_goals"), "/non_goals", &mut errs);

    if let Some(hc) = obj.get("hard_constraints") {
        check_hard_constraints(hc, &mut errs);
    }

    if let Some(t) = obj.get("five_whys_trace") {
        check_five_whys(t, &mut errs);
    }

    if let Some(ar) = obj.get("ambiguities_resolved") {
        check_ambiguities_resolved(ar, &mut errs);
    }

    check_created_at(obj.get("created_at"), &mut errs);

    if let Some(slug) = obj.get("intent_slug").and_then(Value::as_str) {
        match Regex::new(r"^[a-z0-9][a-z0-9-]{0,62}$") {
            Ok(re) if !re.is_match(slug) => errs.push(format!(
                "/intent_slug: must match ^[a-z0-9][a-z0-9-]{{0,62}}$, got \"{slug}\""
            )),
            Ok(_) => {}
            Err(e) => errs.push(format!("internal: intent_slug regex failed to compile: {e}")),
        }
    }

    if !errs.is_empty() {
        eprintln!("intake: {} schema violation(s) in {}", errs.len(), args.validate.display());
        for line in &errs {
            eprintln!("  {line}");
        }
        return Err(anyhow!(
            "intent-card failed autobuilder.intent_card.v1 validation"
        ));
    }

    let slug = obj
        .get("intent_slug")
        .and_then(Value::as_str)
        .unwrap_or("(no intent_slug)");
    let target_kind = card
        .pointer("/hard_constraints/target_kind")
        .and_then(Value::as_str)
        .unwrap_or("?");
    let ac_count = obj
        .get("acceptance_criteria")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    println!(
        "intake: {} valid (slug={slug} target={target_kind} acs={ac_count})",
        args.validate.display()
    );

    if let Some(project) = args.project.as_ref() {
        let project = project
            .canonicalize()
            .with_context(|| format!("project path not found: {}", project.display()))?;
        let receipt_path = project.join("target/autobuilder/receipts/intake.json");
        receipt::write(&receipt_path, card)?;
        println!(
            "intake: wrote receipt to {} (digest-bound; gate-ready)",
            receipt_path.display()
        );
    }

    Ok(())
}

fn check_additional_properties(
    obj: &serde_json::Map<String, Value>,
    allowed: &[&str],
    path: &str,
    errs: &mut Vec<String>,
) {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            errs.push(format!("{path}: additional property `{key}` is not allowed"));
        }
    }
}

fn check_string_len(v: Option<&Value>, path: &str, min: usize, max: usize, errs: &mut Vec<String>) {
    let Some(v) = v else { return };
    match v.as_str() {
        Some(s) if s.len() >= min && s.len() <= max => {}
        Some(s) => errs.push(format!(
            "{path}: length {} not in [{min}, {max}]",
            s.len()
        )),
        None => errs.push(format!("{path}: expected string, got {}", kind(v))),
    }
}

fn check_string_array(v: Option<&Value>, path: &str, errs: &mut Vec<String>) {
    let Some(v) = v else { return };
    match v.as_array() {
        Some(arr) => {
            for (i, item) in arr.iter().enumerate() {
                if !item.is_string() {
                    errs.push(format!("{path}/{i}: expected string, got {}", kind(item)));
                }
            }
        }
        None => errs.push(format!("{path}: expected array, got {}", kind(v))),
    }
}

fn check_unfakeable_metric(v: &Value, errs: &mut Vec<String>) {
    let Some(o) = v.as_object() else {
        errs.push(format!("/unfakeable_metric: expected object, got {}", kind(v)));
        return;
    };
    for f in ["name", "lower_is_better", "harness_command"] {
        if !o.contains_key(f) {
            errs.push(format!("/unfakeable_metric: missing required `{f}`"));
        }
    }
    check_additional_properties(
        o,
        &["name", "lower_is_better", "harness_command", "target"],
        "/unfakeable_metric",
        errs,
    );
    if let Some(n) = o.get("name") {
        if !n.is_string() {
            errs.push(format!("/unfakeable_metric/name: expected string, got {}", kind(n)));
        }
    }
    if let Some(b) = o.get("lower_is_better") {
        if !b.is_boolean() {
            errs.push(format!("/unfakeable_metric/lower_is_better: expected bool, got {}", kind(b)));
        }
    }
    if let Some(c) = o.get("harness_command") {
        if !c.is_string() {
            errs.push(format!("/unfakeable_metric/harness_command: expected string, got {}", kind(c)));
        }
    }
    if let Some(t) = o.get("target") {
        if !t.is_null() && !t.is_number() {
            errs.push(format!(
                "/unfakeable_metric/target: expected number or null, got {}",
                kind(t)
            ));
        }
    }
}

fn check_acceptance_criteria(v: &Value, errs: &mut Vec<String>) {
    let Some(arr) = v.as_array() else {
        errs.push(format!("/acceptance_criteria: expected array, got {}", kind(v)));
        return;
    };
    if arr.is_empty() {
        errs.push("/acceptance_criteria: must have at least 1 item".to_owned());
    }
    let id_re = match Regex::new(r"^AC[0-9]+$") {
        Ok(re) => re,
        Err(e) => {
            errs.push(format!("internal: AC id regex failed to compile: {e}"));
            return;
        }
    };
    let levels = ["MUST", "SHOULD", "MAY"];
    for (i, ac) in arr.iter().enumerate() {
        let Some(o) = ac.as_object() else {
            errs.push(format!("/acceptance_criteria/{i}: expected object, got {}", kind(ac)));
            continue;
        };
        for f in ["id", "level", "test", "description"] {
            if !o.contains_key(f) {
                errs.push(format!("/acceptance_criteria/{i}: missing `{f}`"));
            }
        }
        check_additional_properties(
            o,
            &["id", "level", "test", "description"],
            &format!("/acceptance_criteria/{i}"),
            errs,
        );
        if let Some(id) = o.get("id").and_then(Value::as_str) {
            if !id_re.is_match(id) {
                errs.push(format!(
                    "/acceptance_criteria/{i}/id: must match ^AC[0-9]+$, got \"{id}\""
                ));
            }
        }
        if let Some(level) = o.get("level").and_then(Value::as_str) {
            if !levels.contains(&level) {
                errs.push(format!(
                    "/acceptance_criteria/{i}/level: must be one of {levels:?}, got \"{level}\""
                ));
            }
        }
        check_string_len(o.get("description"), &format!("/acceptance_criteria/{i}/description"), 1, 500, errs);
        if let Some(t) = o.get("test") {
            if !t.is_string() {
                errs.push(format!(
                    "/acceptance_criteria/{i}/test: expected string, got {}",
                    kind(t)
                ));
            }
        }
    }
}

#[allow(clippy::too_many_lines)] // one validator per schema field; splitting per-field would just shuffle the same lines into named helpers
fn check_hard_constraints(v: &Value, errs: &mut Vec<String>) {
    let Some(o) = v.as_object() else {
        errs.push(format!("/hard_constraints: expected object, got {}", kind(v)));
        return;
    };
    for f in ["rust_edition", "target_kind", "deny_unsafe"] {
        if !o.contains_key(f) {
            errs.push(format!("/hard_constraints: missing required `{f}`"));
        }
    }
    check_additional_properties(
        o,
        &["rust_edition", "target_kind", "deny_unsafe", "max_deps", "msrv", "additional"],
        "/hard_constraints",
        errs,
    );
    if let Some(e) = o.get("rust_edition").and_then(Value::as_str) {
        if !["2021", "2024"].contains(&e) {
            errs.push(format!(
                "/hard_constraints/rust_edition: must be 2021 or 2024, got \"{e}\""
            ));
        }
    }
    if let Some(t) = o.get("target_kind").and_then(Value::as_str) {
        if !["cli", "lib"].contains(&t) {
            errs.push(format!(
                "/hard_constraints/target_kind: must be cli or lib, got \"{t}\""
            ));
        }
    }
    if let Some(d) = o.get("deny_unsafe") {
        if !d.is_boolean() {
            errs.push(format!(
                "/hard_constraints/deny_unsafe: expected bool, got {}",
                kind(d)
            ));
        }
    }
    if let Some(m) = o.get("max_deps") {
        match m {
            Value::Null => {}
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    if i < 0 {
                        errs.push(format!(
                            "/hard_constraints/max_deps: must be ≥ 0, got {i}"
                        ));
                    }
                } else {
                    errs.push(format!(
                        "/hard_constraints/max_deps: expected integer, got {n}"
                    ));
                }
            }
            other => errs.push(format!(
                "/hard_constraints/max_deps: expected integer or null, got {}",
                kind(other)
            )),
        }
    }
    if let Some(msrv) = o.get("msrv") {
        match msrv {
            Value::Null => {}
            Value::String(s) => {
                match Regex::new(r"^[0-9]+\.[0-9]+(\.[0-9]+)?$") {
                    Ok(re) if !re.is_match(s) => errs.push(format!(
                        "/hard_constraints/msrv: must match ^[0-9]+\\.[0-9]+(\\.[0-9]+)?$, got \"{s}\""
                    )),
                    Ok(_) => {}
                    Err(e) => errs.push(format!("internal: msrv regex failed to compile: {e}")),
                }
            }
            other => errs.push(format!(
                "/hard_constraints/msrv: expected string or null, got {}",
                kind(other)
            )),
        }
    }
    if let Some(additional) = o.get("additional") {
        if let Some(addl) = additional.as_object() {
            for (k, val) in addl {
                if !val.is_string() && !val.is_number() && !val.is_boolean() {
                    errs.push(format!(
                        "/hard_constraints/additional/{k}: expected string|number|bool, got {}",
                        kind(val)
                    ));
                }
            }
        } else {
            errs.push(format!(
                "/hard_constraints/additional: expected object, got {}",
                kind(additional)
            ));
        }
    }
}

fn check_five_whys(v: &Value, errs: &mut Vec<String>) {
    let Some(arr) = v.as_array() else {
        errs.push(format!("/five_whys_trace: expected array, got {}", kind(v)));
        return;
    };
    if arr.is_empty() || arr.len() > 5 {
        errs.push(format!(
            "/five_whys_trace: must have 1..=5 items, got {}",
            arr.len()
        ));
    }
    for (i, entry) in arr.iter().enumerate() {
        let path = format!("/five_whys_trace/{i}");
        let Some(o) = entry.as_object() else {
            errs.push(format!("{path}: expected object, got {}", kind(entry)));
            continue;
        };
        for f in ["why", "q", "a"] {
            if !o.contains_key(f) {
                errs.push(format!("{path}: missing `{f}`"));
            }
        }
        check_additional_properties(o, &["why", "q", "a"], &path, errs);
        if let Some(w) = o.get("why").and_then(Value::as_i64) {
            if !(1..=5).contains(&w) {
                errs.push(format!("{path}/why: must be 1..=5, got {w}"));
            }
        }
        check_string_len(o.get("q"), &format!("{path}/q"), 1, usize::MAX, errs);
        check_string_len(o.get("a"), &format!("{path}/a"), 1, usize::MAX, errs);
    }
}

fn check_ambiguities_resolved(v: &Value, errs: &mut Vec<String>) {
    let Some(arr) = v.as_array() else {
        errs.push(format!(
            "/ambiguities_resolved: expected array, got {}",
            kind(v)
        ));
        return;
    };
    for (i, entry) in arr.iter().enumerate() {
        let path = format!("/ambiguities_resolved/{i}");
        let Some(o) = entry.as_object() else {
            errs.push(format!("{path}: expected object, got {}", kind(entry)));
            continue;
        };
        for f in ["question", "resolution"] {
            if !o.contains_key(f) {
                errs.push(format!("{path}: missing `{f}`"));
            }
        }
        check_additional_properties(o, &["question", "resolution"], &path, errs);
        if let Some(q) = o.get("question") {
            if !q.is_string() {
                errs.push(format!("{path}/question: expected string, got {}", kind(q)));
            }
        }
        if let Some(r) = o.get("resolution") {
            if !r.is_string() {
                errs.push(format!("{path}/resolution: expected string, got {}", kind(r)));
            }
        }
    }
}

fn check_created_at(v: Option<&Value>, errs: &mut Vec<String>) {
    let Some(v) = v else { return };
    let Some(s) = v.as_str() else {
        errs.push(format!("/created_at: expected string, got {}", kind(v)));
        return;
    };
    // RFC3339 / JSON Schema format=date-time: YYYY-MM-DDTHH:MM:SS(.fff)?(Z|±HH:MM)
    let pattern =
        r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$";
    match Regex::new(pattern) {
        Ok(re) if !re.is_match(s) => errs.push(format!(
            "/created_at: must be RFC3339 date-time, got \"{s}\""
        )),
        Ok(_) => {}
        Err(e) => errs.push(format!("internal: created_at regex failed to compile: {e}")),
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
