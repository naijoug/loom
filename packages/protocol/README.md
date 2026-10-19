# Workbench protocol

`src/workbench.ts` is generated from `crates/loom-core`'s Rust/serde wire types with the optional `bindings` feature. Do not edit declarations by hand.

```sh
cargo run -p loom-core --features bindings --bin export-workbench-types
pnpm check:core
```

The check fails on declaration drift and compiles wire-shape examples. Runtime validation remains in Rust; TypeScript declarations do not authenticate a client or validate untrusted network responses. Transport integration and frontend replay remain separate M1 work.
