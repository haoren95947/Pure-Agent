//! The `pi` binary entry point. Mirrors the TS
//! `packages/coding-agent/src/cli.ts` (the `#!/usr/bin/env node` shebang file
//! whose `bin` entry in `package.json` resolves to `dist/cli.js`, which just
//! calls `main(process.argv.slice(2))`).
//!
//! Rust's analogue: a thin `#[tokio::main]` wrapper that boots the multi-thread
//! runtime the harness needs (the harness's `StreamFn` bridge uses
//! `block_in_place`, which requires the multi-thread flavor) and hands control
//! to [`pure_cli::main::run`], exiting with its returned code.

use std::process::ExitCode;

fn main() -> ExitCode {
    // A print-mode run nests provider, streaming, harness and tool layers deep
    // enough that a debug build can exceed the platform's default main-thread
    // stack (~1 MB on MSVC). A released build fits, but tests and local debug
    // runs must too, so the runtime gets an explicit budget instead of the
    // default.
    let worker = std::thread::Builder::new()
        .name("pure-main".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(run)
        .expect("failed to spawn the main worker thread");
    match worker.join() {
        Ok(code) => code,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn run() -> ExitCode {
    // The harness run loop uses `block_in_place` to drive the provider stream
    // synchronously inside the async lane; that requires the multi-thread
    // runtime (configured above). `run` does everything else: arg parse, model
    // resolution, harness build, mode dispatch.
    let code = pure_cli::app::run().await;
    ExitCode::from(code as u8)
}