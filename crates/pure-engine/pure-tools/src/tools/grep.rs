//! Mirrors `packages/coding-agent/src/core/tools/grep.ts` — the built-in `grep`
//! `AgentTool`. Searches file contents for a regex/literal pattern and returns
//! matching lines with `file:line: text` (and optional `-line-` context lines).
//! Read-only — goes through the `FileSystem` trait so it runs against both
//! `OsExecutionEnv` and `InMemoryExecutionEnv`.
//!
//! Divergence from TS (documented): the TS tool **shells out to `rg`**
//! (ripgrep), parsing its `--json` stream, and re-reads files only for context
//! blocks. The Rust port implements search **in-process** via the `FileSystem`
//! trait + the `regex` crate, so the tool works against any `ExecutionEnv`
//! (including `InMemoryExecutionEnv`, which has no shell). Traversal uses the
//! ported pi-walker: parallel directory listing, `.ignore`/`.gitignore`/
//! `.git/info/exclude`/global excludes, and `.git`/`node_modules` pruning.
//! Files are scanned in path order, so output stays deterministic; PCRE2-only
//! syntax (lookaround, backreferences) is not supported. Output shape,
//! match/context line formats, per-line + byte truncation, and the match-limit
//! semantics match TS exactly.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;
use pure_agent::agent_tool::AgentTool;
use pure_agent::error::AgentError;
use pure_agent::types::{AgentToolResult, TextContentOrImage, ToolResultPartial};
use pure_ai::types::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::env::ExecutionEnv;
use crate::env::FileKind;
use crate::path_utils::resolve_read_tool_path;
use crate::tools::tool_context::ExecutionToolContext;
use crate::truncate::{
    format_size, truncate_head, truncate_line_default, TruncationOptions, TruncationResult,
    DEFAULT_MAX_BYTES, GREP_MAX_LINE_LENGTH,
};
use crate::walker::{self, Action};

/// Default max matches. Mirrors TS `grep.ts::DEFAULT_LIMIT`.
const DEFAULT_LIMIT: u32 = 100;

/// Input for the grep tool. Mirrors TS `GrepToolInput`. `camelCase` field names
/// match the TS wire schema (so `ignoreCase`/`ignore_case` deserialization lines
/// up with what the model emits).
#[derive(Debug, Clone, JsonSchema, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrepInput {
    /// Search pattern (regex or literal string).
    pub pattern: String,
    /// Directory or file to search (default: current directory).
    #[serde(default)]
    pub path: Option<String>,
    /// Filter files by glob pattern, e.g. `*.ts` or `**/*.spec.ts`.
    #[serde(default)]
    pub glob: Option<String>,
    /// Case-insensitive search (default false).
    #[serde(default)]
    pub ignore_case: Option<bool>,
    /// Treat pattern as a literal string instead of regex (default false).
    #[serde(default)]
    pub literal: Option<bool>,
    /// Number of lines to show before and after each match (default 0).
    #[serde(default)]
    pub context: Option<u32>,
    /// Maximum number of matches to return (default 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Structured details for the grep tool. Mirrors TS `GrepToolDetails`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GrepToolDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<TruncationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_limit_reached: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines_truncated: Option<bool>,
}

/// Options for `create_grep_tool`. The TS tool carries an optional `operations`
/// bag for remote backends; the Rust port always uses the `FileSystem` trait, so
/// this is an empty forward-compat seam (parity with `ReadToolOptions`).
#[derive(Debug, Clone, Default)]
pub struct GrepToolOptions {}

/// The built-in `grep` tool. Holds an `Arc<dyn ExecutionEnv>` (read-only view).
pub struct GrepTool {
    schema: Tool,
    env: Arc<dyn ExecutionEnv>,
}

