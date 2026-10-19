//! Local JSONL transport for exercising the real application core. No network
//! listener or provider execution; the caller owns the explicit data directory.
use loom_core::{Core, CoreReply};
use std::io::{BufRead, Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .ok_or("Usage: loom-core-host <data-directory>")?;
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let core = Core::open(root)?;
    let input = std::io::stdin();
    let mut reader = input.lock();
    let output = std::io::stdout();
    let mut writer = output.lock();
    loop {
        let mut bytes = Vec::new();
        let n = reader
            .by_ref()
            .take(8 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)?;
        if n == 0 {
            break;
        }
        let oversized = n > 8 * 1024 * 1024 || !bytes.ends_with(b"\n");
        let reply = if oversized {
            CoreReply::invalid_request("frame is oversized or missing its newline")
        } else {
            core.dispatch_json(&bytes)
        };
        serde_json::to_writer(&mut writer, &reply)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        if oversized {
            break;
        }
    }
    Ok(())
}
