// Project config file support (.zhtw-mcp.toml).
//
// Discovery: resolve once from cwd upward, stopping at VCS root (.git) or
// filesystem root. Apply globally to all files in the run. CLI flags override
// config file values. Config file overrides defaults.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::rules::ruleset::RuleFamily;

/// Config file name.
const CONFIG_FILENAME: &str = ".zhtw-mcp.toml";

/// Parsed project config.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    pub profile: Option<String>,
    pub spacing: Option<crate::rules::ruleset::SpacingPolicy>,
    pub relaxed: Option<bool>,
    pub content_type: Option<String>,
    pub max_errors: Option<usize>,
    pub max_warnings: Option<usize>,
    pub ignore_terms: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub overrides: Option<String>,
    pub suppressions: Option<String>,
    pub packs: Option<Vec<String>>,
    pub translation_memory: Option<String>,
    pub off: Option<Vec<RuleFamily>>,
    pub markdown: Option<MarkdownConfig>,
    pub glossary: Option<GlossaryConfig>,
}

/// Markdown-specific scanning options.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct MarkdownConfig {
    /// When true, treat pulldown-cmark `Tag::BlockQuote` ranges as
    /// exclusion zones.  Useful for documents that quote mainland-Chinese
    /// sources for illustrative purposes.  Off by default.
    pub exempt_blockquotes: Option<bool>,
}

/// Project glossary section.  Layered above the embedded ruleset
/// and pack store but below banned-term enforcement and translation
/// memory.  Precedence: glossary `banned` > TM > glossary `preferred` >
/// domain pack > embedded ruleset.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct GlossaryConfig {
    /// Terms that must always be flagged regardless of context clues.
    /// E.g. ["線程", "內存"] forces those calques to fire even in
    /// otherwise ambiguous prose.
    pub banned: Option<Vec<String>>,
    /// Project-preferred zh-TW forms.  Used by the consistency report
    /// to choose the canonical suggestion when both TW-preferred
    /// and CN-preferred variants appear in the same document.
    pub preferred: Option<Vec<String>>,
    /// Names that should never be flagged (added to the suppression
    /// list).  E.g. ["TSMC", "MediaTek"].
    pub proper_nouns: Option<Vec<String>>,
}

impl ProjectConfig {
    /// Discover and parse the nearest .zhtw-mcp.toml.
    ///
    /// Walks from start_dir upward, stopping at a .git directory or
    /// filesystem root.  Returns None if no config file is found.
    pub fn discover(start_dir: &Path) -> anyhow::Result<Option<Self>> {
        let Some(path) = find_config_file(start_dir) else {
            return Ok(None);
        };
        let cfg = Self::from_file(&path)?;
        tracing::info!("loaded config from {}", path.display());
        Ok(Some(cfg))
    }

    /// Load from an explicit path (--config flag).
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("read config {}: {}", path.display(), e))?;
        toml::from_str::<ProjectConfig>(&content)
            .map_err(|e| anyhow::anyhow!("parse config {}: {}", path.display(), e))
    }
}

/// Walk from start upward looking for .zhtw-mcp.toml.
/// Stop at .git directory or filesystem root.
///
/// Public so a caller that needs the file itself, not just the parsed
/// config, resolves it through the same walk `discover` uses: the hook
/// callback hashes the bytes into its cache fingerprint.
pub fn find_config_file(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join(CONFIG_FILENAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        // Stop at VCS root.
        if dir.join(".git").exists() {
            return None;
        }
        // Move to parent.
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/config/tests.rs"]
mod tests;
