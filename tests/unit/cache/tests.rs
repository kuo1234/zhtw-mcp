use super::*;
use crate::engine::scan::ScanOutput;
use crate::engine::zhtype::ChineseType;
use tempfile::TempDir;

/// A subject carrying placeholder file facts, for the tests whose subject is
/// the cache key rather than the file. What a test actually varies it names,
/// through struct update syntax, so the distinctive value is the one on the
/// page.
fn subject(file_path: &str) -> CacheSubject<'_> {
    CacheSubject {
        file_path,
        content: b"x",
        mtime_secs: 100,
        size: 1,
        input_was_sc: false,
        text_char_count: 1,
    }
}

/// A subject whose content is what the test is about, with `size` and
/// `text_char_count` derived from it rather than restated. The four call sites
/// that spelled all three out kept them consistent by hand.
fn subject_with<'a>(file_path: &'a str, content: &'a [u8], mtime_secs: u64) -> CacheSubject<'a> {
    CacheSubject {
        content,
        mtime_secs,
        size: content.len() as u64,
        text_char_count: content.len(),
        ..subject(file_path)
    }
}

fn empty_output() -> ScanOutput {
    ScanOutput {
        issues: vec![],
        detected_script: ChineseType::Traditional,
        ai_signature: None,
        translationese_signature: None,
        coverage: None,
        oral_density: None,
        quality_flags: Vec::new(),
    }
}

fn test_params() -> ScanParams {
    ScanParams {
        ruleset_hash: "rh".into(),
        profile: "base".into(),
        content_type: "md".into(),
        fix_mode: "none".into(),
        detect_ai: false,
        detect_translationese: false,
        translationese_domain: "general".into(),
        ai_threshold: "1.0".into(),
        exempt_blockquotes: false,
        engine_version: "test".into(),
    }
}

fn test_params_plain() -> ScanParams {
    ScanParams {
        ruleset_hash: "rh".into(),
        profile: "base".into(),
        content_type: "plain".into(),
        fix_mode: "none".into(),
        detect_ai: false,
        detect_translationese: false,
        translationese_domain: "general".into(),
        ai_threshold: "1.0".into(),
        exempt_blockquotes: false,
        engine_version: "test".into(),
    }
}

/// Put `base` into a fresh cache, confirm it hits, then confirm `variant`
/// misses on both the fast and the content path. Lookup never compares the
/// stored params, so a field absent from the key shows up here as the
/// variant hitting the base entry.
fn assert_variant_misses(base: &ScanParams, variant: &ScanParams) {
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));

    cache.put(subject_with("a.md", b"hello", 1000), base, empty_output());
    assert!(matches!(
        cache.check_fast("a.md", 1000, 5, base),
        CacheResult::Hit(_)
    ));
    assert!(matches!(
        cache.check_fast("a.md", 1000, 5, variant),
        CacheResult::Miss
    ));
    assert!(cache.check_content("a.md", b"hello", variant).is_none());
}

#[test]
fn fast_path_hit() {
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params();

    cache.put(subject_with("a.md", b"hello", 1000), &p, empty_output());

    // Same mtime+size = fast hit.
    assert!(matches!(
        cache.check_fast("a.md", 1000, 5, &p),
        CacheResult::Hit(_)
    ));

    // Different mtime = miss.
    assert!(matches!(
        cache.check_fast("a.md", 2000, 5, &p),
        CacheResult::Miss
    ));

    // Different size = miss.
    assert!(matches!(
        cache.check_fast("a.md", 1000, 99, &p),
        CacheResult::Miss
    ));

    // Different profile = miss (different entry entirely).
    let strict = ScanParams {
        profile: "strict".into(),
        ..p.clone()
    };
    assert!(matches!(
        cache.check_fast("a.md", 1000, 5, &strict),
        CacheResult::Miss
    ));
}

#[test]
fn detect_ai_changes_cache_key() {
    let p = test_params();
    assert_variant_misses(
        &p,
        &ScanParams {
            detect_ai: true,
            ..p.clone()
        },
    );
}

