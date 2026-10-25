use std::path::PathBuf;
use clap::Parser;

#[derive(Parser)]
struct Args {
    /// JSONL golden set
    #[arg(long)]
    golden: PathBuf,
    /// Project directory (for loading the index)
    #[arg(long)]
    project_dir: PathBuf,
    /// Directory for JSON output
    #[arg(long, default_value = "eval_runs")]
    out: PathBuf,
    /// Score below which a top hit counts as "no evidence" for negatives
    #[arg(long, default_value_t = 0.0)]
    neg_threshold: f32,
    /// Which retrievers to run (bm25, dense, hybrid). Repeatable.
    #[arg(long, value_delimiter = ',', default_value = "bm25")]
    retrievers: Vec<String>,
    /// Skip golden set validation against index
    #[arg(long)]
    skip_validate: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Validate golden set against index unless skipped
    if !args.skip_validate {
        localaise_lib::eval::validate::validate_or_panic(&args.golden, &args.project_dir);
        println!("golden set validation passed");
    }

    let golden = localaise_lib::eval::harness::load_golden(&args.golden)?;
    println!("loaded {} golden items", golden.len());

    let mut reports = Vec::new();
    let mut runs = Vec::new();

    for name in &args.retrievers {
        // Wire these up as the vertical slice lands. Unknown names fail loudly.
        let retriever = localaise_lib::eval::build_retriever(name, &args.project_dir)?;
        let out = localaise_lib::eval::harness::run(retriever.as_ref(), &golden, args.neg_threshold).await?;
        reports.push(out.report.clone());
        runs.push(out);
    }

    localaise_lib::eval::harness::print_table(&reports);

    std::fs::create_dir_all(&args.out)?;
    let stamp = chrono_stamp();
    let path = args.out.join(format!("run-{stamp}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&runs.iter().map(|r| &r.report).collect::<Vec<_>>())?)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn chrono_stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}