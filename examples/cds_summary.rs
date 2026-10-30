//! Print the headline terms of every FpML CDS document in a directory.
//!
//! cargo run --example cds_summary -- schemas/confirmation/examples/credit-derivatives

use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = env::args()
        .nth(1)
        .unwrap_or_else(|| "schemas/confirmation/examples/credit-derivatives".into());
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "xml"))
        .collect();
    paths.sort();
    let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
    println!(
        "{:<44} {:<30} {:<12} {:>11} {:<3} {:>7} {:<10} {:<10}",
        "file", "reference", "red/id", "notional", "ccy", "coupon", "effective", "maturity"
    );
    for path in paths {
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        match fpml::cds::parse(&fs::read_to_string(&path)?) {
            Ok(cds) => {
                let k = fpml::cds::key_terms(&cds);
                let mut reference = opt(&k.reference);
                reference.truncate(30);
                println!(
                    "{:<44} {:<30} {:<12} {:>11} {:<3} {:>7} {:<10} {:<10}",
                    name,
                    reference,
                    opt(&k.red),
                    k.notional.map_or("-".into(), |n| format!("{n:.0}")),
                    opt(&k.currency),
                    k.fixed_rate
                        .map_or("-".into(), |r| format!("{:.0}bp", r * 10_000.0)),
                    opt(&k.effective_date),
                    opt(&k.termination_date),
                );
            }
            Err(e) => println!("{name:<44} error: {e}"),
        }
    }
    Ok(())
}
