use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/protocol/src/workbench.ts");
    let content = loom_core::typescript_bindings();
    if std::env::args().skip(1).any(|arg| arg == "--check") {
        if std::fs::read_to_string(&path)? != content {
            return Err("workbench types drifted; regenerate them".into());
        }
        println!("Workbench wire declarations verified");
    } else {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, content)?;
        println!("Wrote {}", path.display());
    }
    Ok(())
}
