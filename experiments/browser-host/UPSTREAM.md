# Native browser host experiment

This M0 experiment adapts the `cefsimple` example from [tauri-apps/cef-rs](https://github.com/tauri-apps/cef-rs/tree/a712ad9bacd234c050631a95b9745b27d346b7cc/examples/cefsimple), SHA `a712ad9bacd234c050631a95b9745b27d346b7cc`, licensed Apache-2.0 OR MIT. Both license files are retained here. Its example resources are kept in `designs/craft-v0.14.0/cef-probe-resources/`.

Loom changes: isolated required profile, local fixture URL, larger initial window, native close hides the window, stdio commands marshalled to the CEF UI thread, direct CDP/AX results without a public debugging port. This is a feasibility probe, not the final multi-workspace BrowserHost or a production authorization boundary. No external credentials are configured. Main Loom remains Tauri 2.11.1.

The `cef` registry dependency is pinned independently; `.ref` is only the current build tool/runtime cache. Production packaging must provide pinned Chromium resources and signed helper applications in the normal build pipeline.

## Reproduce on macOS arm64

1. Clone `tauri-apps/cef-rs` at the SHA above into `.ref/repos/cef-rs`.
2. Create `.ref/toolchain` with `python3 -m venv`; install CMake 4.4.3 and Ninja 1.13.2 there.
3. Run the upstream `export-cef-dir` tool with output `<project>/.ref/cef-runtime` (CEF `154.0.32+g682c378+chromium-154.0.8037.58`, macOS arm64). It verifies the archive hash.
4. With `CEF_PATH`, `CMAKE`, and the toolchain's bin directory on PATH, build upstream `bundle-cef-app` (`cargo build -p cef --features build-util --bin bundle-cef-app`).
5. Run `scripts/build-browser-probe.sh`, then `LOOM_PREVIEW_TARGET=browser-probe scripts/debug.sh desktop`. The local fixture uses port 1420.
6. For native input checks use `LOOM_BROWSER_KEEP_OPEN=1` with the same debug command, and finish with `scripts/debug.sh stop`.

Native macOS adaptations also explicitly enable Chromium accessibility and forward Edit menu actions to the focused frame; these resolved an observed missing AX tree and failed paste in the initial example-based build. Chinese paste/Return was verified through CUA. Actual IME composition, Windows/Linux behavior, downloads and production signing remain separate gates.
