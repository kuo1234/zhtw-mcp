//! Validating and parsing the arguments of the `zhtw` tool.
//!
//! Every value a client sends arrives here first: an unknown field, a wrong
//! JSON type or an unrecognized enum spelling becomes an `ErrorData` before
//! any scanning starts, so a typo is an error rather than a silent default.

use rmcp::ErrorData;
use serde_json::Value;

use super::schema::input_schema_properties;
use super::*;

/// Result of parsing a request or tool argument.
///
/// The error side is RMCP's own, because that is what the adapter hands back
/// and nothing between here and the wire adds to it. It carries the JSON-RPC
/// code, the message, and the structured data clients render diagnostics from.
/// Which request id the error correlates to is RMCP's business, not this
/// layer's, which is why none of these helpers take one.
pub(crate) type ParamResult<T> = Result<T, ErrorData>;

/// Return an INVALID_PARAMS JSON-RPC error if `args` contains keys not in
/// the known parameter set. Returns `None` when all keys are recognized.
pub(super) fn reject_unknown_params(args: &Value) -> Option<ErrorData> {
    let obj = args.as_object()?;
    let known = input_schema_properties();
    let unexpected: Vec<&str> = obj
        .keys()
        .filter(|k| !known.contains_key(k.as_str()))
        .map(String::as_str)
        .collect();
    if unexpected.is_empty() {
        return None;
    }
    Some(ErrorData::invalid_params(
        format!(
            "unknown parameter{}: {}",
            if unexpected.len() > 1 { "s" } else { "" },
            unexpected.join(", "),
        ),
        Some(json!({ "unexpected": unexpected })),
    ))
}

/// The values the schema declares for an enum-valued parameter.
///
/// Empty for a parameter the schema does not constrain to a list, which is
/// what `param_error` is for.
pub(super) fn accepted_values(field: &str) -> Vec<&'static str> {
    let Some(prop) = input_schema_properties().get(field) else {
        return Vec::new();
    };

    // An array parameter carries its enum on the item schema: the array itself
    // has no fixed set of values, its entries do. Reading only the top level
    // would hand the client an empty accepted list for such a field.
    prop.get("enum")
        .or_else(|| prop.get("items").and_then(|items| items.get("enum")))
        .and_then(|values| values.as_array())
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Reject a value the schema does not allow, naming what it does.
///
/// The list comes from the schema rather than from the call site: stating it
/// twice is how a value gets added to what the tool advertises and still
/// rejected by what parses it.
pub(super) fn enum_param_error(field: &str, value: &str) -> ErrorData {
    param_error(field, value, &accepted_values(field))
}

/// Build a structured INVALID_PARAMS JSON-RPC error for a bad tool parameter.
/// The `data` field carries `{"field", "value", "accepted"}` so clients can
/// render actionable diagnostics without parsing the message string.
pub(super) fn param_error(field: &str, value: &str, accepted: &[&str]) -> ErrorData {
    ErrorData::invalid_params(
        format!("invalid '{field}': '{value}'"),
        Some(json!({ "field": field, "value": value, "accepted": accepted })),
    )
}

/// Extract a required string field from a JSON object, returning a
/// structured INVALID_PARAMS error on failure. Distinguishes missing
/// field from present-but-wrong-type so clients get actionable diagnostics.
pub(super) fn require_str_validated<'a>(args: &'a Value, field: &str) -> ParamResult<&'a str> {
    match args.get(field) {
        None => Err(ErrorData::invalid_params(
            format!("missing required parameter '{field}'"),
            Some(json!({ "field": field })),
        )),
        Some(v) => v.as_str().ok_or_else(|| {
            let type_name = json_type_name(v);
            ErrorData::invalid_params(
                format!("'{field}' must be a string, got {type_name}"),
                Some(
                    json!({ "field": field, "expected_type": "string", "actual_type": type_name }),
                ),
            )
        }),
    }
}

/// Extract an optional string field, returning INVALID_PARAMS if the
/// value is present but not a string. Returns `Ok(None)` when absent.
fn optional_str_validated<'a>(args: &'a Value, field: &str) -> ParamResult<Option<&'a str>> {
    match args.get(field) {
        None => Ok(None),
        Some(v) => match v.as_str() {
            Some(s) => Ok(Some(s)),
            None => {
                let type_name = json_type_name(v);
                Err(ErrorData::invalid_params(
                    format!("'{field}' must be a string, got {type_name}"),
                    Some(
                        json!({ "field": field, "expected_type": "string", "actual_type": type_name }),
                    ),
                ))
            }
        },
    }
}