#[test]
fn legacy_entries_without_engine_version_miss() {
    // The whole invalidation scheme rests on old entries deserializing with
    // empty defaults and therefore missing. Write a cache file that predates
    // both new fields and check that, rather than trusting the
    // #[serde(default)] annotations to stay put.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("legacy.json");
    let p = test_params();

    // Build a current entry, then strip the two fields back out of the
    // serialized form to reproduce a file written by an older binary.
    {
        let mut cache = ScanCache::open(path.clone());
        cache.put(subject_with("a.md", b"hello", 1000), &p, empty_output());
        cache.flush();
    }
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut stripped = 0;
    for entry in doc.as_array_mut().expect("cache file is an array") {
        if let Some(params) = entry.get_mut("params").and_then(|p| p.as_object_mut()) {
            params.remove("engine_version");
            params.remove("exempt_blockquotes");
            stripped += 1;
        }
    }
    assert!(stripped > 0, "test did not find a params object to strip");
    std::fs::write(&path, serde_json::to_string(&doc).unwrap()).unwrap();

    let mut cache = ScanCache::open(path);

    // The entry must still LOAD, which is what #[serde(default)] buys. Without
    // it the file fails to parse, load_entries swallows the error and returns
    // an empty map, and the Miss below would hold for the wrong reason. Looking
    // it up with legacy-shaped params must therefore Hit.
    let legacy_shaped = ScanParams {
        engine_version: String::new(),
        exempt_blockquotes: false,
        ..p.clone()
    };
    assert!(
        matches!(
            cache.check_fast("a.md", 1000, 5, &legacy_shaped),
            CacheResult::Hit(_)
        ),
        "legacy entry did not load: the serde defaults are gone"
    );

    // And current params miss it, because the key now carries both fields.
    assert!(matches!(
        cache.check_fast("a.md", 1000, 5, &p),
        CacheResult::Miss
    ));
    assert!(cache.check_content("a.md", b"hello", &p).is_none());
}

#[test]
fn exempt_blockquotes_changes_cache_key() {
    // The flag changes which spans are scanned, so a warm cache must not answer
    // for the other setting. It used to: the field was on ScanParams but absent
    // from the key, so a plain lint followed by an exempt one returned the
    // first run's issues.
    let p = test_params();
    assert_variant_misses(
        &p,
        &ScanParams {
            exempt_blockquotes: true,
            ..p.clone()
        },
    );
}

#[test]
fn profile_config_changes_cache_key() {
    // The key carries the whole effective ProfileConfig, not the name the user
    // asked for. The relaxed flag rewrites that config without changing the
    // name, so keying on the name let a relaxed run answer for a strict one and
    // a strict gate report clean. Derive both strings the way build_lint_setup
    // does, so this pins the real mechanism rather than two arbitrary strings
    // that happen to differ.
    use crate::rules::ruleset::Profile;
    let strict = Profile::Strict.config();
    let relaxed = strict.with_relaxed();
    assert_ne!(
        format!("{strict:?}"),
        format!("{relaxed:?}"),
        "with_relaxed must change the config it is keyed on"
    );

    let p = ScanParams {
        profile: format!("{strict:?}"),
        ..test_params()
    };
    assert_variant_misses(
        &p,
        &ScanParams {
            profile: format!("{relaxed:?}"),
            ..p.clone()
        },
    );
}

#[test]
fn engine_version_changes_cache_key() {
    // ruleset_hash covers the rules, not the passes that interpret them.
    // Without this an upgrade that changes a detector keeps serving the old
    // binary's results for every unchanged file until the TTL expires.
    let p = test_params();
    assert_variant_misses(
        &p,
        &ScanParams {
            engine_version: "0.2.0".into(),
            ..p.clone()
        },
    );
}

#[test]
fn ai_threshold_changes_cache_key() {
    let p = ScanParams {
        detect_ai: true,
        ai_threshold: "1.0".into(),
        ..test_params()
    };
    for threshold in ["0.5", "1.5"] {
        assert_variant_misses(
            &p,
            &ScanParams {
                ai_threshold: threshold.into(),
                ..p.clone()
            },
        );
    }
}

