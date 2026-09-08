use super::super::Scanner;
use crate::engine::excluded::ByteRange;
use crate::fixer::{apply_fixes, FixMode};
use crate::rules::ruleset::IssueType;
use crate::rules::ruleset::{Profile, SpacingPolicy};

fn spacing_issues(text: &str) -> Vec<(String, String)> {
    spacing_issues_with_policy(text, SpacingPolicy::Require)
        .into_iter()
        .filter(|i| {
            i.context.as_deref().is_some_and(|c| {
                c.contains("空格")
                    || c.contains("標點")
                    || c.contains("數字應使用")
                    || c.contains("不重複")
            })
        })
        .map(|i| {
            (
                i.context.as_deref().unwrap_or("").to_string(),
                i.suggestions.first().cloned().unwrap_or_default(),
            )
        })
        .collect()
}

fn spacing_issues_with_policy(
    text: &str,
    policy: SpacingPolicy,
) -> Vec<crate::rules::ruleset::Issue> {
    spacing_issues_excluding(text, policy, &[])
}

fn spacing_issues_excluding(
    text: &str,
    policy: SpacingPolicy,
    excluded: &[ByteRange],
) -> Vec<crate::rules::ruleset::Issue> {
    let scanner = Scanner::new(vec![], vec![]);
    scanner
        .scan_with_config(
            text,
            excluded,
            Profile::Base.config().with_spacing_policy(policy),
        )
        .issues
        .into_iter()
        .filter(|issue| issue.rule_type == IssueType::Punctuation)
        .collect()
}

/// The issues carrying one of the named contexts, matched whole.
///
/// Whole rather than by substring because the contexts overlap: rule 3 also
/// ends its removal message in 不加空格, so a test for the boundary policy
/// that filtered on that phrase would count rule 3's findings as its own.
fn issues_with_context<'a>(
    issues: &'a [crate::rules::ruleset::Issue],
    contexts: &[&str],
) -> Vec<&'a crate::rules::ruleset::Issue> {
    issues
        .iter()
        .filter(|issue| {
            issue
                .context
                .as_deref()
                .is_some_and(|c| contexts.contains(&c))
        })
        .collect()
}

/// The contexts the boundary policy itself emits under Strip.
fn boundary_strip_issues(
    issues: &[crate::rules::ruleset::Issue],
) -> Vec<&crate::rules::ruleset::Issue> {
    issues_with_context(issues, &["中英文之間不加空格", "中文與數字之間不加空格"])
}

#[test]
fn cjk_latin_missing_space() {
    let issues = spacing_issues("在LeanCloud上");
    assert!(
        issues.iter().any(|(c, _)| c.contains("中英文")),
        "should flag missing space between CJK and Latin: {issues:?}"
    );
}

#[test]
fn cjk_latin_has_space() {
    let issues = spacing_issues("在 LeanCloud 上");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("中英文")),
        "should not flag when space exists: {issues:?}"
    );
}

#[test]
fn cjk_digit_missing_space() {
    let issues = spacing_issues("花了5000元");
    assert!(
        issues.iter().any(|(c, _)| c.contains("數字")),
        "should flag missing space between CJK and digit: {issues:?}"
    );
}

#[test]
fn cjk_digit_has_space() {
    let issues = spacing_issues("花了 5000 元");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("數字")),
        "should not flag when space exists: {issues:?}"
    );
}

#[test]
fn strip_removes_only_cjk_boundary_spaces() {
    let issues = spacing_issues_with_policy("在 LeanCloud 上，有 42 項", SpacingPolicy::Strip);
    let stripped = boundary_strip_issues(&issues);
    assert_eq!(stripped.len(), 4, "strip issues: {issues:?}");
    assert!(stripped
        .iter()
        .all(|issue| issue.suggestions.first().is_some_and(String::is_empty)));
}

#[test]
fn strip_leaves_adjacent_boundaries_and_other_spacing_rules_unchanged() {
    let issues = spacing_issues_with_policy("在LeanCloud上， Test", SpacingPolicy::Strip);
    assert!(
        !issues.iter().any(|issue| issue
            .context
            .as_deref()
            .is_some_and(|c| c.contains("中英文") || c.contains("中文與數字"))),
        "adjacent boundaries must be accepted by strip: {issues:?}"
    );
    assert!(
        issues.iter().any(|issue| issue
            .context
            .as_deref()
            .is_some_and(|c| c.contains("全形標點"))),
        "rule 3 must remain active in strip mode: {issues:?}"
    );
}