/// Human-readable JSON type name for error diagnostics.
pub(super) fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
        Value::String(_) => "string",
    }
}

/// Everything `tool_check` reads out of its JSON arguments.
///
/// Exists to give the parsing a name and a home, not to be passed around: the
/// caller destructures it immediately, so the body works with the same locals
/// it always did.  That is deliberate.  A struct threaded through four hundred
/// lines would have meant renaming every use, which is a lot of silent risk for
/// a change whose whole point is legibility.
pub(super) struct CheckParams<'a> {
    pub(super) fix_mode: FixMode,
    pub(super) profile: Profile,
    pub(super) content_type: ContentType,
    pub(super) stance: Option<PoliticalStance>,
    pub(super) max_errors: Option<u64>,
    pub(super) max_warnings: Option<u64>,
    pub(super) ignore_terms: Vec<String>,
    pub(super) explain: bool,
    pub(super) output_mode: OutputMode,
    pub(super) fix_output: FixOutputMode,
    #[cfg(feature = "translate")]
    pub(super) verify: bool,
    /// Explicit bool overrides the profile default; absent means inherit.  The
    /// default profile enables both AI-filler and translationese detection.
    pub(super) detect_ai_opt: Option<bool>,
    pub(super) detect_translationese_opt: Option<bool>,
    /// Composite three-axis scorecard, opt-in.  Mirrors the CLI
    /// `--detect-style` shorthand; off by default to keep the payload lean.
    pub(super) detect_style: bool,
    pub(super) translationese_domain_opt: Option<String>,
    pub(super) document_genre_opt: Option<String>,
    pub(super) register_opt: Option<String>,
    pub(super) ai_threshold: Option<&'a str>,
    pub(super) relaxed: bool,
    pub(super) exempt_blockquotes: bool,
    /// Advisory rhythm (氣口) axis. Opt-in and never fixable, exactly as on
    /// the CLI: the tool exposes it so an agent can ask for the same advice a
    /// human gets from --rhythm.
    pub(super) rhythm: bool,
    /// CJK/Latin and CJK/digit boundary policy, validated into a
    /// SpacingPolicy once the config is built.
    pub(super) spacing: Option<&'a str>,
    pub(super) off: Vec<crate::rules::ruleset::RuleFamily>,
    pub(super) glossary: crate::rules::glossary::ProjectGlossary,
    pub(super) consistency_requested: bool,
    pub(super) include_telemetry: bool,
    pub(super) include_stats: bool,
}

impl<'a> CheckParams<'a> {
    pub(super) fn parse(args: &'a Value, default_output: OutputMode) -> ParamResult<Self> {
        Ok(Self {
            fix_mode: parse_fix_mode(args)?,
            profile: parse_profile(args)?,
            content_type: parse_content_type(args)?,
            stance: parse_political_stance(args)?,
            max_errors: args.get("max_errors").and_then(|v| v.as_u64()),
            max_warnings: args.get("max_warnings").and_then(|v| v.as_u64()),
            ignore_terms: parse_ignore_terms(args),
            explain: parse_explain(args),
            output_mode: parse_output_mode(args, default_output)?,
            fix_output: parse_fix_output(args)?,
            #[cfg(feature = "translate")]
            verify: parse_verify(args),
            detect_ai_opt: parse_flag_opt(args, "detect_ai"),
            detect_translationese_opt: parse_flag_opt(args, "detect_translationese"),
            detect_style: parse_flag(args, "detect_style"),
            translationese_domain_opt: args
                .get("translationese_domain")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            document_genre_opt: optional_str_validated(args, "document_genre")?.map(str::to_string),
            register_opt: optional_str_validated(args, "register")?.map(str::to_string),
            ai_threshold: optional_str_validated(args, "ai_threshold")?,
            relaxed: parse_flag(args, "relaxed"),
            exempt_blockquotes: parse_flag(args, "exempt_blockquotes"),
            rhythm: parse_flag(args, "rhythm"),
            spacing: optional_str_validated(args, "spacing")?,
            off: parse_off(args)?,
            glossary: parse_glossary(args),
            consistency_requested: parse_flag(args, "consistency"),
            include_telemetry: parse_flag(args, "include_telemetry"),
            include_stats: parse_flag(args, "include_stats"),
        })
    }
}

