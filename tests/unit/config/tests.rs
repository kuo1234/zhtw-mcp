use super::*;
use tempfile::TempDir;

#[test]
fn discover_finds_config_in_cwd() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join(CONFIG_FILENAME),
        "profile = \"strict\"\nmax_errors = 5\n",
    )
    .unwrap();
    let cfg = ProjectConfig::discover(dir.path()).unwrap().unwrap();
    assert_eq!(cfg.profile.as_deref(), Some("strict"));
    assert_eq!(cfg.max_errors, Some(5));
}

#[test]
fn discover_walks_upward() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("sub").join("deep");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(dir.path().join(CONFIG_FILENAME), "profile = \"base\"\n").unwrap();
    let cfg = ProjectConfig::discover(&sub).unwrap().unwrap();
    assert_eq!(cfg.profile.as_deref(), Some("base"));
}

#[test]
fn discover_stops_at_git_root() {
    let dir = TempDir::new().unwrap();
    // Place config above .git boundary.
    std::fs::write(dir.path().join(CONFIG_FILENAME), "profile = \"base\"\n").unwrap();
    let sub = dir.path().join("repo");
    std::fs::create_dir_all(sub.join(".git")).unwrap();
    let deep = sub.join("src");
    std::fs::create_dir_all(&deep).unwrap();
    // Discovery from deep should not find the config above .git.
    let cfg = ProjectConfig::discover(&deep).unwrap();
    assert!(cfg.is_none());
}

#[test]
fn discover_returns_none_when_absent() {
    let dir = TempDir::new().unwrap();
    // Create .git so we stop quickly.
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    assert!(ProjectConfig::discover(dir.path()).unwrap().is_none());
}

#[test]
fn parse_all_fields() {
    let toml = r#"
profile = "strict"
spacing = "strip"
content_type = "markdown"
max_errors = 0
max_warnings = 10
ignore_terms = ["軟件", "硬件"]
exclude = ["vendor/**", "*.tmp"]
overrides = "/path/to/overrides.json"
suppressions = "/path/to/suppressions.json"
packs = ["medical", "legal"]
off = ["punctuation", "variant"]
"#;
    let cfg: ProjectConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.profile.as_deref(), Some("strict"));
    assert_eq!(
        cfg.spacing,
        Some(crate::rules::ruleset::SpacingPolicy::Strip)
    );
    assert_eq!(cfg.content_type.as_deref(), Some("markdown"));
    assert_eq!(cfg.max_errors, Some(0));
    assert_eq!(cfg.max_warnings, Some(10));
    assert_eq!(
        cfg.suppressions.as_deref(),
        Some("/path/to/suppressions.json")
    );
    assert_eq!(cfg.ignore_terms.as_ref().unwrap().len(), 2);
    assert_eq!(cfg.exclude.as_ref().unwrap().len(), 2);
    assert_eq!(cfg.overrides.as_deref(), Some("/path/to/overrides.json"));
    assert_eq!(cfg.packs.as_ref().unwrap(), &["medical", "legal"]);
    assert_eq!(cfg.off.unwrap().len(), 2);
}

#[test]
fn config_rejects_unknown_fields_and_families() {
    assert!(toml::from_str::<ProjectConfig>("oops = true").is_err());
    assert!(toml::from_str::<ProjectConfig>("off = [\"not-a-family\"]").is_err());
    assert!(toml::from_str::<ProjectConfig>("spacing = \"off\"").is_err());
}

#[test]
fn the_config_spells_a_family_the_way_the_flag_does() {
    // The config file parses "off" through serde's rename_all, while the CLI
    // and the MCP tool parse it through RuleFamily::from_str_strict. Two
    // spellings of one name, agreeing today because every variant is one word:
    // a two-word variant added later would be accepted here under a name the
    // other two surfaces reject, and nothing else would notice.
    for family in crate::rules::ruleset::RuleFamily::ALL {
        let toml_text = format!("off = [\"{}\"]", family.name());
        let cfg: ProjectConfig = toml::from_str(&toml_text).unwrap_or_else(|e| {
            panic!("config rejects the flag spelling '{}': {e}", family.name())
        });
        assert_eq!(cfg.off.as_deref(), Some(&[*family][..]));
    }
}

#[test]
fn parse_empty_config() {
    let cfg: ProjectConfig = toml::from_str("").unwrap();
    assert!(cfg.profile.is_none());
    assert!(cfg.max_errors.is_none());
}
