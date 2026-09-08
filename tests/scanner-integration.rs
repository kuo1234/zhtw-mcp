// Scanner integration tests.
//
// Each test constructs a Scanner with specific rules and verifies that scanning
// produces the expected issues for traditional/simplified filtering, code block
// exclusion, URL/path exclusion, @mention exclusion, case rules, punctuation
// normalization, and alternatives handling.

use zhtw_mcp::engine::scan::{build_exclusions_for_content_type, ContentType, Scanner};
use zhtw_mcp::fixer::{apply_fixes_with_context, FixMode};
use zhtw_mcp::rules::ruleset::{CaseRule, Profile, RuleType, SpellingRule};

// Helpers

fn spelling(from: &str, to: &[&str]) -> SpellingRule {
    SpellingRule::new(
        from,
        to.iter().map(|s| s.to_string()).collect(),
        RuleType::CrossStrait,
    )
}

fn variant(from: &str, to: &[&str]) -> SpellingRule {
    SpellingRule::new(
        from,
        to.iter().map(|s| s.to_string()).collect(),
        RuleType::Variant,
    )
}

fn case_rule(term: &str, alternatives: Option<&[&str]>) -> CaseRule {
    CaseRule {
        term: term.into(),
        alternatives: alternatives.map(|a| a.iter().map(|s| s.to_string()).collect()),
        disabled: false,
    }
}

// Traditional/Simplified filtering (4 tests)

#[test]
fn traditional_rule_applies_to_traditional_text() {
    // Cross-strait rule fires on Traditional Chinese text.
    let scanner = Scanner::new(vec![spelling("你好", &["Hello"])], vec![]);
    let issues = scanner.scan("繁體中文你好").issues;
    assert_eq!(issues.len(), 1);
}

#[test]
fn traditional_rule_applies_to_neutral_text() {
    // Cross-strait rule fires on neutral (unknown) text.
    let scanner = Scanner::new(vec![spelling("你好", &["Hello"])], vec![]);
    let issues = scanner.scan("你好").issues;
    assert_eq!(issues.len(), 1);
}

