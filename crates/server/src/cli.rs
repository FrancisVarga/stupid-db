//! CLI argument parsing and subcommand dispatch.

use std::path::Path;

use crate::{export, import};

/// Parse CLI arguments and dispatch to the appropriate subcommand.
///
/// Returns `Ok(true)` if a subcommand was handled, `Ok(false)` if `serve`
/// should be started (handled by the caller).
pub async fn dispatch(config: &stupid_core::Config, args: &[String]) -> anyhow::Result<bool> {
    match args.get(1).map(|s| s.as_str()) {
        Some("import") => {
            let path = args
                .get(2)
                .expect("Usage: server import <parquet_path> <segment_id>");
            let segment_id = args
                .get(3)
                .expect("Usage: server import <parquet_path> <segment_id>");
            import::import(config, Path::new(path), segment_id)?;
            Ok(true)
        }
        Some("import-dir") => {
            let path = args
                .get(2)
                .expect("Usage: server import-dir <directory>");
            import::import_dir(config, Path::new(path))?;
            Ok(true)
        }
        Some("import-s3") => {
            let prefix = args
                .get(2)
                .expect("Usage: server import-s3 <s3-prefix>");
            export::import_s3(config, prefix).await?;
            Ok(true)
        }
        Some("export") => {
            let flag = args.get(2).map(|s| s.as_str()).unwrap_or("--all");
            let (do_segments, do_graph) = match flag {
                "--segments" => (true, false),
                "--graph" => (false, true),
                "--all" | _ => (true, true),
            };
            export::export(config, do_segments, do_graph).await?;
            Ok(true)
        }
        Some("serve") => Ok(false),
        _ => {
            print_usage();
            Ok(true)
        }
    }
}

/// Extract the optional segment ID and `--eisenbahn` flag from CLI args.
pub fn parse_serve_args(args: &[String]) -> (Option<String>, bool) {
    let eisenbahn = args.iter().any(|a| a == "--eisenbahn");
    let segment_id = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with("--"))
        .cloned();
    (segment_id, eisenbahn)
}

fn print_usage() {
    println!("stupid-db v0.1.0");
    println!("Usage: server.exe <command>");
    println!("  import <parquet_path> <segment_id>  Import single parquet file");
    println!("  import-dir <directory>               Import all parquet files recursively");
    println!("  import-s3 <s3-prefix>                Import parquet files from S3");
    println!("  export [--segments|--graph|--all]     Export to S3 (default: --all)");
    println!("  serve [segment_id] [--eisenbahn]     Start HTTP server (--eisenbahn enables ZMQ broker)");
}
