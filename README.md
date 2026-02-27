# NeuroChain DSL

NeuroChain is an **offline**, **deterministic** DSL that combines:

- a small scripting language (`.nc`)
- local CPU ONNX classifiers (`tract-onnx`)
- and an intent-based macro system (`macro from AI: ...`) that turns clear English prompts into deterministic DSL templates

NeuroChain has two binaries:

- `neurochain` — CLI interpreter (run scripts + interactive mode)
- `neurochain-server` — REST API server (`POST /api/analyze`)

## Highlights

- Offline CPU inference via ONNX classifiers (`tract-onnx`) — no external APIs required
- Built-in classifier workflows: **SST2**, **Toxicity**, **FactCheck**, **Intent**, and **MacroIntent**
- Deterministic **MacroIntent** pipeline (no GPT/LLM fallback)
- Macro loop counts are clamped to `1..=12` for safety (deterministic output)
- Control flow (`if/elif/else`, `and/or`, comparisons) + variables + arithmetic
- Examples double as regression suites (`examples/`)
- CI gates included: `fmt + clippy + test + audit`

## Mini example (`.nc`)

```nc
AI: "models/distilbert-sst2/model.onnx"
set mood from AI: "I love this movie."
# Comparisons are case-insensitive and trim whitespace.
if mood == "positive":
    neuro "Great"

# Switch to the MacroIntent model (intent → deterministic DSL → run).
AI: "models/intent_macro/model.onnx"
macro from AI: Show Ping 3 times
```

## Prerequisites (build from source)

- Install Rust + Cargo (via `rustup`): https://www.rust-lang.org/tools/install
- Models are expected under `models/` by default (see `docs/models.md`).
  - Recommended one-time download: `bash scripts/fetch_models.sh` (or PowerShell: `powershell -ExecutionPolicy Bypass -File scripts/fetch_models.ps1`)
- Windows (MSVC): Visual Studio 2022 Build Tools / Community with **Desktop development with C++** (+ Windows SDK)
- Linux/WSL: `build-essential` + `pkg-config`
- macOS: Xcode Command Line Tools (`xcode-select --install`)

## Quickstart

Start here:

```bash
git clone https://github.com/stellarzerolab/Neurochain-DSL.git
cd Neurochain-DSL
bash scripts/fetch_models.sh
cargo run --release --bin neurochain
```

What this does:

1. Clones the repository.
2. Downloads the model pack.
3. Verifies model archive integrity (manifest SHA256, and signed `SHA256SUMS` if `cosign` is available).
4. Starts the NeuroChain CLI in interactive mode.

In the interactive CLI, you can type `help`, `about`, `version`, `exit`.

Windows PowerShell for model download:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/fetch_models.ps1
```

### Next commands (optional)

Run one model example:

```bash
cargo run --release --bin neurochain -- examples/distilbert-sst2check.nc
```

Run the REST server:

```bash
cargo run --release --bin neurochain-server
```

If you expose `/api/analyze` publicly, set `NC_API_KEY` and require clients/proxy to send `X-API-Key: ...` (or `Authorization: Bearer ...`).

### Quality checks (recommended before pushing)

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo audit --deny warnings --ignore RUSTSEC-2024-0436 \
            --ignore RUSTSEC-2025-0134
```

If disk usage grows due to many builds (debug/release, multiple binaries, tests), run:

```bash
cargo clean
```

If you build in both PowerShell and WSL, you need to clean in both environments. See `docs/troubleshooting.md`.

## Documentation

- `docs/getting_started.md` — end-to-end: run scripts, CLI, server, tests
- `docs/language.md` — DSL language guide (syntax + semantics)
- `docs/macros.md` — MacroIntent (macro → DSL → run) + best practices
- `docs/models.md` — AI models, labels, and multi-model scripts
- `docs/security.md` — Rust security & tooling stack + CI gates
- `docs/troubleshooting.md` — common issues (WSL/target/env/logs)

## Performance notes (MacroIntent)

To see MacroIntent label + score + per-case latency, run:

```bash
cargo test --release --test intent_macro_golden -- --nocapture
```

For model usage examples, see `docs/models.md` and `examples/*check.nc`.

## License

Apache-2.0. See `LICENSE`.

Redistributions must retain `LICENSE` and `NOTICE`.

Note: the `models/` directory may contain third-party model files with their own licenses.

## Branding / trademarks

The Apache-2.0 license does **not** grant any rights to use the NeuroChain DSL or StellarZeroLab names, logos, or branding to imply endorsement or official affiliation.
If you fork this project, please use your own name and branding for your fork/release.

© 2026 StellarZeroLab