#[test]
fn traditional_rule_skipped_for_simplified_text() {
    // Variant rule (character-form correction) is skipped on Simplified text.
    let scanner = Scanner::new(vec![variant("着", &["著"])], vec![]);
    let issues = scanner
        .scan_profiled("简体中文里着重", Profile::Strict)
        .issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn non_traditional_rule_applies_to_simplified_text() {
    // Cross-strait rule fires even on Simplified Chinese text.
    let scanner = Scanner::new(vec![spelling("你好", &["Hello"])], vec![]);
    let issues = scanner.scan("简体中文你好").issues;
    assert_eq!(issues.len(), 1);
}

#[test]
fn formulaic_section_boundary_works_without_a_markdown_content_type() {
    // ContentType defaults to Plain, so an MCP caller passing Markdown text
    // with no filename must still get the detector. The parser index adds
    // boundaries; it is not a precondition for having any.
    let scanner = Scanner::new(vec![], vec![]);
    let mut cfg = Profile::Base.config();
    cfg.ai_structural_patterns = true;
    let text = "本節總結。展望未來。\n\n# 下一節\n";
    for content_type in [ContentType::Markdown, ContentType::Plain] {
        let issues = scanner
            .scan_for_content_type_with_config(text, content_type, cfg)
            .issues;
        assert!(
            issues.iter().any(|issue| issue.found == "展望未來"),
            "closer lost for {content_type:?}: {issues:?}"
        );
    }
}

#[test]
fn a_code_fence_is_not_a_section_boundary_in_either_content_type() {
    // The lead-in sentence before a fence is ordinary technical prose.
    let scanner = Scanner::new(vec![], vec![]);
    let mut cfg = Profile::Base.config();
    cfg.ai_structural_patterns = true;
    let text = "## 安裝\n\n新版套件終於支援離線安裝，值得期待。\n\n```sh\nmake install\n```\n\n安裝完成後即可使用。\n";
    for content_type in [ContentType::Markdown, ContentType::Plain] {
        let issues = scanner
            .scan_for_content_type_with_config(text, content_type, cfg)
            .issues;
        assert!(
            !issues.iter().any(|issue| issue.found == "值得期待"),
            "lead-in flagged as a closer for {content_type:?}: {issues:?}"
        );
    }
}

// Code block exclusion: spelling rules (5 tests)

#[test]
fn single_backtick_code_excluded() {
    // Should ignore errors inside single-backtick inline code.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let issues = scanner.scan("這是 `錯誤` 的文字").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn triple_backtick_with_lang_excluded() {
    // Should ignore errors inside triple-backtick code fence with language tag.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let issues = scanner
        .scan("這是程式碼：\n```javascript\n錯誤\n```\n的文字")
        .issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn triple_backtick_with_other_tag_excluded() {
    // Should ignore errors inside triple-backtick code fence with extra
    // attributes.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let issues = scanner
        .scan("這是程式碼：\n```javascript line=5\n錯誤\n```\n的文字")
        .issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn triple_backtick_no_lang_excluded() {
    // Should ignore errors inside triple-backtick code fence (no language).
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let issues = scanner.scan("這是程式碼：\n```\n錯誤\n```\n的文字").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn error_outside_code_block_detected() {
    // Should detect errors outside excluded blocks.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let issues = scanner.scan("這是 `正確` 但是這裡有錯誤的文字").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "錯誤");
}

// Markdown handling (5 tests)

#[test]
fn markdown_link_text_checked() {
    // Link text in Markdown links is NOT excluded.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let issues = scanner
        .scan("這是 [錯誤](https://example.com) 的文字")
        .issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "錯誤");
    assert_eq!(issues[0].suggestions[..], vec!["正確"]);
}

#[test]
fn markdown_relative_destination_excluded_for_spelling() {
    // This asserted the opposite until the exclusion pass learned what a link
    // is, and the comment it carried described the old implementation rather
    // than a decision: only a destination carrying a URL scheme was excluded,
    // because only the URL regex was looking, so a relative path or an anchor
    // was read as prose.
    //
    // A destination is an address, and the term in one belongs to a filename
    // somebody has to rename, not to a sentence. Reporting it is at best advice
    // the linter cannot act on, and at worst a fix that rewrites the address
    // and leaves the link pointing nowhere. The link text one bracket to the
    // left is the prose, and markdown_link_text_checked above holds it in the
    // scan.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    assert!(scanner.scan("這是 [hi](錯誤) 的文字").issues.is_empty());
    assert!(scanner.scan("這是 [hi](#錯誤) 的文字").issues.is_empty());
    assert!(scanner
        .scan("這是 [hi](docs/錯誤.md) 的文字")
        .issues
        .is_empty());
}

#[test]
fn autolink_excluded() {
    // Should ignore errors inside autolinks.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let issues = scanner.scan("這是 <http://錯誤> 的文字").issues;
    // The URL portion is excluded by the URL regex.
    assert_eq!(issues.len(), 0);
}

#[test]
fn empty_autolink_no_crash() {
    // Should handle empty autolinks gracefully.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let issues = scanner.scan("這是 <> 的文字").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn multiple_excluded_blocks_only_outside_detected() {
    // Should handle multiple exclusion zones correctly. "錯誤" is in code
    // block, [錯誤](url): "錯誤" in link text is NOT in URL, <http://錯誤> is
    // in URL. Only the bare 錯誤 at the end is detected.
    let scanner = Scanner::new(vec![spelling("錯誤", &["正確"])], vec![]);
    let text = "`錯誤` [錯誤](url) <http://錯誤> 這裡有錯誤";
    let issues = scanner.scan(text).issues;

    // The [錯誤] in link text is not excluded, and the trailing 錯誤 is not
    // excluded. The "錯誤" in backticks and http://錯誤 in autolink ARE
    // excluded. So we expect 2 issues: [錯誤] in link text and the trailing
    // one.
    assert!(!issues.is_empty());
    // At minimum the trailing bare 錯誤 must be found.
    assert!(issues.iter().any(|i| i.found == "錯誤"));
}

// Code block exclusion: case rules (4 tests)

#[test]
fn case_error_in_code_block_excluded() {
    // Should ignore case errors inside code blocks.
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner.scan("這是 `javascript` 的程式碼").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn case_error_outside_code_block_detected() {
    // Should detect case errors outside code blocks.
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner.scan("這是 javascript 的文字").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "javascript");
    assert_eq!(issues[0].suggestions[..], vec!["JavaScript"]);
}

#[test]
fn case_error_in_markdown_link_text_detected() {
    // Should detect case errors in Markdown link text.
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner
        .scan("這是 [javascript](https://example.com) 的連結")
        .issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "javascript");
    assert_eq!(issues[0].suggestions[..], vec!["JavaScript"]);
}

#[test]
fn http_case_in_code_block_excluded() {
    // Should ignore case errors inside single-backtick code.
    let scanner = Scanner::new(vec![], vec![case_rule("HTTP", None)]);
    let issues = scanner.scan("這是 `http example.com` 的程式碼").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn http_case_in_triple_backtick_excluded() {
    // Should ignore case errors inside triple-backtick code fence.
    let scanner = Scanner::new(vec![], vec![case_rule("HTTP", None)]);
    let issues = scanner
        .scan("這是程式碼：\n```\nhttp example.com\n```\n的文字")
        .issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn http_case_outside_code_block_detected() {
    // Should detect case errors outside code blocks.
    let scanner = Scanner::new(vec![], vec![case_rule("HTTP", None)]);
    let issues = scanner.scan("這是 http example.com 的文字").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "http");
    assert_eq!(issues[0].suggestions[..], vec!["HTTP"]);
}

#[test]
fn http_case_in_url_inside_code_block_excluded() {
    // Should ignore case errors when URL is inside inline code.
    let scanner = Scanner::new(vec![], vec![case_rule("HTTP", None)]);
    let text = "1. 在瀏覽器中開啟 `http://localhost:3000`（或你設定的其他連接埠）";
    let issues = scanner.scan(text).issues;
    assert_eq!(issues.len(), 0);
}

// URL exclusion (5 tests)

#[test]
fn url_content_excluded_for_spelling() {
    // URLs should not be flagged as errors.
    let scanner = Scanner::new(vec![spelling("example", &["Example"])], vec![]);
    let issues = scanner.scan("這是 https://example.com 的文字").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn metalinguistic_quote_excludes_a_term_but_not_ordinary_use() {
    let scanner = Scanner::new(vec![spelling("軟件", &["軟體"])], vec![]);

    let mentioned = "「軟件」是中國用語。";
    let issues = scanner.scan(mentioned).issues;
    assert!(issues.is_empty());
    assert!(scanner
        .scan_profiled_md(mentioned, Profile::Base, false)
        .issues
        .is_empty());

    // One sentence naming the term and then using it, so the fixer runs with a
    // real issue and has to rewrite one occurrence while leaving the other. An
    // empty issue list would make this pass whatever the exclusion did.
    let both = "「軟件」是中國用語，我們用軟件開發。";
    let issues = scanner.scan(both).issues;
    assert_eq!(issues.len(), 1);
    let fixed = apply_fixes_with_context(
        both,
        &issues,
        FixMode::LexicalSafe,
        &build_exclusions_for_content_type(both, ContentType::Markdown),
        None,
    );
    assert_eq!(fixed.text, "「軟件」是中國用語，我們用軟體開發。");

    assert_eq!(scanner.scan("我們用軟件開發服務。").issues.len(), 1);
    assert_eq!(scanner.scan("我們用「軟件」開發服務。").issues.len(), 1);
    assert_eq!(
        scanner.scan("「軟件」是產品名稱而非舊說法。").issues.len(),
        1
    );
}

#[test]
fn disabled_ciwai_rule_keeps_its_straddle_guard() {
    let scanner = Scanner::new(
        vec![SpellingRule::new("此外", vec![], RuleType::AiFiller)],
        vec![],
    );
    assert!(scanner
        .scan("他個性如此外向，彼此外貌相似，由此外推可得結論。")
        .issues
        .is_empty());
}

#[test]
fn url_content_excluded_for_case_instagram() {
    // Should not flag case issues inside URLs.
    let scanner = Scanner::new(vec![], vec![case_rule("Instagram", None)]);
    let issues = scanner
        .scan("[Instagram](https://instagram.com/em.tec.blog)")
        .issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn http_url_excluded_for_case() {
    // Should ignore case errors inside http:// URLs.
    let scanner = Scanner::new(vec![], vec![case_rule("Google", None)]);
    let issues = scanner.scan("造訪 http://google.com/search 查詢").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn https_url_excluded_for_case() {
    // Should ignore case errors inside https:// URLs.
    let scanner = Scanner::new(vec![], vec![case_rule("Facebook", None)]);
    let issues = scanner.scan("前往 https://facebook.com/profile").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn rtmp_url_excluded_for_case() {
    // Should ignore case errors inside rtmp:// URLs.
    let scanner = Scanner::new(vec![], vec![case_rule("Example", None)]);
    let issues = scanner.scan("前往 rtmp://example.com/stream").issues;
    assert_eq!(issues.len(), 0);
}

// Path exclusion (3 tests)

#[test]
fn relative_path_dot_slash_excluded_for_case() {
    // Should ignore case errors inside relative paths (./).
    let scanner = Scanner::new(vec![], vec![case_rule("Image", None)]);
    let issues = scanner.scan("載入 ./image.png 檔案").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn parent_path_excluded_for_case() {
    // Should ignore case errors inside parent paths (../).
    let scanner = Scanner::new(vec![], vec![case_rule("Config", None)]);
    let issues = scanner.scan("讀取 ../config.json 設定").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn absolute_path_excluded_for_case() {
    // Should ignore case errors inside absolute paths.
    let scanner = Scanner::new(vec![], vec![case_rule("Asset", None)]);
    let issues = scanner.scan("使用 /asset/icon.svg 圖示").issues;
    assert_eq!(issues.len(), 0);
}

// URL vs outside text (2 tests)

#[test]
fn text_outside_url_still_checked() {
    // Text outside URLs should still be checked.
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner
        .scan("使用 javascript 開發，參考 https://javascript.info")
        .issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "javascript");
    assert_eq!(issues[0].suggestions[..], vec!["JavaScript"]);
}

#[test]
fn multiple_urls_all_excluded() {
    // Multiple URLs should all be excluded.
    let scanner = Scanner::new(
        vec![],
        vec![case_rule("Google", None), case_rule("Facebook", None)],
    );
    let issues = scanner
        .scan("造訪 https://google.com 和 http://facebook.com")
        .issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn url_in_markdown_link_excluded_for_case() {
    // URL portion of Markdown links should be excluded.
    let scanner = Scanner::new(vec![], vec![case_rule("Instagram", None)]);
    let issues = scanner
        .scan("這是 [連結](https://instagram.com/user) 的文字")
        .issues;
    assert_eq!(issues.len(), 0);
}

// @mention exclusion (6 tests)

#[test]
fn mention_excludes_spelling_error() {
    // Should ignore spelling errors inside @mentions.
    let scanner = Scanner::new(vec![spelling("test", &["Test"])], vec![]);
    let issues = scanner.scan("提到 @test_user 的使用者").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn mention_excludes_case_error() {
    // Should ignore case errors inside @mentions.
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner.scan("標記 @javascript 開發者").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn mention_supports_underscore_and_digits() {
    // @mentions should support underscores and digits.
    let scanner = Scanner::new(vec![spelling("user", &["User"])], vec![]);
    let issues = scanner.scan("提到 @user_123 和 @test_user").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn text_outside_mention_still_checked() {
    // Text outside @mentions should still be checked.
    let scanner = Scanner::new(vec![spelling("user", &["User"])], vec![]);
    let issues = scanner.scan("提到 @user123 這裡有 user").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "user");
}

#[test]
fn multiple_mentions_all_excluded() {
    // Multiple @mentions should all be excluded.
    let scanner = Scanner::new(
        vec![spelling("user", &["User"])],
        vec![case_rule("JavaScript", None)],
    );
    let issues = scanner.scan("提到 @user1 @user2 和 @admin").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn mention_and_url_both_excluded() {
    // @mentions and URLs should both be excluded.
    let scanner = Scanner::new(
        vec![spelling("user", &["User"])],
        vec![case_rule("Google", None)],
    );
    let issues = scanner.scan("提到 @user 造訪 https://google.com").issues;
    assert_eq!(issues.len(), 0);
}

// Case rule alternatives (9 tests)

#[test]
fn alternatives_accepted() {
    // Should accept valid casing forms listed in alternatives.
    let scanner = Scanner::new(
        vec![],
        vec![case_rule("JavaScript", Some(&["JAVASCRIPT", "javascript"]))],
    );
    let issues = scanner
        .scan("使用 JavaScript, JAVASCRIPT, javascript 開發")
        .issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn form_not_in_alternatives_flagged() {
    // Should flag casing forms not in term or alternatives.
    let scanner = Scanner::new(
        vec![],
        vec![case_rule("JavaScript", Some(&["JAVASCRIPT", "javascript"]))],
    );
    let issues = scanner.scan("使用 JavaScript 和 JavaScrIPT 開發").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "JavaScrIPT");
    assert_eq!(issues[0].suggestions[..], vec!["JavaScript"]);
}

#[test]
fn empty_alternatives_same_as_none() {
    // Empty alternatives array should behave like no alternatives.
    let scanner = Scanner::new(
        vec![],
        vec![CaseRule {
            term: "TypeScript".into(),
            alternatives: Some(vec![]),
            disabled: false,
        }],
    );
    let issues = scanner.scan("使用 TypeScript 和 typescript 開發").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "typescript");
    assert_eq!(issues[0].suggestions[..], vec!["TypeScript"]);
}

#[test]
fn none_alternatives_same_as_no_field() {
    // None alternatives should behave like no alternatives.
    let scanner = Scanner::new(vec![], vec![case_rule("Python", None)]);
    let issues = scanner.scan("使用 Python 和 python 開發").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "python");
    assert_eq!(issues[0].suggestions[..], vec!["Python"]);
}

#[test]
fn undefined_alternatives_same_as_no_field() {
    // None alternatives should behave like no alternatives.
    let scanner = Scanner::new(vec![], vec![case_rule("React", None)]);
    let issues = scanner.scan("使用 React 和 react 開發").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "react");
    assert_eq!(issues[0].suggestions[..], vec!["React"]);
}

#[test]
fn multiple_alternatives_all_accepted() {
    // Should accept all valid alternative forms.
    let scanner = Scanner::new(
        vec![],
        vec![case_rule("API", Some(&["Api", "api", "APIs"]))],
    );
    let issues = scanner.scan("使用 API, Api, api, APIs 開發").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn form_not_in_multiple_alternatives_flagged() {
    // Should flag forms not in the alternatives list.
    let scanner = Scanner::new(vec![], vec![case_rule("API", Some(&["Api", "api"]))]);
    let issues = scanner.scan("使用 API, Api, api, ApI 開發").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "ApI");
    assert_eq!(issues[0].suggestions[..], vec!["API"]);
}

#[test]
fn alternatives_in_code_block_excluded() {
    // Should ignore alternatives case issues inside code blocks.
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", Some(&["javascript"]))]);
    let issues = scanner.scan("這是 `JAVASCRIPT` 的程式碼").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn alternatives_outside_code_block_flagged() {
    // Should flag alternatives case issues outside code blocks.
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", Some(&["javascript"]))]);
    let issues = scanner.scan("這是 JAVASCRIPT 的文字").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "JAVASCRIPT");
    assert_eq!(issues[0].suggestions[..], vec!["JavaScript"]);
}

// Multi-rule interaction (1 test)

#[test]
fn multiple_rules_each_respect_own_alternatives() {
    // Each rule should respect its own alternatives independently.
    let scanner = Scanner::new(
        vec![],
        vec![
            case_rule("JavaScript", Some(&["javascript"])),
            CaseRule {
                term: "TypeScript".into(),
                alternatives: Some(vec!["TypeScript".into()]),
                disabled: false,
            },
        ],
    );
    let issues = scanner
        .scan("使用 JavaScript, javascript, TypeScript, typescript 開發")
        .issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "typescript");
    assert_eq!(issues[0].suggestions[..], vec!["TypeScript"]);
}

// Backtick nesting (2-6 backticks): ported from loop test

#[test]
fn double_backtick_excludes_case() {
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner.scan("這是 ``javascript`` 的文字").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn triple_backtick_inline_excludes_case() {
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner.scan("這是 ```javascript``` 的文字").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn quadruple_backtick_excludes_case() {
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner.scan("這是 ````javascript```` 的文字").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn quintuple_backtick_excludes_case() {
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner.scan("這是 `````javascript````` 的文字").issues;
    assert_eq!(issues.len(), 0);
}

#[test]
fn sextuple_backtick_excludes_case() {
    let scanner = Scanner::new(vec![], vec![case_rule("JavaScript", None)]);
    let issues = scanner.scan("這是 ``````javascript`````` 的文字").issues;
    assert_eq!(issues.len(), 0);
}

// Empty double backtick edge case

#[test]
fn empty_double_backtick_not_excluded() {
    // Empty double backtick should not act as an exclusion barrier. "" with
    // nothing matching is treated as empty inline code.
    let scanner = Scanner::new(vec![spelling("test", &["測試"])], vec![]);
    let issues = scanner.scan("這是 `` 然後這裡有 test 錯誤").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, "test");
    assert_eq!(issues[0].suggestions[..], vec!["測試"]);
}

// Punctuation normalization (half-width → full-width in CJK context)

#[test]
fn punct_comma_in_chinese_text() {
    let scanner = Scanner::new(vec![], vec![]);
    let issues = scanner.scan("蘋果, 香蕉, 和橘子").issues;
    assert_eq!(issues.len(), 2);
    assert!(issues.iter().all(|i| i.found == ","));
    assert!(issues.iter().all(|i| i.suggestions[..] == vec!["，"]));
}

#[test]
fn punct_period_at_sentence_end() {
    let scanner = Scanner::new(vec![], vec![]);
    let issues = scanner.scan("這是一段文字.").issues;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].found, ".");
    assert_eq!(issues[0].suggestions[..], vec!["。"]);
}

#[test]
fn punct_excluded_in_code_block() {
    let scanner = Scanner::new(vec![], vec![]);
    let issues = scanner.scan("請看 `foo, bar.baz` 的範例").issues;
    assert!(issues.is_empty());
}

#[test]
fn punct_excluded_in_triple_code_fence() {
    let scanner = Scanner::new(vec![], vec![]);
    let issues = scanner.scan("```\na, b.\n```").issues;
    assert!(issues.is_empty());
}

#[test]
fn punct_excluded_in_url() {
    let scanner = Scanner::new(vec![], vec![]);
    let issues = scanner
        .scan("請見 https://example.com/a,b.html 的說明")
        .issues;
    assert!(issues.is_empty());
}

#[test]
fn punct_combined_with_spelling_rules() {
    let scanner = Scanner::new(vec![spelling("軟件", &["軟體"])], vec![]);
    let issues = scanner.scan("這個軟件, 很好用.").issues;
    // Should find: 軟件 (spelling) + comma + period
    assert_eq!(issues.len(), 3);
    assert_eq!(issues[0].found, "軟件");
    assert_eq!(issues[1].found, ",");
    assert_eq!(issues[2].found, ".");
}

#[test]
fn punct_english_text_untouched() {
    let scanner = Scanner::new(vec![], vec![]);
    let issues = scanner.scan("Hello, world. How are you?").issues;
    assert!(issues.is_empty());
}

#[test]
fn punct_decimal_number_untouched() {
    let scanner = Scanner::new(vec![], vec![]);

    // Even in Chinese text, decimal numbers should not trigger period
    // conversion.
    let issues = scanner.scan("圓周率是 3.14 左右").issues;
    assert!(issues.is_empty());
}

// Definition-list colon should not be flagged as half-width punctuation

#[test]
fn punct_definition_list_colon_skipped() {
    use zhtw_mcp::engine::scan::ContentType;
    use zhtw_mcp::rules::ruleset::IssueType;
    let scanner = Scanner::new(vec![], vec![]);
    // Markdown definition list: term on one line, ": definition" on the next.
    let text = "尾端延遲\n: 以互補累積分佈函數描述延遲";
    let output = scanner.scan_for_content_type(text, ContentType::Markdown, Profile::Base);
    let colon_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.rule_type == IssueType::Punctuation && i.found == ":")
        .collect();
    assert!(
        colon_issues.is_empty(),
        "definition-list colon should not be flagged: {colon_issues:?}"
    );
}

#[test]
fn punct_definition_list_colon_indented_skipped() {
    use zhtw_mcp::engine::scan::ContentType;
    use zhtw_mcp::rules::ruleset::IssueType;
    let scanner = Scanner::new(vec![], vec![]);
    // Indented definition list (e.g. nested in blockquote or list).
    let text = "術語\n  : 定義內容在此";
    let output = scanner.scan_for_content_type(text, ContentType::Markdown, Profile::Base);
    let colon_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.rule_type == IssueType::Punctuation && i.found == ":")
        .collect();
    assert!(
        colon_issues.is_empty(),
        "indented definition-list colon should not be flagged: {colon_issues:?}"
    );
}

#[test]
fn punct_colon_after_cjk_still_flagged() {
    // A colon after CJK text (not at line start) should still be flagged.
    let scanner = Scanner::new(vec![], vec![]);
    let output = scanner.scan("原因: 這是一個測試");
    let colon_issues: Vec<_> = output.issues.iter().filter(|i| i.found == ":").collect();
    assert_eq!(
        colon_issues.len(),
        1,
        "normal half-width colon should still be flagged"
    );
}

// Grammar scanner: integration through full Scanner pipeline

#[test]
fn grammar_issues_coexist_with_spelling() {
    // 軟件 triggers a cross-strait spelling rule; 是不是…嗎 triggers grammar.
    // Both should appear in output since grammar runs after overlap resolution.
    use zhtw_mcp::rules::ruleset::IssueType;
    let rules = vec![spelling("軟件", &["軟體"])];
    let scanner = Scanner::new(rules, vec![]);
    let output = scanner.scan("你是不是喜歡這個軟件嗎？");
    let has_spelling = output
        .issues
        .iter()
        .any(|i| i.rule_type == IssueType::CrossStrait);
    let has_grammar = output
        .issues
        .iter()
        .any(|i| i.rule_type == IssueType::Grammar);
    assert!(has_spelling, "should have spelling issue for 軟件");
    assert!(has_grammar, "should have grammar issue for 是不是…嗎");
}

#[test]
fn grammar_issues_have_line_col() {
    use zhtw_mcp::rules::ruleset::IssueType;
    let scanner = Scanner::new(vec![], vec![]);
    // Two-line input: grammar issue on the second line.
    let output = scanner.scan("第一行\n你是不是學生嗎？");
    let grammar = output
        .issues
        .iter()
        .find(|i| i.rule_type == IssueType::Grammar);
    assert!(grammar.is_some(), "should produce grammar issue");
    let g = grammar.unwrap();

    // 是不是 starts at byte 10 (第一行\n = 9+1 bytes, then 你 = 3 bytes →
    // offset 13). Line should be 2 (1-based), col should be 2 (UTF-16: 你 is 1
    // code unit → col 2).
    assert_eq!(g.line, 2, "grammar issue should be on line 2");
    assert_eq!(g.col, 2, "grammar issue should start at col 2 (after 你)");
}

#[test]
fn grammar_disabled_with_relaxed() {
    use zhtw_mcp::engine::scan::ContentType;
    use zhtw_mcp::rules::ruleset::IssueType;
    let scanner = Scanner::new(vec![], vec![]);
    let text = "你是不是學生嗎？";
    let cfg = Profile::Base.config().with_relaxed();
    let output = scanner.scan_for_content_type_with_config(text, ContentType::Plain, cfg);
    assert!(
        !output
            .issues
            .iter()
            .any(|i| i.rule_type == IssueType::Grammar),
        "relaxed capability should not produce grammar issues"
    );
}

#[test]
fn grammar_enabled_in_base_profile() {
    use zhtw_mcp::engine::scan::ContentType;
    use zhtw_mcp::rules::ruleset::IssueType;
    let scanner = Scanner::new(vec![], vec![]);
    let text = "你是不是學生嗎？";
    let output = scanner.scan_for_content_type(text, ContentType::Plain, Profile::Base);
    assert!(
        output
            .issues
            .iter()
            .any(|i| i.rule_type == IssueType::Grammar),
        "Base profile should produce grammar issues"
    );
}

#[test]
fn grammar_excluded_in_markdown_code_block() {
    use zhtw_mcp::engine::scan::ContentType;
    use zhtw_mcp::rules::ruleset::IssueType;
    let scanner = Scanner::new(vec![], vec![]);
    let text = "```\n你是不是學生嗎？\n```";
    let output = scanner.scan_for_content_type(text, ContentType::Markdown, Profile::Base);
    assert!(
        !output
            .issues
            .iter()
            .any(|i| i.rule_type == IssueType::Grammar),
        "Grammar issues inside code blocks should be excluded"
    );
}

#[test]
fn grammar_deterministic_sort_order() {
    // 進行 triggers both dui_jinxing (對資料進行分析) and bureaucratic
    // nominalization (進行分析) at overlapping offsets. Verify grammar issues
    // are sorted by offset (ascending), then by length (descending).
    use zhtw_mcp::rules::ruleset::IssueType;
    let scanner = Scanner::new(vec![], vec![]);
    let output = scanner.scan("對資料進行分析");
    let grammar: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.rule_type == IssueType::Grammar)
        .collect();
    assert!(
        grammar.len() >= 2,
        "should have at least 2 grammar issues (dui_jinxing + bureaucratic)"
    );
    // Verify sort: offsets ascending; at same offset, longer span first.
    for pair in grammar.windows(2) {
        assert!(
            pair[0].offset < pair[1].offset
                || (pair[0].offset == pair[1].offset && pair[0].length >= pair[1].length),
            "grammar issues not properly sorted: {:?} before {:?}",
            (pair[0].offset, pair[0].length),
            (pair[1].offset, pair[1].length),
        );
    }
}

#[test]
fn grammar_clean_text_produces_no_issues() {
    let scanner = Scanner::new(vec![], vec![]);
    let output = scanner.scan("台灣是一個美麗的島嶼，有豐富的文化和美食。");
    let has_grammar = output
        .issues
        .iter()
        .any(|i| i.rule_type == zhtw_mcp::rules::ruleset::IssueType::Grammar);
    assert!(!has_grammar, "clean text should not trigger grammar checks");
}

// AI writing detection

use zhtw_mcp::rules::ruleset::IssueType;

fn ai_filler_rule(from: &str, to: &[&str]) -> SpellingRule {
    SpellingRule::new(
        from,
        to.iter().map(|s| s.to_string()).collect(),
        RuleType::AiFiller,
    )
}

#[test]
fn ai_filler_rules_suppressed_in_base_profile() {
    let rules = vec![ai_filler_rule("值得注意的是，", &[""])];
    let scanner = Scanner::new(rules, vec![]);

    // Base profile enables ai_filler_detection by default.
    let output =
        scanner.scan_with_config("值得注意的是，系統需要重啟", &[], Profile::Base.config());
    let ai_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.rule_type == IssueType::AiStyle)
        .collect();
    assert_eq!(
        ai_issues.len(),
        1,
        "AI filler should fire under default Base profile"
    );

    // Explicitly disabling the toggle gates the rule off.
    let mut cfg = Profile::Base.config();
    cfg.ai_filler_detection = false;
    let output = scanner.scan_with_config("值得注意的是，系統需要重啟", &[], cfg);
    let ai_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.rule_type == IssueType::AiStyle)
        .collect();
    assert!(
        ai_issues.is_empty(),
        "AI filler should be gated when explicitly disabled"
    );
}

#[test]
fn ai_filler_rules_fire_when_profile_enabled() {
    let rules = vec![ai_filler_rule("值得注意的是，", &[""])];
    let scanner = Scanner::new(rules, vec![]);
    let mut cfg = Profile::Base.config();
    cfg.ai_filler_detection = true;
    let output = scanner.scan_with_config("值得注意的是，系統需要重啟", &[], cfg);
    let ai_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.rule_type == IssueType::AiStyle)
        .collect();
    assert_eq!(ai_issues.len(), 1);
    assert_eq!(ai_issues[0].found, "值得注意的是，");
}

#[test]
fn ai_semantic_safety_fires_when_profile_enabled() {
    let scanner = Scanner::new(vec![], vec![]);
    let mut cfg = Profile::Base.config();
    cfg.ai_semantic_safety = true;
    let output = scanner.scan_with_config("這個定義意味著所有的值都必須為正", &[], cfg);
    let ai_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.rule_type == IssueType::AiStyle)
        .collect();
    assert_eq!(ai_issues.len(), 1);
    assert_eq!(ai_issues[0].found, "意味著");
}

#[test]
fn ai_semantic_safety_suppressed_in_base_profile() {
    let scanner = Scanner::new(vec![], vec![]);
    let output = scanner.scan("這個定義意味著所有的值都必須為正");
    let ai_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.rule_type == IssueType::AiStyle)
        .collect();
    assert!(
        ai_issues.is_empty(),
        "default profile should suppress AI semantic safety"
    );
}

