//! Stage 1 — Intake. Validates an `intent-card.json` against the
//! `autobuilder.intent_card.v1` shape.
//!
//! The conversational 5-Whys interview itself lives in the skill's prompt
//! (`prompts/prd-intake-5whys.md`); this subcommand only enforces the
//! schema contract on the JSON the interview produces. The schema is
//! hand-validated (not via a general JSON Schema engine) so we don't have
//! to drag in a heavyweight validator + its rustc-1.86 transitive deps.
//! The checks mirror `~/.claude/skills/autobuilder/schemas/intent-card.schema.json`
//! field-for-field; if that file is the source of truth, this validator
//! must stay in sync (see ACs in autobuilder's own intent-card).

use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Path to the intent-card.json to validate.
    #[arg(long)]
    pub validate: PathBuf,
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

    // schema must equal the const.
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

    check_string_len(obj.get("created_at"), "/created_at", 1, usize::MAX, &mut errs);

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
    Ok(())
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
    }
}

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
        let Some(o) = entry.as_object() else {
            errs.push(format!("/five_whys_trace/{i}: expected object, got {}", kind(entry)));
            continue;
        };
        for f in ["why", "q", "a"] {
            if !o.contains_key(f) {
                errs.push(format!("/five_whys_trace/{i}: missing `{f}`"));
            }
        }
        if let Some(w) = o.get("why").and_then(Value::as_i64) {
            if !(1..=5).contains(&w) {
                errs.push(format!("/five_whys_trace/{i}/why: must be 1..=5, got {w}"));
            }
        }
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
