//! node-cpu-v1 calibration and baseline. The worker cwd is the managed copy.
#![allow(dead_code)]
use super::{
    decision::{self, BaselineGate, Sample},
    experiments_allowed,
};
use crate::tool_process::{BoundedCommand, ProcessKind, ProcessOwner};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

const WORKER: &str = r#"
import { pathToFileURL } from "node:url";
import { readFileSync } from "node:fs";
const request = JSON.parse(readFileSync(process.argv[2], "utf8"));
const loaded = await import(pathToFileURL(request.entry).href);
if (typeof loaded.compute !== "function") {
  console.log(JSON.stringify({ ok: false, reason: "template_incompatible" }));
  process.exit(0);
}
const copy = (value) => JSON.parse(JSON.stringify(value));
if (request.phase === "correctness") {
  for (const sample of request.samples) {
    const output = loaded.compute(copy(sample.input));
    if (output instanceof Promise || JSON.stringify(output) !== JSON.stringify(sample.output)) {
      console.log(JSON.stringify({ ok: false, reason: "correctness_failed" }));
      process.exit(0);
    }
  }
  console.log(JSON.stringify({ ok: true, phase: "correctness" }));
  process.exit(0);
}
const prepared = [];
let bytes = 0;
for (let index = 0; index < request.repeat * request.inputs.length; index += 1) {
  const raw = JSON.stringify(request.inputs[index % request.inputs.length]);
  bytes += Buffer.byteLength(raw);
  if (bytes > 64 * 1024 * 1024) {
    console.log(JSON.stringify({ ok: false, reason: "input_copy_limit" }));
    process.exit(0);
  }
  prepared.push(JSON.parse(raw));
}
const outputs = new Array(prepared.length);
const started = process.hrtime.bigint();
for (let index = 0; index < prepared.length; index += 1) outputs[index] = loaded.compute(prepared[index]);
const elapsed = process.hrtime.bigint() - started;
if (outputs.some((output) => output instanceof Promise)) {
  console.log(JSON.stringify({ ok: false, reason: "template_incompatible" }));
  process.exit(0);
}
console.log(JSON.stringify({ ok: true, phase: "time", elapsedNs: elapsed.toString(), calls: outputs.length }));
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaselineOutcome {
    Ready {
        batch_size: u32,
        batches_ns: Vec<u128>,
    },
    Failed {
        reason: String,
    },
    Unstable {
        reason: String,
        batches_ns: Vec<u128>,
    },
    Incompatible {
        reason: String,
    },
}

#[derive(Deserialize)]
struct WorkerReport {
    ok: bool,
    reason: Option<String>,
    #[serde(rename = "elapsedNs")]
    elapsed_ns: Option<String>,
}

