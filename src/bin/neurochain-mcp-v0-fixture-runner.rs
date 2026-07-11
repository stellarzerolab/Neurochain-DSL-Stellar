use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    match neurochain::mcp_v0_fixture::run_fixture_args(env::args().skip(1).collect()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            eprintln!("{}", neurochain::mcp_v0_fixture::usage());
            ExitCode::from(2)
        }
    }
}