#[test]
fn translationese_domain_changes_cache_key() {
    let p = ScanParams {
        detect_translationese: true,
        translationese_domain: "general".into(),
        ..test_params()
    };
    assert_variant_misses(
        &p,
        &ScanParams {
            translationese_domain: "technical".into(),
            ..p.clone()
        },
    );
}

#[test]
fn slow_path_content_check() {
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params_plain();

    cache.put(subject_with("b.md", b"data", 1000), &p, empty_output());

    // Same content despite mtime miss: slow-path hit.
    assert!(cache.check_content("b.md", b"data", &p).is_some());

    // Different content: slow-path miss.
    assert!(cache.check_content("b.md", b"changed", &p).is_none());
}

#[test]
fn cache_persists_to_disk() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.bin");
    let p = test_params_plain();

    {
        let mut cache = ScanCache::open(path.clone());
        cache.put(subject("f.md"), &p, empty_output());
        cache.flush();
    }

    let mut cache = ScanCache::open(path);
    assert!(matches!(
        cache.check_fast("f.md", 100, 1, &p),
        CacheResult::Hit(_)
    ));
}

#[test]
fn expired_entries_pruned() {
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params_plain();
    cache.put(subject("e.md"), &p, empty_output());
    for entry in cache.entries_mut().values_mut() {
        entry.timestamp_secs = 0;
    }
    assert!(matches!(
        cache.check_fast("e.md", 100, 1, &p),
        CacheResult::Miss
    ));
}

#[test]
fn overflow_evicts_oldest() {
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params_plain();

    // Timestamps must be recent: the TTL prune runs before the count eviction,
    // so a fixed epoch would empty the map and let this test pass without the
    // eviction doing anything.
    let base = now_secs() - (MAX_ENTRIES as u64 + 10);
    for i in 0..MAX_ENTRIES + 10 {
        let name = format!("file_{i}.md");
        cache.put(subject(&name), &p, empty_output());
        let key = fast_key(&name, &p);
        if let Some(e) = cache.entries_mut().get_mut(&key) {
            e.timestamp_secs = base + i as u64;
        }
    }

    assert!(cache.entries().len() > MAX_ENTRIES);
    cache.flush();
    assert_eq!(cache.entries().len(), MAX_ENTRIES);
    // Oldest go first, so the newest entry must still be there.
    let newest = format!("file_{}.md", MAX_ENTRIES + 9);
    assert!(cache.entries().values().any(|e| e.file_path == newest));
}

/// Build a ScanOutput carrying `n` issues, for the entry-size caps.
/// `n` identical issues, each reporting `found`.
fn output_of(n: usize, found: &str) -> ScanOutput {
    use crate::rules::ruleset::{Issue, IssueType, Severity};
    let mut out = empty_output();
    out.issues = (0..n)
        .map(|i| {
            Issue::new(
                i,
                6,
                found,
                vec!["軟體".to_owned()],
                IssueType::CrossStrait,
                Severity::Warning,
            )
        })
        .collect();
    out
}

fn output_with_issues(n: usize) -> ScanOutput {
    output_of(n, "軟件")
}

#[test]
fn pathological_issue_count_is_not_cached() {
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params_plain();

    cache.put(
        subject("huge.md"),
        &p,
        output_with_issues(MAX_ENTRY_ISSUES + 1),
    );
    assert!(cache.entries().is_empty(), "oversized entry was stored");

    cache.put(subject("ok.md"), &p, output_with_issues(MAX_ENTRY_ISSUES));
    assert_eq!(cache.entries().len(), 1, "entry at the cap was rejected");
}

