use super::*;
use zhtw_mcp::rules::ruleset::{IssueType, Severity};

fn issue(found: &str, suggestion: &str, line: usize, col: usize) -> Issue {
    let mut issue = Issue::new(
        0,
        found.len(),
        found,
        vec![suggestion.to_string()],
        IssueType::CrossStrait,
        Severity::Error,
    );
    issue.line = line;
    issue.col = col;
    issue
}

#[test]
fn payload_parsing_extracts_the_two_fields_and_rejects_the_rest() {
    let payload = r#"{
        "session_id": "s", "cwd": "/w", "hook_event_name": "PostToolUse",
        "tool_name": "Write",
        "tool_input": {"file_path": "/tmp/a.md", "content": "x"},
        "tool_response": {"ok": true}
    }"#;
    let parsed = parse_payload(payload).expect("realistic payload should parse");
    assert_eq!(parsed.tool_name, "Write");
    assert_eq!(parsed.file_path, "/tmp/a.md");

    assert!(parse_payload("not json").is_none());
    assert!(parse_payload(r#"{"tool_name": "Write"}"#).is_none());
    assert!(parse_payload(r#"{"tool_name": "Write", "tool_input": {}}"#).is_none());
    assert!(parse_payload(r#"{"tool_name": "Write", "tool_input": {"file_path": 3}}"#).is_none());
}

#[test]
fn extension_gate_is_case_insensitive_and_narrow() {
    for path in ["a.md", "b.markdown", "c.txt", "d.yml", "e.yaml", "F.MD"] {
        assert!(lintable_extension(path), "{path} should be lintable");
    }
    for path in ["a.rs", "b.json", "c", "d.md.bak", "md"] {
        assert!(!lintable_extension(path), "{path} should be skipped");
    }
}

#[test]
fn issue_digest_ignores_positions_but_counts_occurrences() {
    let a = [issue("軟件", "軟體", 1, 2), issue("質量", "品質", 3, 4)];
    let moved = [issue("質量", "品質", 30, 1), issue("軟件", "軟體", 12, 9)];
    // Same multiset, different positions and order: same digest.
    assert_eq!(issues_digest(&a), issues_digest(&moved));

    let duplicated = [
        issue("軟件", "軟體", 1, 2),
        issue("軟件", "軟體", 5, 2),
        issue("質量", "品質", 3, 4),
    ];
    assert_ne!(issues_digest(&a), issues_digest(&duplicated));
    assert_ne!(issues_digest(&a), issues_digest(&[]));
}

#[test]
fn context_is_capped_at_three_issues_plus_a_tail() {
    let one = [issue("軟件", "軟體", 12, 8)];
    let text = format_context("/docs/guide.md", &one);
    assert!(text.starts_with("zhtw-mcp: 1 zh-TW issue(s) in guide.md"));
    assert!(text.contains("12:8 軟件 → 軟體"));
    assert!(!text.contains("more"));

    let five: Vec<Issue> = (1..=5).map(|n| issue("網絡", "網路", n, 1)).collect();
    let text = format_context("/docs/guide.md", &five);
    assert_eq!(text.lines().count(), 5, "summary + 3 issues + tail");
    let tail = format!("+2 more: zhtw-mcp lint {}", shell_quote("/docs/guide.md"));
    assert!(text.contains(&tail), "{text}");

    // The tail is a command to paste, so a path with a space has to come out
    // quoted for the shell that will split it.
    let text = format_context("/my docs/guide.md", &five);
    let tail = format!(
        "+2 more: zhtw-mcp lint {}",
        shell_quote("/my docs/guide.md")
    );
    assert!(text.contains(&tail), "{text}");

    let three: Vec<Issue> = (1..=3).map(|n| issue("網絡", "網路", n, 1)).collect();
    assert!(!format_context("/docs/guide.md", &three).contains("more"));
}

#[test]
fn hook_output_is_the_post_tool_use_envelope() {
    let out: Value = serde_json::from_str(&wrap_hook_output("hi")).unwrap();
    assert_eq!(out["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    assert_eq!(out["hookSpecificOutput"]["additionalContext"], "hi");
}

#[test]
fn merge_creates_the_structure_in_an_empty_settings_file() {
    let mut root = json!({});
    let modified = ensure_hook_entry(&mut root, "\"/bin/zhtw-mcp\" internal hook-callback")
        .expect("merge into empty object");
    assert!(modified);
    assert_eq!(
        root,
        json!({
            "hooks": {
                "PostToolUse": [{
                    "matcher": "Write|Edit",
                    "hooks": [{
                        "type": "command",
                        "command": "\"/bin/zhtw-mcp\" internal hook-callback",
                    }],
                }],
            }
        })
    );
}

#[test]
fn merge_preserves_unrelated_settings_and_is_idempotent() {
    let mut root = json!({
        "model": "opus",
        "hooks": {
            "PreToolUse": [{"matcher": "Bash", "hooks": []}],
            "PostToolUse": [{
                "matcher": "Bash",
                "hooks": [{"type": "command", "command": "other-tool check"}],
            }],
        },
    });
    assert!(ensure_hook_entry(&mut root, "\"/bin/zhtw-mcp\" internal hook-callback").unwrap());
    assert_eq!(root["model"], "opus");
    assert_eq!(root["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    assert_eq!(
        root["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
        "other-tool check"
    );
    assert_eq!(root["hooks"]["PostToolUse"][1]["matcher"], "Write|Edit");

    let before = root.clone();
    // A second install, even from a relocated binary, adds nothing.
    assert!(
        !ensure_hook_entry(&mut root, "\"/elsewhere/zhtw-mcp\" internal hook-callback").unwrap()
    );
    assert_eq!(root, before);
}

#[cfg(unix)]
#[test]
fn shell_quoting_suppresses_expansion_and_survives_a_quote() {
    assert_eq!(shell_quote("/opt/tools/zhtw-mcp"), "'/opt/tools/zhtw-mcp'");
    assert_eq!(
        shell_quote("/opt/my tools/zhtw-mcp"),
        "'/opt/my tools/zhtw-mcp'"
    );

    // A dollar sign stays literal inside single quotes; double quotes would
    // have let the shell expand it.
    assert_eq!(shell_quote("/opt/$X/zhtw-mcp"), "'/opt/$X/zhtw-mcp'");

    // An embedded single quote closes, escapes, and reopens.
    assert_eq!(
        shell_quote("/opt/it's/zhtw-mcp"),
        r"'/opt/it'\''s/zhtw-mcp'"
    );
}

#[cfg(windows)]
#[test]
fn shell_quoting_wraps_in_double_quotes() {
    assert_eq!(
        shell_quote(r"C:\Program Files\zhtw-mcp.exe"),
        r#""C:\Program Files\zhtw-mcp.exe""#
    );
}

#[test]
fn merge_ignores_our_command_under_a_matcher_that_never_fires() {
    // The command is present but registered under Bash only, so no Write or
    // Edit ever reaches it: that is not installed, and install must add a
    // working entry rather than report success.
    let mut root = json!({
        "hooks": {
            "PostToolUse": [{
                "matcher": "Bash",
                "hooks": [{"type": "command",
                           "command": "'/bin/zhtw-mcp' internal hook-callback"}],
            }],
        },
    });
    assert!(ensure_hook_entry(&mut root, "'/bin/zhtw-mcp' internal hook-callback").unwrap());
    assert_eq!(root["hooks"]["PostToolUse"][1]["matcher"], "Write|Edit");

    // A broader matcher that does cover both tools counts as installed,
    // whichever shape it takes.
    for matcher in [json!("Write|Edit"), json!("Edit|Write|Bash"), json!(".*")] {
        let mut root = json!({
            "hooks": {
                "PostToolUse": [{
                    "matcher": matcher,
                    "hooks": [{"type": "command",
                               "command": "'/bin/zhtw-mcp' internal hook-callback"}],
                }],
            },
        });
        assert!(!ensure_hook_entry(&mut root, "'/bin/zhtw-mcp' internal hook-callback").unwrap());
    }
}

#[test]
fn merge_refuses_shapes_it_does_not_recognize() {
    assert!(ensure_hook_entry(&mut json!([]), "c").is_err());
    assert!(ensure_hook_entry(&mut json!({"hooks": 3}), "c").is_err());
    assert!(ensure_hook_entry(&mut json!({"hooks": {"PostToolUse": {}}}), "c").is_err());
}

#[test]
fn cache_roundtrips_resets_on_schema_mismatch_and_prunes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hook-cache.json");

    let entry = HookCacheEntry {
        content_hash: "c1".into(),
        issues_hash: "i1".into(),
        fingerprint: "fp1".into(),
        timestamp_secs: 10,
    };
    HookCache::persist(&path, "/a.md", entry);
    let cache = HookCache::open(&path);
    let read_back = cache.lookup("/a.md").unwrap();
    assert_eq!(read_back.content_hash, "c1");
    assert_eq!(read_back.fingerprint, "fp1", "fingerprint roundtrips");
    assert!(cache.lookup("/b.md").is_none());

    // An entry written before the fingerprint field existed reads back with the
    // empty default, which the callback treats as never matching: the file
    // re-scans instead of trusting a verdict of unknown rules.
    std::fs::write(
        &path,
        format!(
            r#"{{"schema_version": {HOOK_CACHE_SCHEMA_VERSION}, "entries": {{"/old.md":
               {{"content_hash": "c", "issues_hash": "i", "timestamp_secs": 1}}}}}}"#
        ),
    )
    .unwrap();
    assert_eq!(
        HookCache::open(&path)
            .lookup("/old.md")
            .unwrap()
            .fingerprint,
        ""
    );

    // A future schema starts over instead of misreading old entries.
    std::fs::write(
        &path,
        r#"{"schema_version": 999, "entries": {"/a.md": {}}}"#,
    )
    .unwrap();
    assert!(HookCache::open(&path).lookup("/a.md").is_none());
    std::fs::write(&path, "not json").unwrap();
    assert!(HookCache::open(&path).lookup("/a.md").is_none());

    // Oldest entries fall out once past the cap.
    std::fs::remove_file(&path).unwrap();
    for n in 0..=HOOK_CACHE_MAX_ENTRIES {
        HookCache::persist(
            &path,
            &format!("/f{n}.md"),
            HookCacheEntry {
                content_hash: "c".into(),
                issues_hash: "i".into(),
                fingerprint: "fp1".into(),
                timestamp_secs: n as u64,
            },
        );
    }
    let cache = HookCache::open(&path);
    assert_eq!(cache.entries.len(), HOOK_CACHE_MAX_ENTRIES);
    assert!(cache.lookup("/f0.md").is_none(), "oldest entry pruned");
    assert!(cache
        .lookup(&format!("/f{HOOK_CACHE_MAX_ENTRIES}.md"))
        .is_some());
}

#[test]
fn callback_stays_silent_on_everything_but_a_lintable_write() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("hook-cache.json");
    let payload = |tool: &str, path: &str| {
        format!(r#"{{"tool_name":"{tool}","tool_input":{{"file_path":"{path}"}}}}"#)
    };

    let md = dir.path().join("clean.md");
    std::fs::write(&md, "# Title\n\nEnglish only.\n").unwrap();
    let md = md.to_string_lossy().into_owned();

    assert!(callback_inner("garbage", &cache).is_none());
    assert!(callback_inner(&payload("Read", &md), &cache).is_none());
    assert!(callback_inner(&payload("Write", "/tmp/x.rs"), &cache).is_none());
    assert!(callback_inner(&payload("Write", "/no/such/file.md"), &cache).is_none());
    assert!(
        callback_inner(&payload("Write", &md), &cache).is_none(),
        "clean file"
    );
}

#[test]
fn project_config_layers_every_axis_the_lint_front_end_reads() {
    // A hook that ignored these would nag about exactly what the project has
    // already told the linter to keep quiet about.
    let toml = r#"
profile = "strict"
spacing = "strip"
relaxed = true
off = ["quotes"]

[markdown]
exempt_blockquotes = true
"#;
    let project: zhtw_mcp::config::ProjectConfig = toml::from_str(toml).unwrap();
    let cfg = project_config(Some(&project));
    assert!(cfg.variant_normalization, "profile = strict was not read");
    assert_eq!(
        cfg.spacing_policy,
        zhtw_mcp::rules::ruleset::SpacingPolicy::Strip
    );
    assert!(!cfg.colon_enforcement, "relaxed was not applied");
    assert!(!cfg.quotes, "off was not applied");
    assert!(cfg.exempt_blockquotes, "[markdown] section was not read");

    // No config file is the base profile untouched.
    assert_eq!(
        format!("{:?}", project_config(None)),
        format!("{:?}", Profile::Base.config()),
    );
}

#[test]
fn hook_honors_project_content_type_over_file_extension() {
    let project: zhtw_mcp::config::ProjectConfig =
        toml::from_str("content_type = \"plain\"").unwrap();
    let content = "    這個軟件很好用\n";
    let glossary = zhtw_mcp::rules::glossary::ProjectGlossary::default();

    let markdown_issues = scan_file(content, "document.md", None, &glossary, None, None);
    let plain_issues = scan_file(
        content,
        "document.md",
        None,
        &glossary,
        None,
        Some(&project),
    );

    assert!(
        markdown_issues.is_empty(),
        "Markdown excludes indented code"
    );
    assert!(
        plain_issues.iter().any(|issue| issue.found == "軟件"),
        "project content_type = plain must override the .md extension"
    );
}

#[test]
fn the_fingerprint_moves_when_a_configured_pack_changes() {
    // The hook now merges the packs the project config names, so a pack is part
    // of what decides the verdict and has to be part of what decides whether a
    // cached verdict still stands. Before packs were active the fingerprint
    // could ignore them; now it cannot.
    let dir = tempfile::tempdir().unwrap();
    let overrides = dir.path().join("overrides.json");
    let tm = dir.path().join("tm.json");
    let pack = dir.path().join("probe.json");
    std::fs::write(&pack, r#"{"schema_version":3,"spelling":[]}"#).unwrap();

    let before = rules_fingerprint(&overrides, None, &tm, std::slice::from_ref(&pack));
    std::fs::write(&pack, r#"{"schema_version":3,"spelling":[{"from":"x"}]}"#).unwrap();
    let after = rules_fingerprint(&overrides, None, &tm, std::slice::from_ref(&pack));
    assert_ne!(before, after, "editing a configured pack must re-scan");

    // A pack the config does not name changes nothing.
    let unrelated = rules_fingerprint(&overrides, None, &tm, &[]);
    assert_ne!(unrelated, after);
    assert_eq!(unrelated, rules_fingerprint(&overrides, None, &tm, &[]));
}
