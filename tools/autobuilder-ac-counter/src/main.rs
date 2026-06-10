use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum)]
enum Format {
    Json,
    Human,
}

#[derive(Parser, Debug)]
#[command(name = "autobuilder-ac-counter")]
#[command(about = "Count acceptance criteria across all test layouts in a Rust crate")]
struct Args {
    /// Path to the crate directory (must contain Cargo.toml)
    crate_dir: PathBuf,

    /// Output format
    #[arg(long, value_enum, default_value = "human")]
    format: Format,
}

fn main() {
    let args = Args::parse();

    let inventory = match autobuilder_ac_counter::discover(&args.crate_dir) {
        Ok(inv) => inv,
        Err(e) => {
            eprintln!("Error discovering ACs: {e}");
            std::process::exit(1);
        }
    };

    match args.format {
        Format::Json => {
            let json = serde_json::to_string_pretty(&inventory).expect("serialization failed");
            println!("{json}");
        }
        Format::Human => {
            println!("Total ACs: {}", inventory.total);
            println!("  split_file:      {}", inventory.by_layout.split_file);
            println!("  monolithic_fns:  {}", inventory.by_layout.monolithic_fns);
            println!("  mock_files:      {}", inventory.by_layout.mock_files);
            if !inventory.names.is_empty() {
                println!("Names:");
                for name in &inventory.names {
                    println!("  - {name}");
                }
            }
        }
    }
}