pub async fn measure_baseline(
    node: &Path,
    managed: &Path,
    entry: &Path,
    correctness: &[serde_json::Value],
    inputs: &[serde_json::Value],
) -> Result<BaselineOutcome, String> {
    if !experiments_allowed() {
        return Err("research_experiments_not_enabled".into());
    }
    let worker = managed.join("worker.mjs");
    fs::write(&worker, WORKER).map_err(|error| error.to_string())?;
    let correctness_report = run_worker(
        node,
        managed,
        &worker,
        &json!({
            "entry": entry,
            "phase": "correctness",
            "samples": correctness,
            "inputs": [],
            "repeat": 1
        }),
    )
    .await?;
    if !correctness_report.ok {
        return Ok(BaselineOutcome::Failed {
            reason: correctness_report
                .reason
                .unwrap_or_else(|| "correctness_failed".into()),
        });
    }
    let mut batch_size = 1u32;
    let started = std::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(5) {
            return Ok(BaselineOutcome::Incompatible {
                reason: "calibration_budget".into(),
            });
        }
        let sample = timed_batch(node, managed, &worker, entry, inputs, batch_size).await?;
        match decision::next_batch_size(batch_size, sample) {
            Ok(chosen) if chosen == batch_size => {
                batch_size = chosen;
                break;
            }
            Ok(next) => batch_size = next,
            Err(reason) => {
                return Ok(BaselineOutcome::Incompatible {
                    reason: reason.into(),
                })
            }
        }
    }
    for _ in 0..3 {
        timed_batch(node, managed, &worker, entry, inputs, batch_size).await?;
    }
    let mut batches = Vec::with_capacity(5);
    for _ in 0..5 {
        batches.push(timed_batch(node, managed, &worker, entry, inputs, batch_size).await?);
    }
    let samples: Vec<Sample> = batches.iter().copied().map(Sample::Ns).collect();
    match decision::baseline_gate(&samples) {
        BaselineGate::Ready => Ok(BaselineOutcome::Ready {
            batch_size,
            batches_ns: batches,
        }),
        BaselineGate::Unstable => Ok(BaselineOutcome::Unstable {
            reason: format!("spread exceeded 2/100 for batches {batches:?}"),
            batches_ns: batches,
        }),
        BaselineGate::Invalid => Ok(BaselineOutcome::Failed {
            reason: format!("baseline samples were not five positive durations: {batches:?}"),
        }),
    }
}

async fn timed_batch(
    node: &Path,
    managed: &Path,
    worker: &Path,
    entry: &Path,
    inputs: &[serde_json::Value],
    repeat: u32,
) -> Result<u128, String> {
    let report = run_worker(
        node,
        managed,
        worker,
        &json!({
            "entry": entry,
            "phase": "time",
            "samples": [],
            "inputs": inputs,
            "repeat": repeat
        }),
    )
    .await?;
    if !report.ok {
        return Err(report.reason.unwrap_or_else(|| "baseline_failed".into()));
    }
    report
        .elapsed_ns
        .ok_or("baseline_failed")?
        .parse()
        .map_err(|_| "baseline_failed".into())
}

