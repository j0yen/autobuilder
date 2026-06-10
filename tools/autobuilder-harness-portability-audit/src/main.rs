use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser, Debug)]
#[command(
    name = "autobuilder-harness-portability-audit",
    about = "Scans shell scripts for Linux-only idioms and reports macOS-equivalent suggestions (draft-only, never edits)"
)]
struct Args {
    /// Directory containing shell scripts to scan
    scripts_dir: PathBuf,

    /// Output format
    #[arg(long, default_value = "json")]
    format: Format,

    /// Exit with code 4 if any unguarded findings exist
    #[arg(long)]
    strict: bool,
}

#[derive(Clone, Debug, ValueEnum)]
enum Format {
    Json,
    Human,
}

#[derive(Debug, Serialize, Deserialize)]
struct Finding {
    rule: String,
    file: String,
    line: usize,
    text: String,
    already_guarded: bool,
    suggestion: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Summary {
    files_scanned: usize,
    findings: usize,
    unguarded: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct Report {
    report: String,
    scripts_dir: String,
    findings: Vec<Finding>,
    summary: Summary,
}

struct Rule {
    id: &'static str,
    suggestion: &'static str,
}

const RULES: &[Rule] = &[
    Rule {
        id: "nproc",
        suggestion: "nproc 2>/dev/null || sysctl -n hw.logicalcpu || echo 4",
    },
    Rule {
        id: "proc-fs",
        suggestion: "use ps/sysctl; /proc is absent on macOS",
    },
    Rule {
        id: "flock",
        suggestion: "mkdir-based lock (macOS has no flock(1))",
    },
    Rule {
        id: "gnu-date",
        suggestion: "BSD date -j -f, or pass epoch ms in",
    },
    Rule {
        id: "readlink-f",
        suggestion: "python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' \"$path\" or cd ... && pwd -P",
    },
    Rule {
        id: "sed-i-empty",
        suggestion: "BSD sed requires sed -i '' (empty string backup suffix)",
    },
    Rule {
        id: "stat-c",
        suggestion: "BSD stat -f (use -f instead of -c for format string)",
    },
];

fn matches_rule(rule_id: &str, line: &str) -> bool {
    match rule_id {
        "nproc" => {
            // Match bare `nproc` - word boundary check
            // Must contain "nproc" as a standalone word (not part of a larger word)
            contains_word(line, "nproc")
        }
        "proc-fs" => line.contains("/proc/"),
        "flock" => contains_flock(line),
        "gnu-date" => line.contains("date -d ") || line.contains("date --date"),
        "readlink-f" => line.contains("readlink -f"),
        "sed-i-empty" => is_sed_i_without_suffix(line),
        "stat-c" => line.contains("stat -c"),
        _ => false,
    }
}

/// Check if `nproc` appears as a word (not part of a larger identifier)
fn contains_word(line: &str, word: &str) -> bool {
    let bytes = line.as_bytes();
    let wbytes = word.as_bytes();
    let wlen = wbytes.len();

    if line.len() < wlen {
        return false;
    }

    let mut start = 0;
    while start + wlen <= bytes.len() {
        if let Some(pos) = line[start..].find(word) {
            let abs_pos = start + pos;
            let before_ok = abs_pos == 0 || !bytes[abs_pos - 1].is_ascii_alphanumeric() && bytes[abs_pos - 1] != b'_' && bytes[abs_pos - 1] != b'-';
            let after_pos = abs_pos + wlen;
            let after_ok = after_pos >= bytes.len() || !bytes[after_pos].is_ascii_alphanumeric() && bytes[after_pos] != b'_' && bytes[after_pos] != b'-';
            if before_ok && after_ok {
                return true;
            }
            start = abs_pos + 1;
        } else {
            break;
        }
    }
    false
}

/// Match `flock ` as a command invocation (not as part of a variable name or comment about "flock")
fn contains_flock(line: &str) -> bool {
    // Match `flock ` followed by at least one character, as a command
    // Avoid matching lines that are just comments explaining flock
    let trimmed = line.trim_start();
    // Skip pure comment lines that just mention flock
    if trimmed.starts_with('#') {
        // Still flag if the comment shows a usage pattern like `# flock <fd>`
        // but per the rule, we just check for `flock ` anywhere
        return line.contains("flock ");
    }
    line.contains("flock ")
}

/// sed -i with no backup suffix: `sed -i ` followed by something other than a quote
/// BSD sed requires `sed -i ''` — so flag `sed -i ` where the next char is not a quote
fn is_sed_i_without_suffix(line: &str) -> bool {
    // Look for `sed -i ` pattern
    // It should NOT be followed by '' or "" (which would be the BSD-safe form)
    let mut search_start = 0;
    while let Some(pos) = line[search_start..].find("sed -i ") {
        let abs_pos = search_start + pos;
        let after = &line[abs_pos + "sed -i ".len()..];
        let after_trimmed = after.trim_start();
        // If next non-space is a quote, it likely has a suffix already
        if after_trimmed.starts_with('\'') || after_trimmed.starts_with('"') {
            // Could still be `sed -i '' ...` which is safe, or `sed -i 'pattern'` which is not
            // A backup suffix would be '' (empty) or a short string before the pattern
            // We consider it safe only if it starts with '' or ""
            if after_trimmed.starts_with("''") || after_trimmed.starts_with("\"\"") {
                search_start = abs_pos + 1;
                continue;
            }
            // Otherwise it's `sed -i 'pattern'` without a backup suffix — flag it
            return true;
        }
        // Next char is not a quote — flag it
        return true;
    }
    false
}

fn is_already_guarded(rule_id: &str, line: &str) -> bool {
    match rule_id {
        "nproc" => {
            // Guarded if the same line contains a fallback
            line.contains("sysctl -n hw.logicalcpu")
        }
        _ => false,
    }
}

fn scan_file(path: &Path, scripts_dir: &str) -> Vec<Finding> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let file_name = path
        .strip_prefix(scripts_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();

    if file_name.is_empty() {
        return vec![];
    }

    let mut findings = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line_num = line_idx + 1; // 1-based

        for rule in RULES {
            if matches_rule(rule.id, line) {
                let already_guarded = is_already_guarded(rule.id, line);
                findings.push(Finding {
                    rule: rule.id.to_string(),
                    file: file_name.clone(),
                    line: line_num,
                    text: line.trim().to_string(),
                    already_guarded,
                    suggestion: rule.suggestion.to_string(),
                });
            }
        }
    }

    findings
}

fn collect_sh_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "sh" {
                        files.push(path);
                    }
                } else {
                    // Also include files without extension but check if shell script
                    // For now, only .sh files per the spec
                }
            } else if path.is_dir() {
                let mut sub = collect_sh_files(&path);
                files.append(&mut sub);
            }
        }
    }
    files
}