impl GrepTool {
    fn schema() -> Tool {
        let params = schemars::schema_for!(GrepInput);
        Tool {
            name: "grep".to_string(),
            description: format!(
                "Search file contents for a pattern. Returns matching lines with file paths and \
                 line numbers. Skips `.git/` directories. Output is truncated to {DEFAULT_LIMIT} \
                 matches or {kb}KB (whichever is hit first). Long lines are truncated to \
                 {GREP_MAX_LINE_LENGTH} chars.",
                kb = DEFAULT_MAX_BYTES / 1024
            ),
            parameters: pure_ai::types::Schema::new(
                serde_json::to_value(params).unwrap_or_default(),
            ),
            constrained_sampling: None,
        }
    }
}

/// Build the `grep` tool from a context. Mirrors TS `createGrepTool`.
pub fn create_grep_tool(
    context: &ExecutionToolContext,
    options: Option<GrepToolOptions>,
) -> Arc<dyn AgentTool> {
    let _ = options; // forward-compat seam, no surface in v1
    Arc::new(GrepTool {
        schema: GrepTool::schema(),
        env: context.env().clone(),
    })
}

#[async_trait]
impl AgentTool for GrepTool {
    fn schema(&self) -> &Tool {
        &self.schema
    }
    fn label(&self) -> &str {
        "grep"
    }

