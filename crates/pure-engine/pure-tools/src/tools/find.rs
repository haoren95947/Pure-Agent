//! Mirrors `packages/coding-agent/src/core/tools/find.ts` — the built-in `find`
//! `AgentTool`. Searches for files by glob pattern and returns matching paths
//! (relative to the search directory, posix separators). Read-only — goes
//! through the `FileSystem` trait so it runs against both `OsExecutionEnv` and
//! `InMemoryExecutionEnv`.
//!
//! Divergence from TS (documented): the TS tool **shells out to `fd`** (with
//! gitignore-aware walking + `--full-path` rewrites + optional auto-download).
//! The Rust port implements the walk **in-process** via the `FileSystem` trait
//! and the `globset` crate, so it works against any `ExecutionEnv`. Trade-offs:
//! no full `.gitignore` awareness in v1 (only `.git/` directories are skipped;
//! revisit via the `ignore` crate for an `OsExecutionEnv`-only fast path), no
//! external binary, and a deterministic sorted BFS walk order rather than fd's
//! walk. The `**/`-prepend rule for path-containing patterns is ported; the
//! Windows `[/\\]` separator rewrite is unnecessary (we normalize to posix
//! internally). `relativizeFindResultPath` is ported directly.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use pure_agent::agent_tool::AgentTool;
use pure_agent::error::AgentError;
use pure_agent::types::{AgentToolResult, TextContentOrImage, ToolResultPartial};
use pure_ai::types::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::env::ExecutionEnv;
use crate::path_utils::resolve_read_tool_path;
use crate::tools::tool_context::ExecutionToolContext;
use crate::truncate::{
    format_size, truncate_head, TruncationOptions, TruncationResult, DEFAULT_MAX_BYTES,
};
use crate::walker::{self, Detail, FileType, GlobError, GlobOptions};

/// Default max results. Mirrors TS `find.ts::DEFAULT_LIMIT`.
const DEFAULT_LIMIT: u32 = 1000;

/// Input for the find tool. Mirrors TS `FindToolInput`.
#[derive(Debug, Clone, JsonSchema, Deserialize)]
pub struct FindInput {
    /// Glob pattern to match files, e.g. `*.ts`, `**/*.json`, `src/**/*.spec.ts`.
    pub pattern: String,
    /// Directory to search in (default: current directory).
    #[serde(default)]
    pub path: Option<String>,
    /// Maximum number of results (default 1000).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Structured details for the find tool. Mirrors TS `FindToolDetails`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FindToolDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<TruncationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_limit_reached: Option<u32>,
}

/// Options for `create_find_tool`. Forward-compat seam (the TS tool carries a
/// pluggable `operations` bag for remote backends; the Rust port always uses
/// the `FileSystem` trait).
#[derive(Debug, Clone, Default)]
pub struct FindToolOptions {}

/// The built-in `find` tool. Holds an `Arc<dyn ExecutionEnv>` (read-only view).
pub struct FindTool {
    schema: Tool,
    env: Arc<dyn ExecutionEnv>,
}

impl FindTool {
    fn schema() -> Tool {
        let params = schemars::schema_for!(FindInput);
        Tool {
            name: "find".to_string(),
            description: format!(
                "Search for files by glob pattern. Returns matching file paths relative to the \
                 search directory. Skips `.git/` directories. Output is truncated to {DEFAULT_LIMIT} \
                 results or {kb}KB (whichever is hit first).",
                kb = DEFAULT_MAX_BYTES / 1024
            ),
            parameters: pure_ai::types::Schema::new(serde_json::to_value(params).unwrap_or_default()),
            constrained_sampling: None,
        }
    }
}

/// Build the `find` tool from a context. Mirrors TS `createFindTool`.
pub fn create_find_tool(
    context: &ExecutionToolContext,
    options: Option<FindToolOptions>,
) -> Arc<dyn AgentTool> {
    let _ = options; // forward-compat seam, no surface in v1
    Arc::new(FindTool {
        schema: FindTool::schema(),
        env: context.env().clone(),
    })
}