async fn run_worker(
    node: &Path,
    managed: &Path,
    worker: &Path,
    request: &serde_json::Value,
) -> Result<WorkerReport, String> {
    let request_path = managed.join(format!(
        "request-{}.json",
        crate::models::IdGenerator::default().next("exp")
    ));
    fs::write(
        &request_path,
        serde_json::to_vec(request).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let output = crate::tool_process::run_bounded(
        BoundedCommand {
            operation_id: "experiment-node".into(),
            owner: ProcessOwner::Feature {
                feature: "experiments".into(),
                scope_id: "baseline".into(),
                run_id: "baseline".into(),
            },
            kind: ProcessKind::Tool,
            program: node.to_string_lossy().into_owned(),
            args: vec![
                worker.to_string_lossy().into_owned(),
                request_path.to_string_lossy().into_owned(),
            ],
            cwd: managed.to_path_buf(),
            timeout: Duration::from_secs(60),
            max_stdout: 1024 * 1024,
            max_stderr: 64 * 1024,
            disk_root: None,
            env: Default::default(),
            remove_env: Vec::new(),
            on_registered: None,
        },
        &|| false,
    )
    .await?;
    if !output.success {
        return Err(format!(
            "node_worker_failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

pub fn spec_digest(
    entry: &str,
    allowed: &[String],
    correctness: &[serde_json::Value],
    inputs: &[serde_json::Value],
) -> String {
    let payload = json!({
        "rule": "node-cpu-v1",
        "calibrationTargetNs": decision::CALIBRATION_TARGET_NS,
        "maxBatch": decision::MAX_BATCH,
        "warmupBatches": 3,
        "measuredBatches": 5,
        "spread": "2/100",
        "gain": "3/100",
        "entry": entry,
        "allowed": allowed,
        "correctness": correctness,
        "benchmarkInputs": inputs,
    });
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&payload).unwrap_or_default())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn enable() {
        assert!(
            !super::super::experiments_allowed()
                || std::env::var("LOOM_RESEARCH_EXPERIMENTS").as_deref() == Ok("1")
        );
        std::env::remove_var("LOOM_RESEARCH_EXPERIMENTS");
        assert!(!super::super::experiments_allowed());
        std::env::set_var("LOOM_RESEARCH_EXPERIMENTS", "1");
        std::env::set_var("LOOM_EXPERIMENT_NODE", "/exec-daemon/node");
    }

    #[tokio::test]
    async fn baseline_failure_and_timing_are_explained_without_touching_the_project() {
        enable();
        let root = std::env::temp_dir().join(format!("loom-exp-base-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("project");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join("src/compute.js"), "export function compute(input){ if (input.ms){ const end = Date.now()+input.ms; while (Date.now()<end){} } return { n: input.n }; }\n").unwrap();
        let status = Command::new("git")
            .args(["init", "--quiet", "-b", "main"])
            .current_dir(&project)
            .status()
            .unwrap();
        assert!(status.success());
        Command::new("git")
            .args([
                "-c",
                "user.name=E",
                "-c",
                "user.email=e@example.invalid",
                "add",
                ".",
            ])
            .current_dir(&project)
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.name=E",
                "-c",
                "user.email=e@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "base",
            ])
            .current_dir(&project)
            .status()
            .unwrap();
        let managed = root.join("managed");
        let snapshot = super::super::snapshot::capture_snapshot(
            &project,
            &managed,
            "abcdef1",
            &["src/compute.js".into()],
        )
        .unwrap();
        let node = Path::new("/exec-daemon/node");
        let entry = managed.join("src/compute.js");
        let failed = measure_baseline(
            node,
            &managed,
            &entry,
            &[json!({"input":{"n":1,"ms":0},"output":{"n":2}})],
            &[json!({"n":1,"ms":0})],
        )
        .await
        .unwrap();
        assert!(
            matches!(failed, BaselineOutcome::Failed { ref reason } if reason == "correctness_failed"),
            "{failed:?}"
        );
        let ready = measure_baseline(
            node,
            &managed,
            &entry,
            &[json!({"input":{"n":1,"ms":0},"output":{"n":1}})],
            &[json!({"n":1,"ms":60})],
        )
        .await
        .unwrap();
        match ready {
            BaselineOutcome::Ready {
                batch_size,
                batches_ns,
            } => {
                assert!(batch_size >= 1);
                assert_eq!(batches_ns.len(), 5);
                assert!(batches_ns.iter().all(|value| *value > 0));
            }
            BaselineOutcome::Unstable { reason, batches_ns } => {
                assert!(reason.contains("spread exceeded"));
                assert_eq!(batches_ns.len(), 5);
            }
            other => panic!("unexpected baseline {other:?}"),
        }
        let porcelain = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&project)
            .output()
            .unwrap();
        assert!(
            porcelain.stdout.is_empty(),
            "{}",
            String::from_utf8_lossy(&porcelain.stdout)
        );
        assert_eq!(snapshot.files[0].path, "src/compute.js");
        let first = spec_digest(
            "src/compute.js",
            &["src/compute.js".into()],
            &[json!({"input":{"n":1},"output":{"n":1}})],
            &[json!({"n":1})],
        );
        let second = spec_digest(
            "src/compute.js",
            &["src/compute.js".into()],
            &[json!({"input":{"n":2},"output":{"n":2}})],
            &[json!({"n":1})],
        );
        assert_ne!(first, second);
        fs::remove_dir_all(root).unwrap();
        std::env::remove_var("LOOM_RESEARCH_EXPERIMENTS");
    }

    #[test]
    fn recorded_unstable_baseline_names_the_spread() {
        let samples = [100, 100, 100, 100, 130].map(|value| Sample::Ns(value * 1_000_000));
        assert_eq!(decision::baseline_gate(&samples), BaselineGate::Unstable);
    }
}
