fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    std::process::exit(loom_lib::agent_cli::main(&args));
}