#[test]
fn require_then_strip_round_trips_boundary_fixture() {
    let original = "在LeanCloud上有42項";
    let required = spacing_issues_with_policy(original, SpacingPolicy::Require);
    let spaced = apply_fixes(original, &required, FixMode::Orthographic, &[]).text;

    // Asserted rather than implied: without this the round trip also passes
    // when Require stops emitting and both fixes are no-ops.
    assert_eq!(spaced, "在 LeanCloud 上有 42 項");
    let stripped = spacing_issues_with_policy(&spaced, SpacingPolicy::Strip);
    let restored = apply_fixes(&spaced, &stripped, FixMode::Orthographic, &[]).text;
    assert_eq!(restored, original);
}

#[test]
fn strip_removes_a_whole_space_run_at_one_boundary() {
    // The only place the new span arithmetic can go wrong: the issue has to
    // cover the run, not just the first space.
    for (text, fixed) in [("中  A", "中A"), ("中   1", "中1")] {
        let issues = spacing_issues_with_policy(text, SpacingPolicy::Strip);
        let stripped = boundary_strip_issues(&issues);
        assert_eq!(stripped.len(), 1, "{text}: {issues:?}");
        assert_eq!(
            apply_fixes(text, &issues, FixMode::Orthographic, &[]).text,
            fixed
        );
    }
}

#[test]
fn strip_governs_u0020_only() {
    // Require inserts U+0020 and rule 3 removes U+0020, so Strip owns the same
    // character and nothing else. A tab or a newline is layout, and U+00A0 or
    // U+3000 is typography the author chose.
    for gap in ['\u{00a0}', '\u{3000}', '\t', '\n'] {
        let text = format!("中{gap}A");
        let issues = spacing_issues_with_policy(&text, SpacingPolicy::Strip);
        assert!(
            boundary_strip_issues(&issues).is_empty(),
            "{:?} is not a stored boundary space: {issues:?}",
            gap
        );
    }
}

#[test]
fn strip_leaves_rules_four_and_five_alone() {
    // Rules 4 and 5 are not boundary policy, so both modes have to agree on
    // them character for character.
    for text in ["好啊！！！", "第３版", "在LeanCloud上！！！"] {
        let under = |policy| {
            let issues = spacing_issues_with_policy(text, policy);
            issues_with_context(&issues, &["不重複使用標點符號", "數字應使用半形字元"])
                .into_iter()
                .map(|issue| (issue.offset, issue.found.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            under(SpacingPolicy::Require),
            under(SpacingPolicy::Strip),
            "{text}: rules 4 and 5 must not read the boundary policy"
        );
    }
}

#[test]
fn strip_keeps_rule_three_when_the_text_after_the_space_is_excluded() {
    // The strip lookahead runs before rule 3 at the same character. When it
    // lands on an excluded range it has nothing to report, and used to return
    // from the whole check, taking rule 3 with it.
    let text = "參考： https://example.com/a";
    let url_start = text.find("https").unwrap();
    let excluded = [ByteRange {
        start: url_start,
        end: text.len(),
    }];
    let rule_three = |policy| {
        let issues = spacing_issues_excluding(text, policy, &excluded);
        issues_with_context(&issues, &["全形標點與其他字元之間不加空格"])
            .into_iter()
            .map(|issue| (issue.offset, issue.length))
            .collect::<Vec<_>>()
    };
    let require = rule_three(SpacingPolicy::Require);
    assert_eq!(require.len(), 1, "rule 3 should fire under require");
    assert_eq!(
        rule_three(SpacingPolicy::Strip),
        require,
        "rule 3 must not depend on the boundary policy"
    );
}

#[test]
fn space_before_fullwidth_punct() {
    let issues = spacing_issues("iPhone ，好開心");
    assert!(
        issues.iter().any(|(c, _)| c.contains("全形標點")),
        "should flag space before fullwidth comma: {issues:?}"
    );
}

#[test]
fn no_space_around_fullwidth_punct() {
    let issues = spacing_issues("iPhone，好開心");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("全形標點")),
        "should not flag correct punctuation: {issues:?}"
    );
}

#[test]
fn repeated_fullwidth_punct() {
    let issues = spacing_issues("太厲害了！！");
    assert!(
        issues.iter().any(|(c, _)| c.contains("不重複")),
        "should flag repeated exclamation: {issues:?}"
    );
}

#[test]
fn single_fullwidth_punct_ok() {
    let issues = spacing_issues("太厲害了！");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("不重複")),
        "should not flag single exclamation: {issues:?}"
    );
}

#[test]
fn fullwidth_digit_flagged() {
    let issues = spacing_issues("只賣１０００元");
    assert!(
        issues.iter().any(|(c, _)| c.contains("數字應使用")),
        "should flag fullwidth digits: {issues:?}"
    );
}

