use std::env;
use std::fs;

use neurochain::actions::{parse_action_plan_from_nc, validate_plan, ActionPlan, Allowlist};

fn print_usage() {
    eprintln!("Usage: neurochain-soroban <file.nc|plan.json> [--flow] [--yes]");
    eprintln!("If input is JSON, it is treated as an ActionPlan.");
    eprintln!(
        "Manual .nc lines can start with 'stellar.' or 'soroban.' (optionally prefixed by '#')."
    );
    eprintln!("Set NC_ALLOWLIST_ENFORCE=1 to hard-fail on allowlist violations.");
    eprintln!("--flow enables simulate → preview → confirm → submit (stub).");
    eprintln!("--yes auto-confirms submit in --flow mode.");
}

fn allowlist_enforced() -> bool {
    matches!(
        std::env::var("NC_ALLOWLIST_ENFORCE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[derive(Debug)]
struct Preview {
    fee_estimate: String,
    effects: Vec<String>,
}

fn simulate_stub(plan: &ActionPlan) -> Preview {
    let fee = plan.actions.len().saturating_mul(100);
    let effects = plan
        .actions
        .iter()
        .map(|action| format!("action: {}", action.kind()))
        .collect();
    Preview {
        fee_estimate: format!("{fee} stroops (stub)"),
        effects,
    }
}

fn print_preview(preview: &Preview) {
    eprintln!("=== Preview (stub) ===");
    eprintln!("Estimated fee: {}", preview.fee_estimate);
    if preview.effects.is_empty() {
        eprintln!("Effects: (none)");
    } else {
        eprintln!("Effects:");
        for effect in &preview.effects {
            eprintln!("  - {effect}");
        }
    }
}

fn confirm_submit(auto_yes: bool) -> bool {
    if auto_yes {
        return true;
    }
    eprint!("Confirm submit? [y/N]: ");
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn submit_stub() -> String {
    "STUB_TX_HASH".to_string()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(2);
    }

    let mut path: Option<String> = None;
    let mut flow = false;
    let mut auto_yes = false;
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--flow" => flow = true,
            "--yes" | "-y" => auto_yes = true,
            _ => {
                if path.is_none() {
                    path = Some(arg.clone());
                }
            }
        }
    }

    let Some(path) = path else {
        print_usage();
        std::process::exit(2);
    };

    let input = match fs::read_to_string(path.clone()) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!("Error reading {path}: {err}");
            std::process::exit(1);
        }
    };

    let mut plan: ActionPlan = match serde_json::from_str(&input) {
        Ok(plan) => plan,
        Err(_) => parse_action_plan_from_nc(&input),
    };
    if plan.source.is_none() {
        plan.source = Some(path.to_string());
    }

    let allowlist = Allowlist::from_env();
    let violations = validate_plan(&plan, &allowlist);
    if !violations.is_empty() {
        for violation in &violations {
            plan.warnings.push(format!(
                "allowlist warning: #{} {} ({})",
                violation.index, violation.action, violation.reason
            ));
        }
        if allowlist_enforced() {
            eprintln!("Allowlist violations (enforced):");
            for violation in &violations {
                eprintln!(
                    "- #{} {}: {}",
                    violation.index, violation.action, violation.reason
                );
            }
            eprintln!("Set NC_ALLOWLIST_ENFORCE=0 (or unset) to allow warnings only.");
            std::process::exit(3);
        }
        eprintln!("Allowlist warnings (stub, not enforced):");
        for violation in &violations {
            eprintln!(
                "- #{} {}: {}",
                violation.index, violation.action, violation.reason
            );
        }
    }

    match serde_json::to_string_pretty(&plan) {
        Ok(json) => println!("{json}"),
        Err(err) => {
            eprintln!("Error serializing action plan: {err}");
            std::process::exit(1);
        }
    }

    if flow {
        let preview = simulate_stub(&plan);
        print_preview(&preview);
        if confirm_submit(auto_yes) {
            let tx_hash = submit_stub();
            eprintln!("Submit (stub) OK: {tx_hash}");
        } else {
            eprintln!("Submit aborted by user.");
        }
    }
}
