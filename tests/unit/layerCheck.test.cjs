const test = require("node:test"),
  assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
test("new shared modules cannot hide feature or process dependencies behind aliases", async () => {
  const { moduleReferences, spawnsProcess } = await import("../../scripts/lib/rust-layer-rules.mjs");
  for (const code of [
    "use crate::research as domain;",
    "use crate::{agent, research::{models as data}};",
    "use crate as root; use root::research::models;",
    "use loom_lib::{research as other};",
  ]) {
    assert.deepEqual(moduleReferences(code, ["research", "experiments"]), ["research"]);
  }
  for (const code of [
    "use std::process::Command as Proc;",
    "use tokio::{process as child};",
    "use tokio::process as child;",
  ])
    assert.ok(spawnsProcess(code));
  assert.deepEqual(moduleReferences('// crate::research\nlet hint = "crate::research";', ["research"]), []);
  assert.equal(spawnsProcess('let hint = "std::process::Command";'), false);
});
test("layer scanner keeps production after inline tests and ignores literal/comment braces", async () => {
  const { productionCode, rustCodeMask } = await import("../../scripts/lib/rust-production.mjs");
  const source = `use crate::agent;
 #[cfg(all(test, unix))]
 mod tests { fn test() { let a = r###"} raw {"###; let b = "escaped \\" {"; let c = '}'; /* } /* { */ } */ } }
 pub fn production() { crate::process_supervisor::supervisor(); }
 #[cfg(test)] mod more_tests;
 pub fn after() { agent::runner::run(); }`;
  const code = rustCodeMask(productionCode("runtime.rs", source));
  assert.ok(!code.includes("fn test"));
  assert.ok(!code.includes("more_tests"));
  assert.ok(code.includes("pub fn production"));
  assert.ok(code.includes("crate::process_supervisor"));
  assert.ok(code.includes("agent::runner::run"));
  assert.equal(code.split("\n").length, source.split("\n").length);
  assert.equal(productionCode("real_tests.rs", source), "");
});
test("the real Chat runner remains inside the layer scan after its test module", async () => {
  const { productionCode } = await import("../../scripts/lib/rust-production.mjs");
  const text = readFileSync("src-tauri/src/chat/runtime.rs", "utf8");
  const code = productionCode("chat/runtime.rs", text);
  assert.ok(code.includes("pub(super) async fn run_observed"));
  assert.ok(code.includes("agent::run("));
  assert.ok(!code.includes("fn consecutive_text_blocks_become_separate_paragraphs"));
});
test("HTTP clients and private agent paths are found behind aliases but not in strings or comments", async () => {
  const { agentPrivatePaths, moduleReferences, usesHttpClient } =
    await import("../../scripts/lib/rust-layer-rules.mjs");
  for (const code of [
    "fn c() -> reqwest::Client { todo!() }",
    "use reqwest::{header, Client};",
    "use reqwest as http;",
    "let r = ::reqwest::get(url);",
  ])
    assert.ok(usesHttpClient(code), code);
  for (const code of ['let hint = "reqwest::Client";', "// reqwest::Client", "let my_reqwest_count = 1;"])
    assert.equal(usesHttpClient(code), false, code);
  for (const code of [
    "crate::agent::providers::resolve_at(dir, id)",
    "use crate::agent::registry::read_agents;",
    "agent :: api :: run(request)",
  ])
    assert.ok(agentPrivatePaths(code), code);
  for (const code of ["use crate::agent::{run, AgentEvent};", "agent::run(request)", '"agent::providers::x"'])
    assert.equal(agentPrivatePaths(code), false, code);
  assert.deepEqual(moduleReferences("use crate::{agent, agent_cli::main};", ["agent_cli"]), ["agent_cli"]);
  assert.deepEqual(moduleReferences("loom_lib::agent_cli::main(&args)", ["agent_cli"]), ["agent_cli"]);
  assert.deepEqual(moduleReferences("use crate::agent::run;", ["agent_cli"]), []);
});