#[test]
fn mixed_exclamation_question_not_repeated() {
    // ！？ are different punctuation marks: not "repeated".
    let issues = spacing_issues("真的嗎！？");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("不重複")),
        "should not flag mixed ！？ as repeated: {issues:?}"
    );
}

#[test]
fn multi_space_before_fullwidth_punct() {
    // Multiple spaces before fullwidth comma should still be flagged.
    let issues = spacing_issues("iPhone  ，好開心");
    assert!(
        issues.iter().any(|(c, _)| c.contains("全形標點")),
        "should flag multi-space before fullwidth comma: {issues:?}"
    );
}

#[test]
fn multi_space_after_fullwidth_punct() {
    // Multiple spaces after fullwidth comma should still be flagged.
    let issues = spacing_issues("好，  開心");
    assert!(
        issues.iter().any(|(c, _)| c.contains("全形標點")),
        "should flag multi-space after fullwidth comma: {issues:?}"
    );
}

#[test]
fn double_ellipsis_ok() {
    // …… (exactly 2) is standard zh-TW form: should NOT be flagged.
    let issues = spacing_issues("他說……算了");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("不重複")),
        "should not flag standard double ellipsis: {issues:?}"
    );
}

#[test]
fn triple_ellipsis_flagged() {
    // ……… (3+) is non-standard: should be flagged.
    let issues = spacing_issues("他說………算了");
    assert!(
        issues.iter().any(|(c, _)| c.contains("不重複")),
        "should flag triple ellipsis: {issues:?}"
    );
}

#[test]
fn double_em_dash_ok() {
    // —— (exactly 2) is standard zh-TW form: should NOT be flagged.
    let issues = spacing_issues("他——就是那個人");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("不重複")),
        "should not flag standard double em dash: {issues:?}"
    );
}

#[test]
fn triple_em_dash_flagged() {
    // ——— (3+) is non-standard: should be flagged.
    let issues = spacing_issues("他———就是那個人");
    assert!(
        issues.iter().any(|(c, _)| c.contains("不重複")),
        "should flag triple em dash: {issues:?}"
    );
}

#[test]
fn space_after_fullwidth_punct_before_latin() {
    // Per guidelines: "全形標點與其他字元之間不加空格" applies to Latin too.
    let issues = spacing_issues("好， Test很好");
    assert!(
        issues.iter().any(|(c, _)| c.contains("全形標點")),
        "should flag space after fullwidth comma before Latin: {issues:?}"
    );
}

#[test]
fn no_space_after_fullwidth_punct_before_latin() {
    let issues = spacing_issues("好，Test很好");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("全形標點")),
        "should not flag when no space after fullwidth punct: {issues:?}"
    );
}

#[test]
fn space_after_fullwidth_punct_before_digit() {
    let issues = spacing_issues("共 3 項，其中有 2 項");

    // "，其" has no space → OK. But "， 2" would be flagged. This input has no
    // space after comma, so no flag.
    assert!(
        !issues
            .iter()
            .any(|(c, s)| c.contains("全形標點") && s.is_empty()),
        "no space after comma here: {issues:?}"
    );
    // Now with space after comma before digit:
    let issues2 = spacing_issues("共有， 2項");
    assert!(
        issues2.iter().any(|(c, _)| c.contains("全形標點")),
        "should flag space after fullwidth comma before digit: {issues2:?}"
    );
}

// Edge-case stress tests for sliding-window rewrite

#[test]
fn space_at_text_start_before_fullwidth_punct() {
    // Leading space before fullwidth punct: prev is None, so rule 3
    // space-before-punct requires prev to be non-space content → no fire.
    let issues = spacing_issues(" ，好開心");
    assert!(
        !issues
            .iter()
            .any(|(c, s)| c.contains("全形標點") && s.is_empty()),
        "leading space before punct should not flag (no preceding content): {issues:?}"
    );
}

#[test]
fn trailing_spaces_after_fullwidth_punct() {
    // Text ends with spaces after fullwidth punct: the forward scan for
    // space-after-punct should not fire because after_ch stays ' '.
    let issues = spacing_issues("好，   ");
    assert!(
        !issues
            .iter()
            .any(|(c, s)| c.contains("全形標點") && s.is_empty()),
        "trailing spaces after punct at end of text should not flag: {issues:?}"
    );
}

#[test]
fn fullwidth_punct_then_space_then_fullwidth_punct() {
    // ， ？: space between two fullwidth puncts. Rule 3 space-after-punct only
    // fires if after_ch is CJK/alphanumeric, not another punct.
    let issues = spacing_issues("好， ？");
    assert!(
        !issues
            .iter()
            .any(|(c, s)| c.contains("全形標點") && s.is_empty()),
        "space between two fullwidth puncts should not flag: {issues:?}"
    );
}

