fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/protocol/src/runtime.ts");
    let content = loom_runtime::typescript_bindings();
    if std::env::args().any(|arg| arg == "--check") {
        if std::fs::read_to_string(&path)? != content {
            return Err("runtime protocol declarations drifted; regenerate them".into());
        }
        println!("Runtime protocol declarations verified");
    } else {
        std::fs::write(path, content)?;
        println!("Runtime protocol declarations generated");
    }
    Ok(())
}