#[test]
fn ai_density_detection_fires_with_editorial_profile() {
    let scanner = Scanner::new(vec![], vec![]);
    // Build a ~1200 char text with high density of '更重要的是'.
    let filler = "這是正常的技術內容段落。";
    let mut text = String::new();
    for i in 0..100 {
        if i % 20 == 0 {
            text.push_str("更重要的是，我們需要重新評估這個方案。");
        } else {
            text.push_str(filler);
        }
    }
    let mut cfg = Profile::Base.config();
    cfg.ai_filler_detection = true;
    cfg.ai_density_detection = true;
    cfg.ai_semantic_safety = true;
    cfg.ai_structural_patterns = true;
    let output = scanner.scan_for_content_type_with_config(&text, ContentType::Plain, cfg);
    let density_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| {
            i.rule_type == IssueType::AiStyle
                && i.context.as_ref().is_some_and(|c| c.contains("次/千字"))
        })
        .collect();
    assert!(
        !density_issues.is_empty(),
        "detect_ai should detect high density AI phrases"
    );
}

#[test]
fn ai_density_detection_suppressed_in_base_profile() {
    let scanner = Scanner::new(vec![], vec![]);
    let filler = "這是正常的技術內容段落。";
    let mut text = String::new();
    for i in 0..100 {
        if i % 20 == 0 {
            text.push_str("更重要的是，我們需要重新評估這個方案。");
        } else {
            text.push_str(filler);
        }
    }
    let output = scanner.scan_for_content_type(&text, ContentType::Plain, Profile::Base);
    let density_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|i| {
            i.rule_type == IssueType::AiStyle
                && i.context.as_ref().is_some_and(|c| c.contains("次/千字"))
        })
        .collect();
    assert!(
        density_issues.is_empty(),
        "default profile should NOT run density detection"
    );
}

