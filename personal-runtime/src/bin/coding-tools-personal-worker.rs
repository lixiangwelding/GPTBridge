fn main() {
    match coding_tools_personal_runtime::worker::run_from_args() {
        Some(code) => std::process::exit(code),
        None => { eprintln!("This binary only executes persisted personal-runtime jobs."); std::process::exit(2); }
    }
}
