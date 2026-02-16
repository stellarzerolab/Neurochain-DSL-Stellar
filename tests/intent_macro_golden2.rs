use neurochain::ai::model::AIModel;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
struct Case {
    text: &'static str,
    primary_label: &'static str,
    accepted_labels: &'static [&'static str],
    min_score: f32,
}

fn macro_model_path() -> PathBuf {
    let base = std::env::var("NC_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            path.push("models");
            path
        });

    base.join("intent_macro").join("model.onnx")
}

#[test]
fn intent_macro_golden2() {
    let model_path = macro_model_path();
    if !model_path.exists() {
        eprintln!(
            "intent_macro_golden2 skipped: model not found at {}",
            model_path.display()
        );
        return;
    }

    let model = AIModel::new(model_path.to_string_lossy().as_ref()).expect("intent_macro loads");

    // Golden v2:
    // - Keep strong coverage across all macro labels.
    // - Allow tiny lexical variance for a few naturally ambiguous prompts.
    let cases: &[Case] = &[
        // Loop
        Case {
            text: "Show Ping 2 times",
            primary_label: "Loop",
            accepted_labels: &["Loop"],
            min_score: 0.80,
        },
        Case {
            text: "Say Hello 3 times",
            primary_label: "Loop",
            accepted_labels: &["Loop"],
            min_score: 0.70,
        },
        // Branch
        Case {
            text: "If score equals 10 say Congrats else say Nope",
            primary_label: "Branch",
            accepted_labels: &["Branch"],
            min_score: 0.60,
        },
        Case {
            text: "If battery < 20 print Low elif battery < 50 print Medium else print Full",
            primary_label: "Branch",
            accepted_labels: &["Branch"],
            min_score: 0.60,
        },
        // Arith
        Case {
            text: "Create variable total = 3 + 4 and print it",
            primary_label: "Arith",
            accepted_labels: &["Arith"],
            min_score: 0.70,
        },
        Case {
            text: "Set remainder = 17 % 5 and print remainder",
            primary_label: "Arith",
            accepted_labels: &["Arith"],
            min_score: 0.60,
        },
        // SetVar
        Case {
            text: "Set x to 5",
            primary_label: "SetVar",
            accepted_labels: &["SetVar"],
            min_score: 0.70,
        },
        Case {
            text: "Store 'hello' in greeting and echo it",
            primary_label: "SetVar",
            accepted_labels: &["SetVar", "DocPrint"],
            min_score: 0.50,
        },
        // Concat
        Case {
            text: "Print 'Hello ' + name",
            primary_label: "Concat",
            accepted_labels: &["Concat"],
            min_score: 0.60,
        },
        Case {
            text: "Print greeting + ' ' + target",
            primary_label: "Concat",
            accepted_labels: &["Concat"],
            min_score: 0.60,
        },
        Case {
            text: "Join title + ': ' + body",
            primary_label: "Concat",
            accepted_labels: &["Concat"],
            min_score: 0.50,
        },
        // DocPrint
        Case {
            text: "Say the number 42",
            primary_label: "DocPrint",
            accepted_labels: &["DocPrint"],
            min_score: 0.60,
        },
        Case {
            text: "Print final score",
            primary_label: "DocPrint",
            accepted_labels: &["DocPrint"],
            min_score: 0.60,
        },
        Case {
            text: "Add comment # init block and print Starting",
            primary_label: "DocPrint",
            accepted_labels: &["DocPrint"],
            min_score: 0.70,
        },
        // RoleFlag
        Case {
            text: "Set role moderator",
            primary_label: "RoleFlag",
            accepted_labels: &["RoleFlag"],
            min_score: 0.70,
        },
        Case {
            text: "Promote user to admin",
            primary_label: "RoleFlag",
            accepted_labels: &["RoleFlag"],
            min_score: 0.60,
        },
        // AIBridge
        Case {
            text: "Bridge assistant output to UI",
            primary_label: "AIBridge",
            accepted_labels: &["AIBridge", "DocPrint"],
            min_score: 0.50,
        },
        Case {
            text: "Forward model output to client",
            primary_label: "AIBridge",
            accepted_labels: &["AIBridge", "DocPrint"],
            min_score: 0.35,
        },
        // Unknown
        Case {
            text: "Tell me a joke",
            primary_label: "Unknown",
            accepted_labels: &["Unknown"],
            min_score: 0.30,
        },
        Case {
            text: "How are you doing?",
            primary_label: "Unknown",
            accepted_labels: &["Unknown"],
            min_score: 0.30,
        },
    ];

    fn run_pass(model: &AIModel, cases: &[Case], pass: usize) -> (Vec<f64>, Duration) {
        let mut total = Duration::from_secs(0);
        let mut per_case_ms: Vec<f64> = Vec::with_capacity(cases.len());

        for (i, c) in cases.iter().enumerate() {
            let started = Instant::now();
            let (label, score) = model
                .predict_with_score(c.text)
                .unwrap_or_else(|e| panic!("pass {pass} case {i} predict failed: {e}"));
            let elapsed = started.elapsed();
            total += elapsed;

            let ms = elapsed.as_secs_f64() * 1000.0;
            per_case_ms.push(ms);

            println!(
                "run {pass} case {i}: label={label} score={score:.3} latency_ms={ms:.2} primary={} accepted={:?} min={} | {:?}",
                c.primary_label, c.accepted_labels, c.min_score, c.text
            );

            assert!(
                c.accepted_labels.contains(&label.as_str()),
                "pass {pass} case {i} label mismatch for input: {:?} (got={label}, primary={}, accepted={:?}, score={score:.3})",
                c.text,
                c.primary_label,
                c.accepted_labels
            );
            assert!(
                score >= c.min_score,
                "pass {pass} case {i} score too low for input: {:?} (label={label}, score={score:.3}, min={})",
                c.text,
                c.min_score
            );
        }

        let avg_ms = (total.as_secs_f64() * 1000.0) / (cases.len().max(1) as f64);
        println!(
            "run {pass} summary: cases={} total_ms={:.2} avg_ms={:.2}",
            cases.len(),
            total.as_secs_f64() * 1000.0,
            avg_ms
        );

        (per_case_ms, total)
    }

    let (ms1, total1) = run_pass(&model, cases, 1);
    let (ms2, total2) = run_pass(&model, cases, 2);

    let t1 = total1.as_secs_f64() * 1000.0;
    let t2 = total2.as_secs_f64() * 1000.0;
    let avg1 = t1 / (cases.len().max(1) as f64);
    let avg2 = t2 / (cases.len().max(1) as f64);
    println!(
        "warmup delta: total_ms {:.2} -> {:.2} (diff {:+.2}), avg_ms {:.2} -> {:.2} (diff {:+.2})",
        t1,
        t2,
        t2 - t1,
        avg1,
        avg2,
        avg2 - avg1
    );

    for i in 0..cases.len().min(ms1.len()).min(ms2.len()) {
        println!(
            "case {i} delta_ms: run1={:.2} run2={:.2} diff={:+.2}",
            ms1[i],
            ms2[i],
            ms2[i] - ms1[i]
        );
    }
}
