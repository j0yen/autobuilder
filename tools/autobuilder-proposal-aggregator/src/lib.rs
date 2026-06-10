use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

// ── Normalized record ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Record {
    pub slug: String,
    pub target_file: String,
    pub kind: String,
    pub rationale: String,
    pub id: String,
}

// ── Output types ─────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Cluster {
    pub target_file: String,
    pub kind: String,
    pub recurrence: usize,
    pub slugs: Vec<String>,
    pub exemplar_rationale: String,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Coverage {
    pub applied_filtered: usize,
    pub unparseable_skipped: usize,
    pub clusters_total: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Output {
    pub backlog: String,
    pub generated_proposals_read: usize,
    pub clusters: Vec<Cluster>,
    pub coverage: Coverage,
}

// ── Schema-flexible parser ───────────────────────────────────────────────────

fn extract_top_level_slug(obj: &serde_json::Map<String, Value>) -> Option<String> {
    for key in &["slug", "crate_name", "name"] {
        if let Some(Value::String(s)) = obj.get(*key) {
            if !s.is_empty() {
                return Some(s.clone());
            }
        }
    }
    None
}

/// Shape 1: top-level object with suggestions[] array
fn try_shape_suggestions_array(
    slug: &str,
    obj: &serde_json::Map<String, Value>,
) -> Option<Vec<Record>> {
    let suggestions = obj.get("suggestions")?.as_array()?;
    if suggestions.is_empty() {
        return None;
    }
    let mut records = Vec::new();
    for sug in suggestions {
        let sug_obj = match sug.as_object() {
            Some(o) => o,
            None => continue,
        };
        let target_file = sug_obj
            .get("target")
            .or_else(|| sug_obj.get("target_file"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let kind = sug_obj
            .get("type")
            .or_else(|| sug_obj.get("kind"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let rationale = sug_obj
            .get("rationale")
            .or_else(|| sug_obj.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let id = sug_obj
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !target_file.is_empty() {
            records.push(Record {
                slug: slug.to_string(),
                target_file,
                kind,
                rationale,
                id,
            });
        }
    }
    if records.is_empty() {
        None
    } else {
        Some(records)
    }
}

/// Shape 2: top-level PatchSuggestion — single suggestion at root with target_file
fn try_shape_patch_suggestion(
    slug: &str,
    obj: &serde_json::Map<String, Value>,
) -> Option<Vec<Record>> {
    let target_file = obj
        .get("target_file")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    let kind = obj
        .get("kind")
        .or_else(|| obj.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let rationale = obj
        .get("rationale")
        .or_else(|| obj.get("description"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(vec![Record {
        slug: slug.to_string(),
        target_file: target_file.to_string(),
        kind,
        rationale,
        id,
    }])
}

/// Shape 3: flat record with top-level target or target_file (fallback)
fn try_shape_flat_record(
    slug: &str,
    obj: &serde_json::Map<String, Value>,
) -> Option<Vec<Record>> {
    let target_file = obj
        .get("target")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    let rationale = obj
        .get("rationale")
        .or_else(|| obj.get("description"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let kind = obj
        .get("kind")
        .or_else(|| obj.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(vec![Record {
        slug: slug.to_string(),
        target_file: target_file.to_string(),
        kind,
        rationale,
        id,
    }])
}

/// Parse a single proposal JSON file into normalized records.
pub fn parse_proposal_file(path: &std::path::Path) -> Result<Vec<Record>, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    let value: Value =
        serde_json::from_str(&content).map_err(|e| format!("json parse error: {e}"))?;

    let obj = value
        .as_object()
        .ok_or_else(|| "top-level value is not an object".to_string())?;

    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let slug = extract_top_level_slug(obj).unwrap_or(file_stem);

    if let Some(records) = try_shape_suggestions_array(&slug, obj) {
        return Ok(records);
    }
    if let Some(records) = try_shape_patch_suggestion(&slug, obj) {
        return Ok(records);
    }
    if let Some(records) = try_shape_flat_record(&slug, obj) {
        return Ok(records);
    }

    Err("no recognized schema shape".to_string())
}

// ── Applied-log filter ───────────────────────────────────────────────────────

/// Load applied.log and return set of filtered identifiers.
pub fn load_applied_ids(path: &std::path::Path) -> HashSet<String> {
    let mut ids = HashSet::new();
    if !path.exists() {
        return ids;
    }
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return ids,
    };
    for line in content.lines() {
        let line = line.trim();
        if let Some(sha) = line.strip_prefix("applied-suggestion:") {
            ids.insert(sha.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("#REJECTED:") {
            ids.insert(rest.trim().to_string());
        }
    }
    ids
}

/// Return true if the record should be filtered out.
pub fn is_filtered(record: &Record, applied_ids: &HashSet<String>) -> bool {
    if !record.id.is_empty() && applied_ids.contains(&record.id) {
        return true;
    }
    if applied_ids.contains(&record.slug) {
        return true;
    }
    false
}

// ── Jaccard similarity ───────────────────────────────────────────────────────

/// Tokenize a string by splitting on whitespace and punctuation.
pub fn tokenize(s: &str) -> HashSet<String> {
    s.split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// Compute Jaccard similarity between two token sets.
pub fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

const JACCARD_THRESHOLD: f64 = 0.5;

// ── Clustering ───────────────────────────────────────────────────────────────

/// Cluster records by target_file, then by rationale Jaccard within target.
pub fn cluster_records(records: Vec<Record>) -> Vec<Cluster> {
    let mut by_target: HashMap<String, Vec<Record>> = HashMap::new();
    for record in records {
        by_target
            .entry(record.target_file.clone())
            .or_default()
            .push(record);
    }

    let mut clusters: Vec<Cluster> = Vec::new();

    let mut sorted_targets: Vec<String> = by_target.keys().cloned().collect();
    sorted_targets.sort();

    for target_file in sorted_targets {
        let recs = by_target.remove(&target_file).unwrap();

        type Group = (HashSet<String>, String, String, Vec<(String, String)>);
        // Greedy clustering by Jaccard threshold
        // Each group: (exemplar_tokens, exemplar_rationale, exemplar_kind, slug_id_pairs)
        let mut groups: Vec<Group> = Vec::new();

        for rec in recs {
            let tokens = tokenize(&rec.rationale);
            let mut best_idx: Option<usize> = None;
            let mut best_score = 0.0_f64;

            for (i, (ex_tokens, _, _, _)) in groups.iter().enumerate() {
                let score = jaccard(&tokens, ex_tokens);
                if score >= JACCARD_THRESHOLD && score > best_score {
                    best_score = score;
                    best_idx = Some(i);
                }
            }

            match best_idx {
                Some(idx) => {
                    groups[idx]
                        .3
                        .push((rec.slug.clone(), rec.id.clone()));
                }
                None => {
                    groups.push((
                        tokens,
                        rec.rationale.clone(),
                        rec.kind.clone(),
                        vec![(rec.slug.clone(), rec.id.clone())],
                    ));
                }
            }
        }

        for (_, exemplar_rationale, kind, slug_id_pairs) in groups {
            let mut seen_slugs: HashSet<String> = HashSet::new();
            for (slug, _) in &slug_id_pairs {
                seen_slugs.insert(slug.clone());
            }
            let mut slugs: Vec<String> = seen_slugs.into_iter().collect();
            slugs.sort();
            let recurrence = slugs.len();

            clusters.push(Cluster {
                target_file: target_file.clone(),
                kind,
                recurrence,
                slugs,
                exemplar_rationale,
                status: "open".to_string(),
            });
        }
    }

    clusters
}

/// Sort clusters: recurrence desc, target_file asc; slugs within cluster sorted.
pub fn sort_clusters(clusters: &mut [Cluster]) {
    clusters.sort_by(|a, b| {
        b.recurrence
            .cmp(&a.recurrence)
            .then_with(|| a.target_file.cmp(&b.target_file))
    });
    for c in clusters.iter_mut() {
        c.slugs.sort();
    }
}

// ── Shell-expand ~ ────────────────────────────────────────────────────────────

pub fn shellexpand(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{}", home.to_string_lossy(), rest);
        }
    }
    s.to_string()
}

// ── Human formatter ───────────────────────────────────────────────────────────

fn format_human(output: &Output) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Hardening Backlog — {} proposals read, {} clusters",
        output.generated_proposals_read, output.coverage.clusters_total
    ));
    lines.push(format!(
        "Coverage: applied_filtered={} unparseable_skipped={}",
        output.coverage.applied_filtered, output.coverage.unparseable_skipped
    ));
    lines.push(String::new());

    for (i, c) in output.clusters.iter().enumerate() {
        lines.push(format!(
            "{}. [rec={}] {} ({})",
            i + 1,
            c.recurrence,
            c.target_file,
            c.kind
        ));
        lines.push(format!("   Slugs: {}", c.slugs.join(", ")));
        lines.push(format!("   Rationale: {}", c.exemplar_rationale));
        lines.push(String::new());
    }

    lines.join("\n")
}

// ── Main pipeline ─────────────────────────────────────────────────────────────

pub fn run(
    proposals_dir: &str,
    applied_log_path: &str,
    min_recurrence: usize,
    format: &str,
) -> Result<String, String> {
    let proposals_dir_expanded = shellexpand(proposals_dir);
    let applied_log_expanded = shellexpand(applied_log_path);

    let proposals_path = PathBuf::from(&proposals_dir_expanded);
    let applied_path = PathBuf::from(&applied_log_expanded);

    let applied_ids = load_applied_ids(&applied_path);

    let entries = fs::read_dir(&proposals_path)
        .map_err(|e| format!("Cannot read proposals dir '{proposals_dir_expanded}': {e}"))?;

    let mut json_files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("hardening-backlog.json"))
        .collect();
    json_files.sort();

    let total_files = json_files.len();
    let mut all_records: Vec<Record> = Vec::new();
    let mut applied_filtered = 0usize;
    let mut unparseable_skipped = 0usize;

    for file_path in &json_files {
        match parse_proposal_file(file_path) {
            Ok(records) => {
                for rec in records {
                    if is_filtered(&rec, &applied_ids) {
                        applied_filtered += 1;
                    } else {
                        all_records.push(rec);
                    }
                }
            }
            Err(reason) => {
                let path_str = file_path.display();
                eprintln!("SKIP {path_str}: {reason}");
                unparseable_skipped += 1;
            }
        }
    }

    let mut all_clusters = cluster_records(all_records);
    sort_clusters(&mut all_clusters);
    let clusters_total = all_clusters.len();

    let output_clusters: Vec<Cluster> = all_clusters
        .into_iter()
        .filter(|c| c.recurrence >= min_recurrence)
        .collect();

    let output = Output {
        backlog: "hardening.v1".to_string(),
        generated_proposals_read: total_files,
        clusters: output_clusters,
        coverage: Coverage {
            applied_filtered,
            unparseable_skipped,
            clusters_total,
        },
    };

    match format {
        "human" => Ok(format_human(&output)),
        _ => serde_json::to_string_pretty(&output).map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests_unit {
    use super::*;

    #[test]
    fn test_jaccard_identical() {
        let a = tokenize("foo bar baz");
        let b = tokenize("foo bar baz");
        assert!((jaccard(&a, &b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_jaccard_disjoint() {
        let a = tokenize("foo bar");
        let b = tokenize("qux quux");
        assert_eq!(jaccard(&a, &b), 0.0);
    }

    #[test]
    fn test_jaccard_partial() {
        let a = tokenize("foo bar");
        let b = tokenize("foo baz");
        // intersection={foo} union={foo,bar,baz} = 1/3
        let score = jaccard(&a, &b);
        assert!((score - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_tokenize_punctuation() {
        let tokens = tokenize("hello, world! foo-bar");
        assert!(tokens.contains("hello"));
        assert!(tokens.contains("world"));
        assert!(tokens.contains("foo"));
        assert!(tokens.contains("bar"));
    }

    #[test]
    fn test_applied_log_sha() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "applied-suggestion:abc123").unwrap();
        let ids = load_applied_ids(f.path());
        assert!(ids.contains("abc123"));
    }

    #[test]
    fn test_applied_log_rejected() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "#REJECTED: my-slug").unwrap();
        let ids = load_applied_ids(f.path());
        assert!(ids.contains("my-slug"));
    }
}
