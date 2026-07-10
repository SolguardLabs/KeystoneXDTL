use crate::{KeystoneError, KeystoneResult};

pub fn run() -> KeystoneResult<()> {
    let scenario = std::env::args().nth(1).unwrap_or_else(|| "loan".to_owned());
    if scenario == "--list" || scenario == "list" {
        println!(
            "loan\nrepayment\nprepayment\ndefault\nliquidation\nredistribution\nportfolio\nsnapshot"
        );
        return Ok(());
    }
    let report = crate::scenario::run_named(&scenario)?;
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| KeystoneError::serialization(error.to_string()))?;
    println!("{json}");
    Ok(())
}