#[test]
fn oversized_entry_on_disk_is_dropped_on_load() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.bin");
    let p = test_params_plain();

    // Write an oversized entry directly, as a cache built before the cap
    // existed would contain.
    let entry = CacheEntry {
        file_path: "legacy.md".into(),
        content_hash: blake3_hex(b"x"),
        file_meta: FileMeta {
            mtime_secs: 100,
            size: 1,
        },
        params: p.clone(),
        output: output_with_issues(MAX_ENTRY_ISSUES + 1),
        input_was_sc: false,
        text_char_count: 1,
        timestamp_secs: now_secs(),
        fast_path_safe: true,
    };
    std::fs::write(&path, serde_json::to_vec(&vec![&entry]).unwrap()).unwrap();
    let before = std::fs::metadata(&path).unwrap().len();

    let mut cache = ScanCache::open(path.clone());
    assert!(
        cache.entries().is_empty(),
        "legacy oversized entry survived load"
    );

    // Dropping it from the map is not the point: the cost this cap exists to
    // remove is the per-run parse of the file, so it has to leave disk. A run
    // that stores nothing new never called put(), so only the load path can
    // mark the cache dirty.
    drop(cache);
    let after = std::fs::metadata(&path).unwrap().len();
    assert!(
        after < before,
        "oversized entry still on disk: {before} -> {after} bytes"
    );
    let (reloaded, _) = load_entries(&path, DEFAULT_TTL_SECS);
    assert!(reloaded.is_empty(), "file still holds the oversized entry");
}

#[test]
fn flush_keeps_entries_another_process_wrote_after_we_loaded() {
    // A second CLI process stores entries while this one holds a snapshot.
    // Flushing used to serialize that snapshot and swap the whole file, so the
    // other process's work vanished. The cleanup-on-load path made this
    // reachable from a run that never stored anything itself.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.bin");
    let p = test_params_plain();

    let mut ours = ScanCache::open(path.clone());
    ours.put(subject("ours.md"), &p, empty_output());
    ours.entries(); // take the snapshot

    // Meanwhile, another process writes a disjoint entry and exits.
    {
        let mut theirs = ScanCache::open(path.clone());
        theirs.put(
            CacheSubject {
                content: b"y",
                ..subject("theirs.md")
            },
            &p,
            empty_output(),
        );
        theirs.flush();
    }
    let (mid, _) = load_entries(&path, DEFAULT_TTL_SECS);
    assert_eq!(mid.len(), 1, "setup: other process did not land its entry");

    ours.flush();

    let (merged, _) = load_entries(&path, DEFAULT_TTL_SECS);
    let names: std::collections::BTreeSet<&str> =
        merged.values().map(|e| e.file_path.as_str()).collect();
    assert!(
        names.contains("theirs.md"),
        "flush discarded a concurrently written entry: {names:?}"
    );
    assert!(names.contains("ours.md"), "flush lost our own entry");
}

#[test]
fn fast_path_declines_while_the_recorded_mtime_is_this_second() {
    // mtime has second granularity, so a file rewritten to the same length
    // within the second it was scanned matches on mtime and size and would
    // serve the previous content's issues. The fast path must decline and let
    // the caller fall back to the content hash, which reads the file.
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params_plain();
    let now = now_secs();

    cache.put(
        CacheSubject {
            mtime_secs: now,
            ..subject("fresh.md")
        },
        &p,
        empty_output(),
    );
    assert!(
        matches!(cache.check_fast("fresh.md", now, 1, &p), CacheResult::Miss),
        "fast path trusted an mtime from this second"
    );

    // An older mtime is outside the window and still hits.
    let old = now - 60;
    cache.put(
        CacheSubject {
            mtime_secs: old,
            ..subject("settled.md")
        },
        &p,
        empty_output(),
    );
    assert!(
        matches!(
            cache.check_fast("settled.md", old, 1, &p),
            CacheResult::Hit(_)
        ),
        "fast path stopped working for settled files"
    );
}

#[test]
fn an_entry_stored_while_fresh_stays_off_the_fast_path() {
    // The window a clock guard at lookup time cannot close: the file was
    // written and scanned inside one second, so a later rewrite in that same
    // second leaves mtime and size unchanged. Time passing does not make the
    // recorded pair trustworthy, so the entry must keep failing the fast path
    // and force a content check for as long as it lives.
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params_plain();
    let now = now_secs();

    cache.put(
        CacheSubject {
            mtime_secs: now,
            ..subject("fresh.md")
        },
        &p,
        empty_output(),
    );
    let key = fast_key("fresh.md", &p);

    // Age the entry by an hour. Its mtime is still the second it was written
    // in, so nothing about it became safer.
    if let Some(e) = cache.entries_mut().get_mut(&key) {
        e.timestamp_secs -= 3600;
    }
    assert!(
        matches!(cache.check_fast("fresh.md", now, 1, &p), CacheResult::Miss),
        "an entry stored inside the write second became fast-path trusted"
    );
}

