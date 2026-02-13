use std::path::Path;
use anyhow::Result;
use synapseed_core::state::ProjectState;

pub async fn run_diagnose(path: &Path) -> Result<()> {
    let state = ProjectState::detect(path);
    println!("=== SYNAPSEED DIAGNOSTIC ===\n");
    println!("{}", state.diagnostic());
    Ok(())
}

pub fn run_check(command: &str) -> Result<()> {
    let sentinel = synapseed_root::sentinel::Sentinel::with_defaults()
        .map_err(|e| anyhow::anyhow!("Failed to load security policy: {e}"))?;

    match sentinel.evaluate(command) {
        Ok(action) => {
            println!("Command: \"{}\"", command);
            println!("Action:  {:?}", action);
            if matches!(action, synapseed_core::policy::PolicyAction::Deny) {
                println!("\n[REJECTED] Policy violation detected.");
            } else {
                println!("\n[ALLOWED] Command satisfies security policy.");
            }
        }
        Err(e) => {
            println!("Command: \"{}\"", command);
            println!("Error:   {}", e);
            println!("\n[FAILED] System error during evaluation.");
        }
    }
    Ok(())
}