fn main() {
    let args = Args::parse();

    let scripts_dir_str = args.scripts_dir.to_string_lossy().to_string();

    if !args.scripts_dir.exists() {
        eprintln!("error: scripts-dir '{scripts_dir_str}' does not exist");
        process::exit(1);
    }

    if !args.scripts_dir.is_dir() {
        eprintln!("error: scripts-dir '{scripts_dir_str}' is not a directory");
        process::exit(1);
    }

    let mut sh_files = collect_sh_files(&args.scripts_dir);
    // Sort files for deterministic output
    sh_files.sort();

    let files_scanned = sh_files.len();
    let mut all_findings: Vec<Finding> = Vec::new();

    for file in &sh_files {
        let mut file_findings = scan_file(file, &scripts_dir_str);
        all_findings.append(&mut file_findings);
    }

    // Sort findings by (file, line) for determinism
    all_findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    let total_findings = all_findings.len();
    let unguarded = all_findings.iter().filter(|f| !f.already_guarded).count();

    let report = Report {
        report: "portability.v1".to_string(),
        scripts_dir: scripts_dir_str,
        findings: all_findings,
        summary: Summary {
            files_scanned,
            findings: total_findings,
            unguarded,
        },
    };

    match args.format {
        Format::Json => {
            let json = serde_json::to_string_pretty(&report).expect("failed to serialize report");
            println!("{json}");
        }
        Format::Human => {
            println!("Portability Audit: {}", report.scripts_dir);
            println!("Files scanned: {}", report.summary.files_scanned);
            println!(
                "Findings: {} ({} unguarded)",
                report.summary.findings, report.summary.unguarded
            );
            if report.findings.is_empty() {
                println!("  No issues found.");
            } else {
                for f in &report.findings {
                    let guard_str = if f.already_guarded {
                        "[guarded]"
                    } else {
                        "[UNGUARDED]"
                    };
                    println!(
                        "  {} {}:{}  rule={}\n    text: {}\n    suggestion: {}",
                        guard_str, f.file, f.line, f.rule, f.text, f.suggestion
                    );
                }
            }
        }
    }

    if args.strict && unguarded > 0 {
        process::exit(4);
    }
}
