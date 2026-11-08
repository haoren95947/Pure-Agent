//! Walker-backed grep/find behavior that the pre-walker BFS did not have:
//! `.gitignore` pruning, nested ignore files, and `node_modules` skipping.
//! These fail on the old traversal (which only skipped `.git/`).

use std::sync::Arc;

use pure_agent::agent_tool::AgentTool;
use pure_tools::{ExecutionToolContext, FileSystem, InMemoryExecutionEnv};
use tokio_util::sync::CancellationToken;

fn fresh_context() -> (Arc<InMemoryExecutionEnv>, ExecutionToolContext) {
    let env = Arc::new(InMemoryExecutionEnv::with_cwd("/tmp/work".into()));
    let env_dyn: Arc<dyn pure_tools::ExecutionEnv> = env.clone();
    let mut_env: Arc<dyn pure_tools::MutatingEnv> = env.clone();
    let ctx = ExecutionToolContext::new(env_dyn, Some(mut_env));
    (env, ctx)
}

async fn seed(env: &InMemoryExecutionEnv, rel: &str, bytes: Vec<u8>) {
    if let Some(parent) = std::path::Path::new(rel).parent() {
        if !parent.as_os_str().is_empty() {
            env.create_dir(&parent.to_string_lossy(), true, None)
                .await
                .expect("create parent");
        }
    }
    let abs = env
        .absolute_path(rel, None)
        .await
        .expect("absolute_path for seed");
    env.seed_file(&abs.to_string_lossy(), bytes).await;
}

fn text_output(r: &pure_agent::types::AgentToolResult) -> String {
    use pure_agent::types::TextContentOrImage;
    r.content
        .iter()
        .filter_map(|c| match c {
            TextContentOrImage::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn run(
    tool: Arc<dyn AgentTool>,
    params: serde_json::Value,
) -> pure_agent::types::AgentToolResult {
    tool.execute("walk-1", params, CancellationToken::new(), Arc::new(|_| ()))
        .await
        .expect("tool ok")
}

#[tokio::test]
async fn grep_honours_gitignore() {
    let (env, ctx) = fresh_context();
    seed(&env, ".gitignore", b"ignored/\n".to_vec()).await;
    seed(&env, "ignored/secret.txt", b"needle\n".to_vec()).await;
    seed(&env, "kept.txt", b"needle\n".to_vec()).await;

    let out = text_output(
        &run(
            pure_tools::create_grep_tool(&ctx, None),
            serde_json::json!({ "pattern": "needle", "path": "." }),
        )
        .await,
    );
    assert!(out.contains("kept.txt"), "got: {out}");
    assert!(
        !out.contains("ignored/secret.txt"),
        "gitignore ignored: {out}"
    );
}

#[tokio::test]
async fn grep_honours_nested_gitignore() {
    let (env, ctx) = fresh_context();
    seed(&env, "sub/.gitignore", b"*.log\n".to_vec()).await;
    seed(&env, "sub/a.log", b"needle\n".to_vec()).await;
    seed(&env, "sub/b.txt", b"needle\n".to_vec()).await;

    let out = text_output(
        &run(
            pure_tools::create_grep_tool(&ctx, None),
            serde_json::json!({ "pattern": "needle", "path": "." }),
        )
        .await,
    );
    assert!(out.contains("sub/b.txt"), "got: {out}");
    assert!(
        !out.contains("sub/a.log"),
        "nested gitignore ignored: {out}"
    );
}

#[tokio::test]
async fn find_honours_gitignore_and_skips_node_modules() {
    let (env, ctx) = fresh_context();
    seed(&env, ".gitignore", b"build/\n".to_vec()).await;
    seed(&env, "build/drop.rs", b"".to_vec()).await;
    seed(&env, "node_modules/pkg/index.rs", b"".to_vec()).await;
    seed(&env, "src/keep.rs", b"".to_vec()).await;

    let out = text_output(
        &run(
            pure_tools::create_find_tool(&ctx, None),
            serde_json::json!({ "pattern": "**/*.rs", "path": "." }),
        )
        .await,
    );
    assert!(out.contains("src/keep.rs"), "got: {out}");
    assert!(!out.contains("build/drop.rs"), "gitignore ignored: {out}");
    assert!(
        !out.contains("node_modules"),
        "node_modules not pruned: {out}"
    );
}

#[tokio::test]
async fn find_returns_path_order() {
    let (env, ctx) = fresh_context();
    for name in ["b.rs", "a/z.rs", "a.rs", "c/d/e.rs"] {
        seed(&env, name, b"".to_vec()).await;
    }
    let out = text_output(
        &run(
            pure_tools::create_find_tool(&ctx, None),
            serde_json::json!({ "pattern": "**/*.rs", "path": "." }),
        )
        .await,
    );
    let lines: Vec<&str> = out.lines().filter(|line| line.ends_with(".rs")).collect();
    assert_eq!(lines, ["a.rs", "a/z.rs", "b.rs", "c/d/e.rs"]);
}
