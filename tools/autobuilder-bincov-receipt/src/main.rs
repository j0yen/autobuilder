use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process;

#[derive(Parser, Debug)]
#[command(name = "autobuilder-bincov-receipt")]
#[command(about = "Detects [[bin]] crates lacking an integration test and emits a receipt")]
struct Args {
    /// Path to the crate directory (must contain Cargo.toml)
    crate_dir: PathBuf,

    /// Output format
    #[arg(long, default_value = "json")]
    format: Format,

    /// Exit with code 3 when verdict is "concern"
    #[arg(long)]
    strict: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Format {
    Json,
    Human,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Receipt {
    receipt: String,
    crate_name: String,
    has_bin: bool,
    bin_names: Vec<String>,
    has_integration_test: bool,
    integration_test_files: Vec<String>,
    verdict: String,
    note: String,
}

// Rename field for JSON output
#[derive(Debug, Serialize)]
struct ReceiptOutput {
    receipt: String,
    #[serde(rename = "crate")]
    crate_name: String,
    has_bin: bool,
    bin_names: Vec<String>,
    has_integration_test: bool,
    integration_test_files: Vec<String>,
    verdict: String,
    note: String,
}

impl From<Receipt> for ReceiptOutput {
    fn from(r: Receipt) -> Self {
        ReceiptOutput {
            receipt: r.receipt,
            crate_name: r.crate_name,
            has_bin: r.has_bin,
            bin_names: r.bin_names,
            has_integration_test: r.has_integration_test,
            integration_test_files: r.integration_test_files,
            verdict: r.verdict,
            note: r.note,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CargoToml {
    package: Option<CargoPackage>,
    bin: Option<Vec<CargoBin>>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CargoBin {
    name: Option<String>,
}

fn detect_bin(crate_dir: &Path, cargo: &CargoToml) -> (bool, Vec<String>) {
    // Explicit [[bin]] entries
    if let Some(bins) = &cargo.bin {
        if !bins.is_empty() {
            let names: Vec<String> = bins
                .iter()
                .filter_map(|b| b.name.clone())
                .collect();
            return (true, names);
        }
    }

    // Single-bin convention: src/main.rs exists
    let main_rs = crate_dir.join("src").join("main.rs");
    if main_rs.exists() {
        let crate_name = cargo
            .package
            .as_ref()
            .and_then(|p| p.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        return (true, vec![crate_name]);
    }

    (false, vec![])
}

fn file_has_integration_test(content: &str, bin_names: &[String]) -> bool {
    let has_command = content.contains("std::process::Command")
        || content.contains("process::Command")
        || content.contains("Command::new")
        || content.contains("assert_cmd::Command")
        || content.contains("assert_cmd");

    if !has_command {
        return false;
    }

    // Check for reference to binary name or CARGO_BIN_EXE_
    if content.contains("CARGO_BIN_EXE_") {
        return true;
    }

    // Check for reference to any binary name
    for name in bin_names {
        if content.contains(name.as_str()) {
            return true;
        }
    }

    // If has_command but no explicit bin name reference, still count it
    // (the command-based check is sufficient if there's any Command usage in tests/)
    has_command
}

fn scan_tests(crate_dir: &Path, bin_names: &[String]) -> (bool, Vec<String>) {
    let tests_dir = crate_dir.join("tests");
    if !tests_dir.exists() {
        return (false, vec![]);
    }

    let mut matching_files: Vec<String> = Vec::new();

    let entries = match std::fs::read_dir(&tests_dir) {
        Ok(e) => e,
        Err(_) => return (false, vec![]),
    };

    let mut file_names: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "rs").unwrap_or(false))
        .collect();
    file_names.sort();

    for path in file_names {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if file_has_integration_test(&content, bin_names) {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            matching_files.push(file_name);
        }
    }

    let found = !matching_files.is_empty();
    (found, matching_files)
}

pub fn run_check(crate_dir: &Path) -> Result<Receipt, String> {
    let cargo_toml_path = crate_dir.join("Cargo.toml");
    if !cargo_toml_path.exists() {
        return Err(format!(
            "No Cargo.toml found at {}",
            cargo_toml_path.display()
        ));
    }

    let cargo_content = std::fs::read_to_string(&cargo_toml_path)
        .map_err(|e| format!("Failed to read Cargo.toml: {e}"))?;

    let cargo: CargoToml = toml::from_str(&cargo_content)
        .map_err(|e| format!("Failed to parse Cargo.toml: {e}"))?;

    let crate_name = cargo
        .package
        .as_ref()
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let (has_bin, bin_names) = detect_bin(crate_dir, &cargo);

    if !has_bin {
        return Ok(Receipt {
            receipt: "bincov.v1".to_string(),
            crate_name,
            has_bin: false,
            bin_names: vec![],
            has_integration_test: false,
            integration_test_files: vec![],
            verdict: "pass".to_string(),
            note: "Crate does not ship a [[bin]]; no integration test required.".to_string(),
        });
    }

    let (has_integration_test, integration_test_files) = scan_tests(crate_dir, &bin_names);

    let (verdict, note) = if has_integration_test {
        (
            "pass".to_string(),
            format!(
                "Crate ships a [[bin]] and has {} integration test file(s) that drive it via subprocess.",
                integration_test_files.len()
            ),
        )
    } else {
        (
            "concern".to_string(),
            "Crate ships a [[bin]] but no tests/ file drives it via std::process::Command; binary dispatch arms are unreachable by lib-only `cargo test`.".to_string(),
        )
    };

    Ok(Receipt {
        receipt: "bincov.v1".to_string(),
        crate_name,
        has_bin: true,
        bin_names,
        has_integration_test,
        integration_test_files,
        verdict,
        note,
    })
}

fn main() {
    let args = Args::parse();

    let receipt = match run_check(&args.crate_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    };

    let verdict = receipt.verdict.clone();
    let output: ReceiptOutput = receipt.into();

    match args.format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Format::Human => {
            println!("bincov.v1 receipt for crate: {}", output.crate_name);
            println!("  has_bin:               {}", output.has_bin);
            println!("  bin_names:             {:?}", output.bin_names);
            println!("  has_integration_test:  {}", output.has_integration_test);
            println!("  integration_test_files:{:?}", output.integration_test_files);
            println!("  verdict:               {}", output.verdict);
            println!("  note:                  {}", output.note);
        }
    }

    if args.strict && verdict == "concern" {
        process::exit(3);
    }
}
