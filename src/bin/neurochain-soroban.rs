use std::env;
use std::fs;

use neurochain::actions::{parse_action_plan_from_nc, validate_plan, ActionPlan, Allowlist};

fn print_usage() {
    eprintln!("Usage: neurochain-soroban <file.nc|plan.json>");
    eprintln!("If input is JSON, it is treated as an ActionPlan.");
    eprintln!("Manual .nc lines can start with 'stellar.' or 'soroban.' (optionally prefixed by '#').");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(2);
    }

    let path = &args[1];
    let input = match fs::read_to_string(path) {
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
        eprintln!("Allowlist warnings (stub, not enforced):");
        for violation in &violations {
            eprintln!("- #{} {}: {}", violation.index, violation.action, violation.reason);
        }
    }

    match serde_json::to_string_pretty(&plan) {
        Ok(json) => println!("{json}"),
        Err(err) => {
            eprintln!("Error serializing action plan: {err}");
            std::process::exit(1);
        }
    }
}