#[test]
fn punct_run_resets_after_non_punct() {
    // ！好！: the second ！ should not be flagged as repeated because a CJK
    // char intervenes, resetting same_punct_run.
    let issues = spacing_issues("太棒！好！");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("不重複")),
        "punct separated by content should not flag as repeated: {issues:?}"
    );
}

#[test]
fn different_punct_not_repeated() {
    // ，。: different fullwidth punct chars should not trigger rule 4.
    let issues = spacing_issues("好，好。");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("不重複")),
        "different punct chars should not flag as repeated: {issues:?}"
    );
}

#[test]
fn single_char_text() {
    // Single CJK character: no next char, no prev initially.
    let issues = spacing_issues("好");
    assert!(
        issues.is_empty(),
        "single char should produce no spacing issues: {issues:?}"
    );
}

#[test]
fn only_spaces() {
    let issues = spacing_issues("   ");
    assert!(
        issues.is_empty(),
        "only spaces should produce no spacing issues: {issues:?}"
    );
}

#[test]
fn empty_text() {
    let issues = spacing_issues("");
    assert!(
        issues.is_empty(),
        "empty text should produce no spacing issues"
    );
}

#[test]
fn cjk_space_latin_space_cjk_correct() {
    // Properly spaced: CJK SPACE Latin SPACE CJK, no issues.
    let issues = spacing_issues("好 ABC 好");
    assert!(
        !issues.iter().any(|(c, _)| c.contains("中英文")),
        "properly spaced CJK-Latin-CJK should not flag: {issues:?}"
    );
}

#[test]
fn fullwidth_digit_adjacent_to_cjk() {
    // Fullwidth digit next to CJK should flag rule 5 (fullwidth→halfwidth) but
    // NOT rule 2 (CJK-digit spacing), because the fullwidth digit is not an
    // ASCII digit.
    let issues = spacing_issues("有３項");
    let has_fw_digit = issues.iter().any(|(c, _)| c.contains("數字應使用"));
    assert!(has_fw_digit, "should flag fullwidth digit: {issues:?}");
}

#[test]
fn rule3_space_before_punct_with_latin_content() {
    // Latin char before space before fullwidth punct.
    let issues = spacing_issues("test ，好");
    assert!(
        issues.iter().any(|(c, _)| c.contains("全形標點")),
        "should flag space before fullwidth punct after Latin: {issues:?}"
    );
}

#[test]
fn rule3_space_before_punct_with_digit_content() {
    // Digit before space before fullwidth punct.
    let issues = spacing_issues("123 ，好");
    assert!(
        issues.iter().any(|(c, _)| c.contains("全形標點")),
        "should flag space before fullwidth punct after digit: {issues:?}"
    );
}

#[test]
fn rule4_quadruple_ellipsis() {
    // 4 consecutive ellipsis marks: run=1 is OK (paired), run=2 and 3 flagged.
    let issues = spacing_issues("他說…………算了");
    let repeat_count = issues.iter().filter(|(c, _)| c.contains("不重複")).count();
    assert_eq!(
        repeat_count, 2,
        "4 ellipsis should flag 2 extras: {issues:?}"
    );
}

#[test]
fn rule1_boundary_latin_then_cjk() {
    // Latin immediately followed by CJK.
    let issues = spacing_issues("Hello世界");
    assert!(
        issues.iter().any(|(c, _)| c.contains("中英文")),
        "should flag missing space Latin→CJK: {issues:?}"
    );
}

#[test]
fn rule2_boundary_digit_then_cjk() {
    // Digit immediately followed by CJK.
    let issues = spacing_issues("42個");
    assert!(
        issues.iter().any(|(c, _)| c.contains("數字")),
        "should flag missing space digit→CJK: {issues:?}"
    );
}

#[test]
fn strip_never_reports_a_span_that_covers_excluded_bytes() {
    // An exclusion can begin or end inside the run, not only on the character
    // the lookahead lands on. Reporting the whole run then hands the fixer a
    // span that deletes protected bytes.
    let text = "中   A";
    let excluded = [ByteRange { start: 4, end: 5 }];
    let issues = spacing_issues_excluding(text, SpacingPolicy::Strip, &excluded);
    assert!(
        boundary_strip_issues(&issues).is_empty(),
        "a run holding excluded bytes is not removable: {issues:?}"
    );

    // Without the exclusion the same run is still reported whole.
    let clean = spacing_issues_excluding(text, SpacingPolicy::Strip, &[]);
    let strip = boundary_strip_issues(&clean);
    assert_eq!(strip.len(), 1);
    assert_eq!((strip[0].offset, strip[0].length), (3, 3));
}
