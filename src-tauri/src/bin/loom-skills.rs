fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match loom_lib::skills::cli::run(&args) {
        Ok(output) => {
            use std::io::Write;
            let result = std::io::stdout()
                .write_all(output.text.as_bytes())
                .and_then(|_| std::io::stdout().flush());
            if let Err(error) = result {
                if error.kind() != std::io::ErrorKind::BrokenPipe {
                    eprintln!("error: could not write CLI output: {error}");
                    std::process::exit(1);
                }
            }
            std::process::exit(output.exit_code);
        }
        Err(error) => {
            eprintln!("error: {}", error.message);
            std::process::exit(1);
        }
    }
}