// Four performance bugs in one week shared a shape: an index whose build walks
// the whole document, called from inside a loop over paragraphs. Output is
// identical either way, so only the clock changes and no assertion notices.
// engine::index_guard counts builds per scan and trips in debug builds; this
// drives a real multi-paragraph scan so that counter is actually evaluated.
#[test]
fn document_scoped_indexes_are_built_once_per_scan() {
    let ruleset: zhtw_mcp::rules::ruleset::Ruleset =
        serde_json::from_str(include_str!("../assets/ruleset.json")).expect("ruleset parses");
    let scanner = Scanner::new(ruleset.spelling_rules, ruleset.case_rules);

    // Many paragraphs, many sentences, and the shapes that make the indexes get
    // built at all: a closing phrase, an attribution, a citation.
    let unit = "這個模組需要最佳化。研究顯示成果很好。專家認為這項安排可行[1]。\n\n\
                總的來說，這是一個值得注意的現象。綜上所述，效能已明顯改善。\n\n";
    let text = unit.repeat(40);

    let mut cfg = Profile::Strict.config();
    cfg.ai_filler_detection = true;
    cfg.ai_semantic_safety = true;
    cfg.ai_density_detection = true;
    cfg.ai_structural_patterns = true;
    cfg.translationese_detection = true;

    // The assertion lives at the end of the scan itself, so reaching this line
    // without a panic is the check. Confirm the scan did real work, or a future
    // change that stopped scanning would pass this silently.
    let excluded =
        zhtw_mcp::engine::scan::build_exclusions_for_content_type(&text, ContentType::Markdown);
    let out =
        scanner.scan_with_prebuilt_excluded_config(&text, &excluded, cfg, ContentType::Markdown);
    assert!(
        !out.issues.is_empty(),
        "the fixture must produce findings, or the guard sees an empty scan"
    );
}