/// An optional boolean argument, absent meaning "inherit the default".
fn parse_flag_opt(args: &Value, field: &str) -> Option<bool> {
    args.get(field).and_then(|v| v.as_bool())
}

/// A boolean argument that defaults to false when absent or malformed.
fn parse_flag(args: &Value, field: &str) -> bool {
    parse_flag_opt(args, field).unwrap_or(false)
}

/// Parse public rule-family subtractions.  The schema guides regular clients,
/// but this remains strict because direct JSON-RPC calls bypass it.
pub(super) fn parse_off(args: &Value) -> ParamResult<Vec<crate::rules::ruleset::RuleFamily>> {
    let Some(value) = args.get("off") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| {
        ErrorData::invalid_params(
            "'off' must be an array of rule family names",
            Some(json!({ "field": "off", "expected_type": "array" })),
        )
    })?;
    // Repeats are harmless: with_disabled only clears flags.
    values
        .iter()
        .map(|entry| {
            let name = entry.as_str().ok_or_else(|| {
                ErrorData::invalid_params(
                    "'off' entries must be strings",
                    Some(json!({ "field": "off", "expected_type": "string" })),
                )
            })?;
            crate::rules::ruleset::RuleFamily::from_str_strict(name)
                .ok_or_else(|| enum_param_error("off", name))
        })
        .collect()
}

/// Parse the optional "fix_mode" field from tool arguments.
/// Returns an INVALID_PARAMS error for unrecognized values.
pub(super) fn parse_fix_mode(args: &Value) -> ParamResult<FixMode> {
    match optional_str_validated(args, "fix_mode")? {
        Some("orthographic") => Ok(FixMode::Orthographic),
        Some("lexical_safe") => Ok(FixMode::LexicalSafe),
        Some("lexical_contextual") => Ok(FixMode::LexicalContextual),
        None | Some("none") => Ok(FixMode::None),
        Some(other) => Err(enum_param_error("fix_mode", other)),
    }
}

/// Parse the optional "content_type" field from tool arguments.
/// Returns an INVALID_PARAMS error for unrecognized values.
pub(super) fn parse_content_type(args: &Value) -> ParamResult<ContentType> {
    match optional_str_validated(args, "content_type")? {
        // Plain, not the file-name guess the CLI makes: a tool call carries
        // text and no name to guess from, and reading unmarked text as Markdown
        // would skip whatever looks like a fence inside it.
        None => Ok(ContentType::Plain),
        Some(other) => {
            ContentType::from_name(other).ok_or_else(|| enum_param_error("content_type", other))
        }
    }
}

/// Parse the optional "profile" field from tool arguments.
/// Returns an INVALID_PARAMS error for unrecognized values.
pub(super) fn parse_profile(args: &Value) -> ParamResult<Profile> {
    match optional_str_validated(args, "profile")? {
        None => Ok(Profile::Base),
        Some(s) => Profile::from_str_strict(s).ok_or_else(|| enum_param_error("profile", s)),
    }
}

/// Parse the optional "political_stance" field from tool arguments.
/// Returns an INVALID_PARAMS error for unrecognized values.
pub(super) fn parse_political_stance(args: &Value) -> ParamResult<Option<PoliticalStance>> {
    match optional_str_validated(args, "political_stance")? {
        None => Ok(None),
        Some(s) => PoliticalStance::from_str_strict(s)
            .map(Some)
            .ok_or_else(|| enum_param_error("political_stance", s)),
    }
}

/// Fix output format: how corrected text is returned when fixes are applied.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FixOutputMode {
    /// Return the full corrected text (backward compat default).
    Full,
    /// Return search/replace blocks (LLM-friendly patching format).
    SearchReplace,
    /// Return a patches array with byte offsets into the original text.
    Patch,
}

impl FixOutputMode {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::SearchReplace => "search_replace",
            Self::Patch => "patch",
        }
    }
}