#[async_trait]
impl AgentTool for FindTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }
    fn label(&self) -> &str {
        "find"
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        params: serde_json::Value,
        signal: CancellationToken,
        _on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let input: FindInput = serde_json::from_value(params)
            .map_err(|e| AgentError::Validation(format!("find input invalid: {e}")))?;

        let cancel = Some(&signal);
        let effective_limit = input.limit.unwrap_or(DEFAULT_LIMIT).max(1) as usize;

        // Resolve the search path (read-only + fuzzy variants).
        let search_path =
            resolve_read_tool_path(&*self.env, input.path.as_deref().unwrap_or("."), cancel)
                .await
                .map_err(file_err_to_agent)?;

        // Path-not-found (the TS custom-glob branch checks exists; the fd branch
        // lets fd error — we check up-front for a uniform in-process path).
        if !self
            .env
            .exists(&search_path, cancel)
            .await
            .map_err(file_err_to_agent)?
        {
            return Err(AgentError::Tool(format!("Path not found: {search_path}")));
        }

        // A file root matches by basename; a directory root goes through the
        // gitignore-aware parallel glob. Ask for one extra match so hitting the
        // result limit is observable, then truncate.
        let info = self
            .env
            .file_info(&search_path, cancel)
            .await
            .map_err(file_err_to_agent)?;
        let fs = walker::Fs::new(self.env.clone(), Some(signal.clone()));
        let (collected, result_limit_reached) = if info.kind == crate::env::FileKind::File {
            let selector = walker::Selector::new(&input.pattern, true)
                .map_err(|e| AgentError::Validation(format!("find pattern invalid: {e}")))?;
            let name = Path::new(&search_path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let collected = if selector.matches(name.as_bytes()) {
                vec![name]
            } else {
                Vec::new()
            };
            (collected, false)
        } else {
            let options = GlobOptions {
                pattern: input.pattern.clone(),
                recursive: true,
                file_type: None,
                max_results: Some(effective_limit.saturating_add(1)),
                sort_by_mtime: false,
                include_node_modules: Some(false),
                walk: walker::Options {
                    hidden: true,
                    gitignore: true,
                    skip_git: true,
                    skip_node_modules: true,
                    detail: Detail::Minimal,
                    ..Default::default()
                },
            };
            let mut found = walker::glob(&fs, Path::new(&search_path), &options)
                .await
                .map_err(|error| match error {
                    GlobError::Pattern(err) => {
                        AgentError::Validation(format!("find pattern invalid: {err}"))
                    }
                    GlobError::Walk(err) => AgentError::Tool(err.to_string()),
                })?;

            // Symlinks are not reported during a walk.
            found.retain(|m| m.file_type != FileType::Symlink);
            let reached = found.len() > effective_limit;
            found.truncate(effective_limit);
            let collected: Vec<String> = found
                .into_iter()
                .map(|m| {
                    if m.file_type == FileType::Dir {
                        let mut path = m.path;
                        if !path.ends_with('/') {
                            path.push('/');
                        }
                        path
                    } else {
                        m.path
                    }
                })
                .collect();
            (collected, reached)
        };

        if collected.is_empty() {
            return Ok(AgentToolResult {
                content: vec![TextContentOrImage::text("No files found matching pattern")],
                details: serde_json::Value::Null,
                ..Default::default()
            });
        }

        let raw_output = collected.join("\n");
        // Byte cap only — line cap disabled (result count already capped).
        let truncation = truncate_head(
            &raw_output,
            TruncationOptions {
                max_lines: Some(usize::MAX),
                max_bytes: None,
            },
        );
        let was_truncated = truncation.truncated;
        let mut output = truncation.content.clone();
        let mut details = FindToolDetails {
            truncation: None,
            result_limit_reached: None,
        };
        let mut notices: Vec<String> = Vec::new();
        if result_limit_reached {
            notices.push(format!(
                "{effective_limit} results limit reached. Use limit={} for more, or refine pattern",
                effective_limit * 2
            ));
            details.result_limit_reached = Some(effective_limit as u32);
        }
        if was_truncated {
            notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES)));
            details.truncation = Some(truncation);
        }
        if !notices.is_empty() {
            output.push_str(&format!("\n\n[{}]", notices.join(". ")));
        }

        let details_value = serde_json::to_value(&details).unwrap_or(serde_json::Value::Null);
        Ok(AgentToolResult {
            content: vec![TextContentOrImage::text(output)],
            details: details_value,
            ..Default::default()
        })
    }
}

/// Map a `FileError` into an `AgentError::Tool` (same shape as `read.rs`).
fn file_err_to_agent(e: crate::error::FileError) -> AgentError {
    AgentError::Tool(e.to_string())
}