#[test]
fn flush_creates_the_cache_directory_before_taking_the_lock() {
    // On a first run the cache directory does not exist. Opening the lock
    // sidecar used to fail there and the write proceeded unlocked, so two
    // concurrent first runs could each replace the other's file.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing").join("deeper").join("c.bin");
    let p = test_params_plain();

    let mut cache = ScanCache::open(path.clone());
    cache.put(subject("a.md"), &p, empty_output());
    cache.flush();

    assert!(path.exists(), "cache file was not written");
    assert!(
        path.with_extension("lock").exists(),
        "no lock sidecar, so the write was taken unlocked"
    );
    let (loaded, _) = load_entries(&path, DEFAULT_TTL_SECS);
    assert_eq!(loaded.len(), 1);
}

#[test]
fn flush_keeps_the_newer_of_two_entries_for_one_key() {
    // Both processes scanned the same file. The later scan is the one worth
    // keeping, whichever side holds it.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.bin");
    let p = test_params_plain();

    let mut ours = ScanCache::open(path.clone());
    ours.put(subject("same.md"), &p, empty_output());
    let key = fast_key("same.md", &p);
    let stale = now_secs() - 500;
    if let Some(e) = ours.entries_mut().get_mut(&key) {
        e.timestamp_secs = stale;
    }

    {
        let mut theirs = ScanCache::open(path.clone());
        theirs.put(
            CacheSubject {
                content: b"y",
                mtime_secs: 200,
                size: 2,
                text_char_count: 9,
                ..subject("same.md")
            },
            &p,
            empty_output(),
        );
        theirs.flush();
    }

    ours.flush();

    let (merged, _) = load_entries(&path, DEFAULT_TTL_SECS);
    let e = merged.get(&key).expect("entry survived");
    assert_eq!(
        e.text_char_count, 9,
        "older snapshot overwrote the newer scan"
    );
}

#[test]
fn byte_cap_does_not_bind_on_a_normal_full_cache() {
    // The regression this pins: an 8 MiB budget sat BELOW what a healthy
    // MAX_ENTRIES-sized cache produces, so every run evicted a quarter of a
    // repo that was doing nothing wrong, and the next run rescanned it. A full
    // cache of ordinary documents must survive flush intact.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.bin");
    let mut cache = ScanCache::open(path);
    let p = test_params_plain();

    let base = now_secs() - MAX_ENTRIES as u64;
    for i in 0..MAX_ENTRIES {
        let name = format!("file_{i}.md");
        cache.put(subject(&name), &p, output_with_issues(20));
        let key = fast_key(&name, &p);
        if let Some(e) = cache.entries_mut().get_mut(&key) {
            e.timestamp_secs = base + i as u64;
        }
    }
    cache.flush();

    assert_eq!(
        cache.entries().len(),
        MAX_ENTRIES,
        "byte cap evicted from a cache that is merely full, not oversized"
    );
}

/// One issue carrying `bytes` of filler, for the tests that need a cache
/// over `MAX_TOTAL_BYTES`.  Those tests assert on the byte cap, and the
/// byte cap does not care how the bytes arrived.  Reaching 32 MiB through
/// issue *count* instead means serializing a few hundred thousand structs
/// three times over in a debug build, which cost more wall clock than the
/// rest of the unit suite combined.  `MAX_ENTRY_ISSUES` caps the count and
/// is tested separately.
fn output_with_bytes(bytes: usize) -> ScanOutput {
    output_of(1, &"x".repeat(bytes))
}

/// Filler per entry that puts 300 entries comfortably over the 32 MiB cap.
const OVERSIZE_FILLER: usize = 120 * 1024;