    async fn execute(
        &self,
        _tool_call_id: &str,
        params: serde_json::Value,
        signal: CancellationToken,
        _on_update: Arc<dyn Fn(ToolResultPartial) + Send + Sync>,
    ) -> Result<AgentToolResult, AgentError> {
        let input: GrepInput = serde_json::from_value(params)
            .map_err(|e| AgentError::Validation(format!("grep input invalid: {e}")))?;

        let cancel = Some(&signal);
        let effective_limit = input.limit.unwrap_or(DEFAULT_LIMIT).max(1) as usize;
        let context_val = input.context.unwrap_or(0).min(u32::MAX) as usize;

        // Compile the regex. `literal` ⇒ escape; `ignore_case` ⇒ case-insensitive.
        let pattern = if input.literal.unwrap_or(false) {
            regex::escape(&input.pattern)
        } else {
            input.pattern.clone()
        };
        let re = regex::RegexBuilder::new(&pattern)
            .case_insensitive(input.ignore_case.unwrap_or(false))
            .build()
            .map_err(|e| AgentError::Validation(format!("grep pattern invalid: {e}")))?;

        // Compile the optional glob filter. `has_separator` decides whether to
        // match the full relative path (`**/*.ts`) or just the basename (`*.ts`),
        // mirroring rg's basename-glob leniency.
        let glob: Option<(globset::GlobMatcher, bool)> = match input.glob.as_deref() {
            Some(g) if !g.is_empty() => {
                let compiled = globset::Glob::new(g)
                    .map_err(|e| AgentError::Validation(format!("grep glob invalid: {e}")))?
                    .compile_matcher();
                Some((compiled, g.contains('/')))
            }
            _ => None,
        };

        // Resolve the search path. A `.`/absent path resolves to the env cwd.
        let search_path =
            resolve_read_tool_path(&*self.env, input.path.as_deref().unwrap_or("."), cancel)
                .await
                .map_err(file_err_to_agent)?;

        // Determine dir vs file (following a symlinked root via canonical_path).
        let info = self
            .env
            .file_info(&search_path, cancel)
            .await
            .map_err(file_err_to_agent)?;
        let root_kind = if info.kind == FileKind::Symlink {
            match self.env.canonical_path(&search_path, cancel).await {
                Ok(canon) => match self.env.file_info(&canon.to_string_lossy(), cancel).await {
                    Ok(i) => i.kind,
                    Err(_) => info.kind, // treat as whatever lstat said
                },
                Err(_) => info.kind,
            }
        } else {
            info.kind
        };

        // Mutable collection state shared across the file-search helper.
        let mut matches: usize = 0;
        let mut match_limit_reached = false;
        let mut lines_truncated = false;
        let mut output_lines: Vec<String> = Vec::new();

        if root_kind == FileKind::Directory {
            // gitignore-aware parallel walk (walker::collect sorts by path, so
            // output stays deterministic).
            let fs = walker::Fs::new(self.env.clone(), Some(signal.clone()));
            let opts = walker::Options {
                hidden: true,
                gitignore: true,
                skip_git: true,
                skip_node_modules: true,
                ..Default::default()
            };
            let glob_filter = glob.clone();
            let collected = walker::collect(
                &fs,
                Path::new(&search_path),
                &opts,
                true,
                move |_, entry| {
                    if entry.file_type != walker::FileType::File {
                        return Action::Skip;
                    }
                    if let Some((matcher, has_sep)) = glob_filter.as_ref() {
                        let rel = entry.path_lossy();
                        let basename = String::from_utf8_lossy(entry.name());
                        let passes = if *has_sep {
                            matcher.is_match(rel.as_ref())
                        } else {
                            matcher.is_match(basename.as_ref())
                        };
                        if !passes {
                            return Action::Skip;
                        }
                    }
                    Action::Continue
                },
            )
            .await
            .map_err(|e| AgentError::Tool(e.to_string()))?;

            let files: Vec<(String, String)> = collected
                .iter()
                .map(|entry| {
                    let abs = fs.join(Path::new(&search_path), entry.path);
                    (
                        abs.to_string_lossy().into_owned(),
                        entry.path_lossy().into_owned(),
                    )
                })
                .collect();

            // Scan in path-ordered windows so files are read concurrently while
            // the output and the match budget stay deterministic.
            let workers = fs.workers();
            let mut files = files.into_iter().peekable();
            'search: while files.peek().is_some() {
                let remaining = effective_limit.saturating_sub(matches);
                let chunk: Vec<_> = files.by_ref().take(workers).collect();
                let results = join_all(chunk.iter().map(|(abs, rel)| {
                    search_file(
                        &*self.env,
                        cancel,
                        &re,
                        None,
                        context_val,
                        remaining,
                        abs,
                        rel,
                    )
                }))
                .await;

                for result in results {
                    // Mirrors the serial loop: reaching the budget is noticed
                    // when the next file starts, before any of its lines.
                    if matches >= effective_limit {
                        match_limit_reached = true;
                        break 'search;
                    }
                    let result = result?;
                    for block in result.blocks {
                        if matches >= effective_limit {
                            match_limit_reached = true;
                            break 'search;
                        }
                        lines_truncated |= block.truncated;
                        matches += 1;
                        output_lines.extend(block.lines);
                    }
                    if result.limit_reached {
                        match_limit_reached = true;
                        break 'search;
                    }
                }
                if matches >= effective_limit && files.peek().is_some() {
                    match_limit_reached = true;
                    break;
                }
            }
        } else if root_kind == FileKind::File {
            let rel = basename(&search_path);
            let found = search_file(
                &*self.env,
                cancel,
                &re,
                glob.as_ref(),
                context_val,
                effective_limit,
                &search_path,
                &rel,
            )
            .await?;
            for block in found.blocks {
                if matches >= effective_limit {
                    match_limit_reached = true;
                    break;
                }
                lines_truncated |= block.truncated;
                matches += 1;
                output_lines.extend(block.lines);
            }
            if found.limit_reached {
                match_limit_reached = true;
            }
        } else {
            // Symlink root that didn't resolve to a file/dir (or not_supported).
            return Err(AgentError::Tool(format!(
                "Path is neither a file nor a directory: {search_path}"
            )));
        }

        if output_lines.is_empty() {
            return Ok(AgentToolResult {
                content: vec![TextContentOrImage::text("No matches found")],
                details: serde_json::Value::Null,
                ..Default::default()
            });
        }

