use neurochain::actions::Action;
use neurochain::intent_stellar::{
    build_action_plan, has_intent_blocking_issue, IntentDecision, IntentStellarLabel,
};
use neurochain::soroban_deep::{
    apply_contract_intent_templates, apply_policy_typed_templates_v2, ContractPolicy,
};

fn decision(label: IntentStellarLabel) -> IntentDecision {
    IntentDecision {
        label,
        score: 0.95,
        threshold: 0.55,
        downgraded_to_unknown: false,
    }
}

fn assert_no_intent_error(plan: &neurochain::actions::ActionPlan) {
    assert!(
        !plan.warnings.iter().any(|w| w.starts_with("intent_error:")),
        "unexpected intent_error warnings: {:?}",
        plan.warnings
    );
}

#[test]
fn intent_stellar_template_mapping_happy_paths() {
    let g1 = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let g2 = "GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ";
    let c1 = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let hash = "f3eb378466903fc8eb132f67bc33519bb1233f5f78df4d9f0f6998a1445e5f15";

    let plan = build_action_plan(
        &format!("Check balance for {g1} asset XLM"),
        &decision(IntentStellarLabel::BalanceQuery),
    );
    assert_eq!(plan.actions.len(), 1);
    match &plan.actions[0] {
        Action::StellarAccountBalance { account, asset } => {
            assert_eq!(account, g1);
            assert_eq!(asset.as_deref(), Some("XLM"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Create account {g2} with starting balance 2"),
        &decision(IntentStellarLabel::CreateAccount),
    );
    match &plan.actions[0] {
        Action::StellarAccountCreate {
            destination,
            starting_balance,
        } => {
            assert_eq!(destination, g2);
            assert_eq!(starting_balance, "2");
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Add trustline TESTUSD:{g1} limit 1000"),
        &decision(IntentStellarLabel::ChangeTrust),
    );
    match &plan.actions[0] {
        Action::StellarChangeTrust {
            asset_code,
            asset_issuer,
            limit,
        } => {
            assert_eq!(asset_code, "TESTUSD");
            assert_eq!(asset_issuer, g1);
            assert_eq!(limit.as_deref(), Some("1000"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Send 5 XLM to {g2}"),
        &decision(IntentStellarLabel::TransferXLM),
    );
    match &plan.actions[0] {
        Action::StellarPayment {
            to,
            amount,
            asset_code,
            asset_issuer,
        } => {
            assert_eq!(to, g2);
            assert_eq!(amount, "5");
            assert_eq!(asset_code, "XLM");
            assert!(asset_issuer.is_none());
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Send 12.5 TESTUSD:{g1} to {g2}"),
        &decision(IntentStellarLabel::TransferAsset),
    );
    match &plan.actions[0] {
        Action::StellarPayment {
            to,
            amount,
            asset_code,
            asset_issuer,
        } => {
            assert_eq!(to, g2);
            assert_eq!(amount, "12.5");
            assert_eq!(asset_code, "TESTUSD");
            assert_eq!(asset_issuer.as_deref(), Some(g1));
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Fund testnet account {g1}"),
        &decision(IntentStellarLabel::FundTestnet),
    );
    match &plan.actions[0] {
        Action::StellarAccountFundTestnet { account } => assert_eq!(account, g1),
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Check tx status {hash}"),
        &decision(IntentStellarLabel::TxStatus),
    );
    match &plan.actions[0] {
        Action::StellarTxStatus { hash: got_hash } => assert_eq!(got_hash, hash),
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        &format!("Invoke contract {c1} function transfer args={{\"to\":\"{g2}\",\"amount\":5}}"),
        &decision(IntentStellarLabel::ContractInvoke),
    );
    match &plan.actions[0] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(contract_id, c1);
            assert_eq!(function, "transfer");
            assert_eq!(args["to"].as_str(), Some(g2));
            assert_eq!(args["amount"].as_i64(), Some(5));
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        "Deploy contract alias hello-demo wasm ./contracts/hello.wasm",
        &decision(IntentStellarLabel::Unknown),
    );
    match &plan.actions[0] {
        Action::SorobanContractDeploy { alias, wasm } => {
            assert_eq!(alias, "hello-demo");
            assert_eq!(wasm, "./contracts/hello.wasm");
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);

    let plan = build_action_plan(
        "Deploy contract alias hello-demo wasm .\\contracts\\hello.wasm",
        &decision(IntentStellarLabel::Unknown),
    );
    match &plan.actions[0] {
        Action::SorobanContractDeploy { alias, wasm } => {
            assert_eq!(alias, "hello-demo");
            assert_eq!(wasm, ".\\contracts\\hello.wasm");
        }
        other => panic!("unexpected action: {other:?}"),
    }
    assert!(!has_intent_blocking_issue(&plan));
    assert_no_intent_error(&plan);
}

#[test]
fn intent_stellar_slot_missing_is_blocking_unknown() {
    let g2 = "GCBYKY5GGH4GYUE5AXAUGUS4VUQAQAEO5YMSEJSD2OLC2WBAXEXAJGZQ";
    let c1 = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";

    let cases = [
        (IntentStellarLabel::TransferXLM, format!("Send XLM to {g2}")),
        (
            IntentStellarLabel::ChangeTrust,
            "Add trustline USDC limit 100".to_string(),
        ),
        (
            IntentStellarLabel::CreateAccount,
            "Create account with starting balance 2".to_string(),
        ),
        (
            IntentStellarLabel::TxStatus,
            "Show latest tx status".to_string(),
        ),
        (
            IntentStellarLabel::ContractInvoke,
            format!("Invoke contract {c1} args={{\"to\":\"World\"}}"),
        ),
    ];

    for (label, prompt) in cases {
        let plan = build_action_plan(&prompt, &decision(label));
        assert_eq!(plan.actions.len(), 1);
        assert!(has_intent_blocking_issue(&plan));
        match &plan.actions[0] {
            Action::Unknown { reason } => {
                assert!(
                    reason.starts_with("slot_missing:"),
                    "expected slot_missing reason, got: {reason}"
                );
            }
            other => panic!("expected Unknown action, got: {other:?}"),
        }
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.starts_with("intent_error: slot_missing:")),
            "missing slot_missing warning: {:?}",
            plan.warnings
        );
    }
}

#[test]
fn intent_stellar_contract_invoke_typed_validation_blocks_type_errors() {
    let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let g1 = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";

    let ok_prompt = format!(
        "Invoke contract {contract} function transfer args={{\"to\":\"{g1}\",\"blob\":\"0x0A0B\",\"ticker\":\"USDC\",\"amount\":100}} arg_types={{\"to\":\"address\",\"blob\":\"bytes\",\"ticker\":\"symbol\",\"amount\":\"u64\"}}"
    );
    let ok_plan = build_action_plan(&ok_prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert_eq!(ok_plan.actions.len(), 1);
    assert_eq!(ok_plan.actions[0].kind(), "soroban.contract.invoke");
    assert!(!has_intent_blocking_issue(&ok_plan));
    assert!(ok_plan
        .warnings
        .iter()
        .all(|w| !w.starts_with("intent_error: slot_type_error")));

    let bad_prompt = format!(
        "Invoke contract {contract} function transfer args={{\"to\":\"World\",\"amount\":-1}} arg_types={{\"to\":\"address\",\"amount\":\"u64\"}}"
    );
    let bad_plan = build_action_plan(&bad_prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&bad_plan));
    match &bad_plan.actions[0] {
        Action::Unknown { reason } => {
            assert!(
                reason.starts_with("slot_type_error:"),
                "expected slot_type_error reason, got: {reason}"
            );
        }
        other => panic!("expected Unknown action, got: {other:?}"),
    }
    assert!(bad_plan
        .warnings
        .iter()
        .any(|w| w.starts_with("intent_error: slot_type_error:")));
}

#[test]
fn soroban_deep_template_expands_high_level_contract_intent() {
    let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["hello"],
  "args_schema": {{
    "hello": {{
      "required": {{
        "to": "symbol"
      }},
      "optional": {{}}
    }}
  }},
  "intent_templates": {{
    "hello": {{
      "aliases": ["say hello", "greet"],
      "function": "hello",
      "args": {{
        "to": {{
          "source": "after_to",
          "type": "symbol",
          "default": "World"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = "Please say hello to World";
    let mut plan = build_action_plan(prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&plan));

    let report = apply_contract_intent_templates(prompt, &mut plan, &[policy]);
    assert!(report.expanded, "template should expand: {report:?}");
    assert!(!has_intent_blocking_issue(&plan));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("soroban_deep_template: template=hello")));
    assert!(plan
        .warnings
        .iter()
        .all(|w| !w.contains("ContractInvoke missing contract_id")));

    match &plan.actions[0] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(contract_id, contract);
            assert_eq!(function, "hello");
            assert_eq!(args["to"].as_str(), Some("World"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn soroban_deep_template_expands_claim_rewards_use_case() {
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let contract = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["claim_rewards"],
  "args_schema": {{
    "claim_rewards": {{
      "required": {{
        "account": "address"
      }},
      "optional": {{
        "pool": "symbol"
      }}
    }}
  }},
  "intent_templates": {{
    "claim_rewards": {{
      "aliases": ["claim rewards", "collect rewards", "claim yield"],
      "function": "claim_rewards",
      "args": {{
        "account": {{
          "source": "first_account",
          "type": "address"
        }},
        "pool": {{
          "value": "rewards",
          "type": "symbol"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = format!("Invoke contract rewards function claim_rewards for wallet {account}");
    let mut plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&plan));

    let report = apply_contract_intent_templates(&prompt, &mut plan, std::slice::from_ref(&policy));
    assert!(report.expanded, "template should expand: {report:?}");
    assert_eq!(report.template_name.as_deref(), Some("claim_rewards"));

    let typed_report = apply_policy_typed_templates_v2(&mut plan, std::slice::from_ref(&policy));
    assert_eq!(typed_report.converted, 0);
    assert!(!has_intent_blocking_issue(&plan));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("soroban_deep_template: template=claim_rewards")));

    match &plan.actions[0] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(contract_id, contract);
            assert_eq!(function, "claim_rewards");
            assert_eq!(args["account"].as_str(), Some(account));
            assert_eq!(args["pool"].as_str(), Some("rewards"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn soroban_deep_template_keeps_claim_rewards_missing_account_blocking() {
    let contract = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["claim_rewards"],
  "args_schema": {{
    "claim_rewards": {{
      "required": {{
        "account": "address"
      }},
      "optional": {{}}
    }}
  }},
  "intent_templates": {{
    "claim_rewards": {{
      "aliases": ["claim rewards"],
      "function": "claim_rewards",
      "args": {{
        "account": {{
          "source": "first_account",
          "type": "address"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = "Claim rewards now";
    let mut plan = build_action_plan(prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&plan));

    let report = apply_contract_intent_templates(prompt, &mut plan, std::slice::from_ref(&policy));
    assert!(!report.expanded);
    assert_eq!(
        report.reason.as_deref(),
        Some("slot_missing: ContractInvoke template claim_rewards missing arg account")
    );
    assert!(has_intent_blocking_issue(&plan));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w
            .contains("slot_missing: ContractInvoke template claim_rewards missing arg account")));
}

#[test]
fn soroban_deep_template_expands_deposit_use_case() {
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let contract = "CFLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["deposit"],
  "args_schema": {{
    "deposit": {{
      "required": {{
        "account": "address",
        "amount": "u64",
        "asset": "symbol"
      }},
      "optional": {{}}
    }}
  }},
  "intent_templates": {{
    "deposit": {{
      "aliases": ["deposit", "vault deposit", "add liquidity"],
      "function": "deposit",
      "args": {{
        "account": {{
          "source": "first_account",
          "type": "address"
        }},
        "amount": {{
          "source": "first_number",
          "type": "u64"
        }},
        "asset": {{
          "value": "USDC",
          "type": "symbol"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = format!("Invoke contract deposit function deposit 100 for wallet {account}");
    let mut plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&plan));

    let report = apply_contract_intent_templates(&prompt, &mut plan, std::slice::from_ref(&policy));
    assert!(report.expanded, "template should expand: {report:?}");
    assert_eq!(report.template_name.as_deref(), Some("deposit"));

    let typed_report = apply_policy_typed_templates_v2(&mut plan, std::slice::from_ref(&policy));
    assert_eq!(typed_report.converted, 0);
    assert_eq!(typed_report.normalized_args, 1);
    assert!(!has_intent_blocking_issue(&plan));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("soroban_deep_template: template=deposit")));

    match &plan.actions[0] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(contract_id, contract);
            assert_eq!(function, "deposit");
            assert_eq!(args["account"].as_str(), Some(account));
            assert_eq!(args["amount"].as_u64(), Some(100));
            assert_eq!(args["asset"].as_str(), Some("USDC"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn soroban_deep_template_keeps_deposit_missing_amount_blocking() {
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let contract = "CFLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["deposit"],
  "args_schema": {{
    "deposit": {{
      "required": {{
        "account": "address",
        "amount": "u64",
        "asset": "symbol"
      }},
      "optional": {{}}
    }}
  }},
  "intent_templates": {{
    "deposit": {{
      "aliases": ["deposit"],
      "function": "deposit",
      "args": {{
        "account": {{
          "source": "first_account",
          "type": "address"
        }},
        "amount": {{
          "source": "first_number",
          "type": "u64"
        }},
        "asset": {{
          "value": "USDC",
          "type": "symbol"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = format!("Invoke contract deposit function deposit for wallet {account}");
    let mut plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&plan));

    let report = apply_contract_intent_templates(&prompt, &mut plan, std::slice::from_ref(&policy));
    assert!(!report.expanded);
    assert_eq!(
        report.reason.as_deref(),
        Some("slot_missing: ContractInvoke template deposit missing arg amount")
    );
    assert!(has_intent_blocking_issue(&plan));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("slot_missing: ContractInvoke template deposit missing arg amount")));
}

#[test]
fn soroban_deep_template_expands_swap_use_case() {
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let contract = "CGLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["swap"],
  "args_schema": {{
    "swap": {{
      "required": {{
        "account": "address",
        "amount": "u64",
        "from_asset": "symbol",
        "to_asset": "symbol",
        "min_out": "u64"
      }},
      "optional": {{}}
    }}
  }},
  "intent_templates": {{
    "swap": {{
      "aliases": ["swap", "trade"],
      "function": "swap",
      "args": {{
        "account": {{
          "source": "first_account",
          "type": "address"
        }},
        "amount": {{
          "source": "after_amount",
          "type": "u64"
        }},
        "from_asset": {{
          "source": "after_from",
          "type": "symbol"
        }},
        "to_asset": {{
          "source": "after_to",
          "type": "symbol"
        }},
        "min_out": {{
          "source": "after_min_out",
          "type": "u64"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = format!(
        "Invoke contract swap function swap amount 100 from USDC to XLM min_out 95 for wallet {account}"
    );
    let mut plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&plan));

    let report = apply_contract_intent_templates(&prompt, &mut plan, std::slice::from_ref(&policy));
    assert!(report.expanded, "template should expand: {report:?}");
    assert_eq!(report.template_name.as_deref(), Some("swap"));

    let typed_report = apply_policy_typed_templates_v2(&mut plan, std::slice::from_ref(&policy));
    assert_eq!(typed_report.converted, 0);
    assert_eq!(typed_report.normalized_args, 2);
    assert!(!has_intent_blocking_issue(&plan));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("soroban_deep_template: template=swap")));

    match &plan.actions[0] {
        Action::SorobanContractInvoke {
            contract_id,
            function,
            args,
        } => {
            assert_eq!(contract_id, contract);
            assert_eq!(function, "swap");
            assert_eq!(args["account"].as_str(), Some(account));
            assert_eq!(args["amount"].as_u64(), Some(100));
            assert_eq!(args["from_asset"].as_str(), Some("USDC"));
            assert_eq!(args["to_asset"].as_str(), Some("XLM"));
            assert_eq!(args["min_out"].as_u64(), Some(95));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn soroban_deep_template_keeps_swap_missing_min_out_blocking() {
    let account = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
    let contract = "CGLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["swap"],
  "args_schema": {{
    "swap": {{
      "required": {{
        "account": "address",
        "amount": "u64",
        "from_asset": "symbol",
        "to_asset": "symbol",
        "min_out": "u64"
      }},
      "optional": {{}}
    }}
  }},
  "intent_templates": {{
    "swap": {{
      "aliases": ["swap"],
      "function": "swap",
      "args": {{
        "account": {{
          "source": "first_account",
          "type": "address"
        }},
        "amount": {{
          "source": "after_amount",
          "type": "u64"
        }},
        "from_asset": {{
          "source": "after_from",
          "type": "symbol"
        }},
        "to_asset": {{
          "source": "after_to",
          "type": "symbol"
        }},
        "min_out": {{
          "source": "after_min_out",
          "type": "u64"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = format!(
        "Invoke contract swap function swap amount 100 from USDC to XLM for wallet {account}"
    );
    let mut plan = build_action_plan(&prompt, &decision(IntentStellarLabel::ContractInvoke));
    assert!(has_intent_blocking_issue(&plan));

    let report = apply_contract_intent_templates(&prompt, &mut plan, std::slice::from_ref(&policy));
    assert!(!report.expanded);
    assert_eq!(
        report.reason.as_deref(),
        Some("slot_missing: ContractInvoke template swap missing arg min_out")
    );
    assert!(has_intent_blocking_issue(&plan));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("slot_missing: ContractInvoke template swap missing arg min_out")));
}

#[test]
fn soroban_deep_template_can_expand_policy_alias_from_unknown_label() {
    let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["hello"],
  "args_schema": {{
    "hello": {{
      "required": {{
        "to": "symbol"
      }},
      "optional": {{}}
    }}
  }},
  "intent_templates": {{
    "hello": {{
      "aliases": ["say hello"],
      "function": "hello",
      "args": {{
        "to": {{
          "source": "after_to",
          "type": "symbol"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = "Please say hello to World";
    let mut plan = build_action_plan(prompt, &decision(IntentStellarLabel::Unknown));
    assert!(has_intent_blocking_issue(&plan));

    let report = apply_contract_intent_templates(prompt, &mut plan, &[policy]);
    assert!(report.expanded, "template should expand: {report:?}");
    assert!(!has_intent_blocking_issue(&plan));
    assert_eq!(plan.actions[0].kind(), "soroban.contract.invoke");
    assert!(plan
        .warnings
        .iter()
        .all(|w| !w.contains("Unknown intent has no action mapping")));
}

#[test]
fn soroban_deep_template_leaves_unmatched_intent_blocking() {
    let contract = "CBLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";
    let policy: ContractPolicy = serde_json::from_str(&format!(
        r#"{{
  "contract_id": "{contract}",
  "allowed_functions": ["hello"],
  "args_schema": {{
    "hello": {{
      "required": {{
        "to": "symbol"
      }},
      "optional": {{}}
    }}
  }},
  "intent_templates": {{
    "hello": {{
      "aliases": ["say hello"],
      "function": "hello",
      "args": {{
        "to": {{
          "source": "after_to",
          "type": "symbol"
        }}
      }}
    }}
  }}
}}"#
    ))
    .expect("parse policy");

    let prompt = "Please run some unknown contract action";
    let mut plan = build_action_plan(prompt, &decision(IntentStellarLabel::ContractInvoke));
    let report = apply_contract_intent_templates(prompt, &mut plan, &[policy]);

    assert!(!report.expanded);
    assert!(has_intent_blocking_issue(&plan));
    assert!(matches!(plan.actions[0], Action::Unknown { .. }));
}

#[test]
fn intent_stellar_low_confidence_downgrade_is_blocking_unknown() {
    let decision = IntentDecision {
        label: IntentStellarLabel::Unknown,
        score: 0.20,
        threshold: 0.55,
        downgraded_to_unknown: true,
    };
    let plan = build_action_plan("Send 5 XLM to G...", &decision);
    assert!(has_intent_blocking_issue(&plan));
    match &plan.actions[0] {
        Action::Unknown { reason } => {
            assert!(reason.starts_with("intent_low_confidence:"));
        }
        other => panic!("expected Unknown action, got: {other:?}"),
    }
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.starts_with("intent_warning: low_confidence")),
        "missing low confidence warning: {:?}",
        plan.warnings
    );
}