#[test]
fn total_size_cap_evicts_oldest() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.bin");
    let mut cache = ScanCache::open(path.clone());
    let p = test_params_plain();

    // Each entry sits under the per-entry cap; together they blow the file
    // budget, which is exactly the case MAX_ENTRIES alone does not catch.
    let base = now_secs() - 300;
    for i in 0..300 {
        let name = format!("file_{i}.md");
        cache.put(subject(&name), &p, output_with_bytes(OVERSIZE_FILLER));
        let key = fast_key(&name, &p);
        if let Some(e) = cache.entries_mut().get_mut(&key) {
            e.timestamp_secs = base + i as u64;
        }
    }
    cache.flush();

    // Prove eviction actually ran. Without this the test passes trivially
    // whenever the fixture fails to exceed the cap, which is one edit to
    // OVERSIZE_FILLER or one bump of MAX_TOTAL_BYTES away.
    assert!(
        cache.entries().len() < 300,
        "nothing was evicted: the fixture never exceeded {MAX_TOTAL_BYTES} bytes"
    );

    let written = std::fs::metadata(&path).unwrap().len() as usize;
    assert!(
        written <= MAX_TOTAL_BYTES,
        "cache file {written} bytes exceeds {MAX_TOTAL_BYTES}"
    );
    // Newest survive: eviction is oldest-first, not arbitrary.
    assert!(cache
        .entries()
        .values()
        .any(|e| e.file_path == "file_299.md"));
}

#[test]
fn legacy_total_size_over_budget_is_rewritten_on_cache_hit_only_run() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.bin");
    let p = test_params_plain();
    let base = now_secs() - 300;
    let entries: Vec<CacheEntry> = (0..300)
        .map(|i| CacheEntry {
            file_path: format!("file_{i}.md"),
            content_hash: blake3_hex(b"x"),
            file_meta: FileMeta {
                mtime_secs: 100,
                size: 1,
            },
            params: p.clone(),
            output: output_with_bytes(OVERSIZE_FILLER),
            input_was_sc: false,
            text_char_count: 1,
            timestamp_secs: base + i,
            fast_path_safe: true,
        })
        .collect();
    std::fs::write(&path, serde_json::to_vec(&entries).unwrap()).unwrap();
    let before = std::fs::metadata(&path).unwrap().len() as usize;
    assert!(
        before > MAX_TOTAL_BYTES,
        "test cache did not exceed byte cap: {before}"
    );

    let mut cache = ScanCache::open(path.clone());
    assert_eq!(cache.entries().len(), entries.len());
    drop(cache);

    let after = std::fs::metadata(&path).unwrap().len() as usize;
    assert!(
        after <= MAX_TOTAL_BYTES,
        "legacy cache stayed over byte cap: {before} -> {after}"
    );
}

#[test]
fn char_count_survives_cache_hits() {
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params_plain();

    cache.put(
        CacheSubject {
            content: "甲乙丙".as_bytes(),
            mtime_secs: 1000,
            size: 9,
            text_char_count: 3,
            ..subject("chars.md")
        },
        &p,
        empty_output(),
    );

    let fast_hit = cache
        .check_fast("chars.md", 1000, 9, &p)
        .into_hit()
        .unwrap();
    assert_eq!(fast_hit.text_char_count, 3);

    let content_hit = cache
        .check_content("chars.md", "甲乙丙".as_bytes(), &p)
        .unwrap();
    assert_eq!(content_hit.text_char_count, 3);
}

#[test]
fn legacy_entries_without_char_count_miss_until_refreshed() {
    let dir = TempDir::new().unwrap();
    let mut cache = ScanCache::open(dir.path().join("c.bin"));
    let p = test_params_plain();

    cache.put(
        CacheSubject {
            content: b"abc",
            mtime_secs: 1000,
            size: 3,
            text_char_count: 0,
            ..subject("legacy.md")
        },
        &p,
        empty_output(),
    );

    assert!(matches!(
        cache.check_fast("legacy.md", 1000, 3, &p),
        CacheResult::Miss
    ));
    assert!(cache.check_content("legacy.md", b"abc", &p).is_none());
}