/// Parse the optional "fix_output" parameter from tool arguments.
pub(super) fn parse_fix_output(args: &Value) -> ParamResult<FixOutputMode> {
    match optional_str_validated(args, "fix_output")? {
        Some("full") | None => Ok(FixOutputMode::Full),
        Some("search_replace") => Ok(FixOutputMode::SearchReplace),
        Some("patch") => Ok(FixOutputMode::Patch),
        Some(other) => Err(enum_param_error("fix_output", other)),
    }
}

/// Parse the optional "explain" boolean from tool arguments.
fn parse_explain(args: &Value) -> bool {
    args.get("explain")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Output mode for zhtw responses.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputMode {
    Full,
    Compact,
    /// Header-once TSV format for LLM-facing responses.
    /// Eliminates JSON syntax tax (repeated keys, braces, quotes) that
    /// inflates BPE token count by 40-60% with zero semantic value.
    Tabular,
    /// AI summary only: issue counts + AI signature report.
    /// No individual issues, no text. Lets downstream tools quickly
    /// decide whether to trigger a full review.
    Summary,
}

/// Parse the optional "output" mode from tool arguments.
/// When no explicit value is given, uses the provided default (which may
/// be auto-detected from the client identity).
pub(super) fn parse_output_mode(args: &Value, default: OutputMode) -> ParamResult<OutputMode> {
    match optional_str_validated(args, "output")? {
        Some("compact") => Ok(OutputMode::Compact),
        Some("full") => Ok(OutputMode::Full),
        Some("tabular") => Ok(OutputMode::Tabular),
        Some("summary") => Ok(OutputMode::Summary),
        None => Ok(default),
        Some(other) => Err(enum_param_error("output", other)),
    }
}

/// Known AI agent/CLI client names that benefit from compact output.
/// Matched as exact full-name against the lowercased `clientInfo.name`.
/// Only programmatic agents/CLIs: NOT desktop GUI apps like "Claude Desktop".
const AI_AGENT_CLIENTS: &[&str] = &[
    "claude-code",
    "claude code",
    "cursor",
    "cline",
    "continue",
    "zed",
    "windsurf",
    "copilot",
    "aider",
    "cody",
    "roo",
    "roo-code",
    "roo code",
];

/// Determine default output mode from client identity.
/// Uses exact full-name match only to avoid false positives on clients
/// like "Claude Desktop" that happen to share a token with an agent name.
/// Strips trailing version suffixes (`/1.0`, ` 1.0`) before matching,
/// since some clients embed version info in the name field.
pub(super) fn default_output_mode(client_name: Option<&str>) -> OutputMode {
    match client_name {
        Some(name) => {
            let lower = name.to_ascii_lowercase();

            // Strip trailing version suffix: "Cursor/0.1.0" → "cursor", "cline
            // 1.2" → "cline"
            let base = lower
                .split('/')
                .next()
                .unwrap_or(&lower)
                .trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
                .trim();
            if AI_AGENT_CLIENTS.contains(&base) {
                OutputMode::Compact
            } else {
                OutputMode::Full
            }
        }
        None => OutputMode::Full,
    }
}

/// Parse the optional "verify" flag from tool arguments.
#[cfg(feature = "translate")]
pub(super) fn parse_verify(args: &Value) -> bool {
    args.get("verify")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Parse the optional "ignore_terms" array from tool arguments.
fn parse_ignore_terms(args: &Value) -> Vec<String> {
    args.get("ignore_terms")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse the optional `glossary` object.  Shape:
/// `{ "banned": [...], "preferred": [...], "proper_nouns": [...] }`.
/// Each field is optional.  Missing object → empty glossary.
pub(super) fn parse_glossary(args: &Value) -> crate::rules::glossary::ProjectGlossary {
    fn array_of_strings(v: Option<&Value>) -> Vec<String> {
        v.and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }
    let Some(glossary) = args.get("glossary").and_then(|v| v.as_object()) else {
        return crate::rules::glossary::ProjectGlossary::default();
    };
    crate::rules::glossary::ProjectGlossary {
        banned: array_of_strings(glossary.get("banned")),
        preferred: array_of_strings(glossary.get("preferred")),
        proper_nouns: array_of_strings(glossary.get("proper_nouns")),
    }
}