        let raw_output = output_lines.join("\n");
        // Byte cap only — line cap disabled (match count already capped).
        let truncation = truncate_head(
            &raw_output,
            TruncationOptions {
                max_lines: Some(usize::MAX),
                max_bytes: None,
            },
        );
        let was_truncated = truncation.truncated;
        let mut output = truncation.content.clone();
        let mut details = GrepToolDetails {
            truncation: None,
            match_limit_reached: None,
            lines_truncated: None,
        };
        let mut notices: Vec<String> = Vec::new();
        if match_limit_reached {
            notices.push(format!(
                "{effective_limit} matches limit reached. Use limit={} for more, or refine pattern",
                effective_limit * 2
            ));
            details.match_limit_reached = Some(effective_limit as u32);
        }
        if was_truncated {
            notices.push(format!("{} limit reached", format_size(DEFAULT_MAX_BYTES)));
            details.truncation = Some(truncation);
        }
        if lines_truncated {
            notices.push(format!(
                "Some lines truncated to {GREP_MAX_LINE_LENGTH} chars. Use read tool to see full lines"
            ));
            details.lines_truncated = Some(true);
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

/// The formatted output of one match (its line alone, or its context block).
struct FileBlock {
    lines: Vec<String>,
    truncated: bool,
}

/// One file's matches, collected without shared mutable state so files can be
/// searched concurrently.
#[derive(Default)]
struct FileSearch {
    blocks: Vec<FileBlock>,
    /// The per-file cap was reached. Whether that also means the global limit
    /// was hit depends on the remaining budget at merge time.
    limit_reached: bool,
}

/// Search a single file for `re`, returning up to `match_cap` formatted match
/// blocks. Reads via the `FileSystem` trait; an unreadable file (non-UTF-8,
/// etc.) is silently skipped (a clean divergence — in-process search can't
/// match lines it can't decode).
#[allow(clippy::too_many_arguments)]
async fn search_file(
    env: &dyn ExecutionEnv,
    cancel: Option<&CancellationToken>,
    re: &regex::Regex,
    glob: Option<&(globset::GlobMatcher, bool)>,
    context_val: usize,
    match_cap: usize,
    abs: &str,
    rel: &str,
) -> Result<FileSearch, AgentError> {
    // Glob filter (basename vs full-relative-path per `has_separator`).
    if let Some((matcher, has_sep)) = glob {
        let basename = basename(abs);
        let passes = if *has_sep {
            matcher.is_match(rel)
        } else {
            matcher.is_match(&basename)
        };
        if !passes {
            return Ok(FileSearch::default());
        }
    }

    // Read; skip on failure (non-UTF-8 / unreadable).
    let text = match env.read_text_file(abs, cancel).await {
        Ok(t) => t,
        Err(_) => return Ok(FileSearch::default()),
    };
    // Normalize line endings, split on `\n` (mirror TS getFileLines).
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();

    let mut found = FileSearch::default();
    for (idx, line) in lines.iter().enumerate() {
        if found.blocks.len() >= match_cap {
            found.limit_reached = true;
            return Ok(found);
        }
        if !re.is_match(line) {
            continue;
        }
        let line_number = idx + 1; // 1-indexed.
        let mut block = FileBlock {
            lines: Vec::new(),
            truncated: false,
        };

        if context_val == 0 {
            // No context: emit the single match line (truncated).
            let out = truncate_line_default(line);
            block.truncated |= out.was_truncated;
            block
                .lines
                .push(format!("{rel}:{line_number}: {}", out.text));
        } else {
            // Context block: match line uses `:`, context lines use `-`.
            let start = line_number.saturating_sub(context_val).max(1);
            let end = (line_number + context_val).min(lines.len());
            for current in start..=end {
                let text_line = lines.get(current - 1).copied().unwrap_or("");
                let out = truncate_line_default(text_line);
                block.truncated |= out.was_truncated;
                if current == line_number {
                    block.lines.push(format!("{rel}:{current}: {}", out.text));
                } else {
                    block.lines.push(format!("{rel}-{current}- {}", out.text));
                }
            }
        }
        found.blocks.push(block);
    }
    Ok(found)
}

/// Filename component of a path string (POSIX-agnostic: splits on `/` and `\`).
fn basename(p: &str) -> String {
    Path::new(p)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

/// Map a `FileError` into an `AgentError::Tool` (same shape as `read.rs`).
fn file_err_to_agent(e: crate::error::FileError) -> AgentError {
    AgentError::Tool(e.to_string())
}
