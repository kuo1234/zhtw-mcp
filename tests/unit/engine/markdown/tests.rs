use super::*;

#[test]
fn block_boundaries_are_normalized_to_source_line_starts() {
    let md = "前言。\n\n   # 標題\n\n  - 項目\n\n > 引文\n\n```sh\necho ok\n```\n\n---\n";
    let starts = block_boundary_starts(md);
    for line in ["   # 標題", "---"] {
        let start = md.find(line).unwrap();
        assert!(
            starts.binary_search(&start).is_ok(),
            "missing {line:?}: {starts:?}"
        );
    }

    // A list, blockquote, or fence opens a block inside the section, not a new
    // section. The sentence before it is a lead-in, not a closer.
    for line in ["  - 項目", " > 引文", "```sh"] {
        let start = md.find(line).unwrap();
        assert!(
            starts.binary_search(&start).is_err(),
            "{line:?} must not end a section: {starts:?}"
        );
    }
    assert!(starts.binary_search(&0).is_err());
}

#[test]
fn nested_headings_and_rules_are_not_section_boundaries() {
    for md in [
        "本節總結。展望未來。\n\n> # 引用裡的標題\n",
        "本節總結。展望未來。\n\n> ---\n",
        "本節總結。展望未來。\n\n- 項目\n\n  # 清單裡的標題\n",
    ] {
        let starts = block_boundary_starts(md);
        assert!(
            starts.is_empty(),
            "nested construct opened a section: {md:?} -> {starts:?}"
        );
    }
}

#[test]
fn frontmatter_is_not_a_section_boundary() {
    let md = "---\ntitle: t\n---\n\n本節總結。展望未來。\n\n# 下一節\n";
    let starts = block_boundary_starts(md);
    let heading = md.find("# 下一節").unwrap();
    assert_eq!(
        starts,
        vec![heading],
        "only the real heading closes a section: {starts:?}"
    );
}

#[test]
fn a_heading_shaped_line_inside_a_fence_is_not_a_boundary() {
    // The reason this index comes from the parser rather than a line prefix
    // test.
    let md = "前言。\n\n```sh\n# install\nmake\n```\n";
    let starts = block_boundary_starts(md);
    assert!(
        starts.is_empty(),
        "a comment inside a fence opened a section: {starts:?}"
    );
}

#[test]
fn fenced_code_block_excluded() {
    let md = "前言\n```rust\nlet x = 1;\n```\n後語\n";
    let ranges = build_markdown_excluded_ranges(md);
    assert!(!ranges.is_empty());
    let excluded_text: String = ranges
        .iter()
        .map(|r| &md[r.start..r.end])
        .collect::<Vec<_>>()
        .join("");
    assert!(excluded_text.contains("let x = 1;"));
    assert!(!excluded_text.contains("前言"));
    assert!(!excluded_text.contains("後語"));
}

#[test]
fn inline_code_excluded() {
    let md = "使用 `println!` 來輸出\n";
    let ranges = build_markdown_excluded_ranges(md);
    assert!(!ranges.is_empty());
    let any_covers_println = ranges
        .iter()
        .any(|r| md[r.start..r.end].contains("println"));
    assert!(any_covers_println);
}

#[test]
fn yaml_frontmatter_keys_excluded_values_scannable() {
    let md = "---\ntitle: 測試\ndate: 2024-01-01\n---\n正文開始\n";
    let ranges = build_markdown_excluded_ranges(md);
    assert!(!ranges.is_empty());
    // Key+colon is excluded.
    let any_covers_title_key = ranges.iter().any(|r| md[r.start..r.end].contains("title:"));
    assert!(any_covers_title_key, "title: key should be excluded");
    // Value text is NOT excluded: it's prose to be scanned.
    let value_excluded = ranges.iter().any(|r| md[r.start..r.end].contains("測試"));
    assert!(
        !value_excluded,
        "frontmatter value 測試 should be scannable"
    );
    // Body is not excluded.
    let body_excluded = ranges
        .iter()
        .any(|r| md[r.start..r.end].contains("正文開始"));
    assert!(!body_excluded);
    // Closing --- fence is excluded.
    let close_excluded = ranges.iter().any(|r| md[r.start..r.end].trim() == "---");
    assert!(close_excluded, "closing --- fence should be excluded");
}

#[test]
fn extract_heading_ranges_skips_frontmatter_setext() {
    // pulldown-cmark synthesises a setext H2 from frontmatter content + closing
    // "---". We must skip that false-positive heading so the severity boost
    // does not apply to frontmatter values.
    let md = "---\ntitle: 軟件測試指南\ndate: 2026\n---\n# 真標題\n正文。\n";
    let ranges = extract_heading_ranges(md);

    // Should contain the real heading "真標題" but NOT the synthesised
    // frontmatter heading.
    for r in &ranges {
        let span = &md[r.start..r.end];
        assert!(
            !span.contains("title:"),
            "frontmatter content leaked into heading ranges: {span:?}"
        );
    }
    let real_heading_present = ranges.iter().any(|r| md[r.start..r.end].contains("真標題"));
    assert!(
        real_heading_present,
        "real heading '真標題' should still be detected: {ranges:?}"
    );
}

#[test]
fn yaml_frontmatter_not_in_middle() {
    let md = "前言\n---\ntitle: 測試\n---\n後語\n";
    let ranges = build_markdown_excluded_ranges(md);
    // --- in the middle is not frontmatter (it's a thematic break).
    let any_covers_title = ranges.iter().any(|r| md[r.start..r.end].contains("title:"));
    assert!(!any_covers_title);
}

#[test]
fn html_block_excluded() {
    let md = "前言\n<div>some html</div>\n後語\n";
    let ranges = build_markdown_excluded_ranges(md);
    let any_covers_html = ranges.iter().any(|r| md[r.start..r.end].contains("<div>"));
    assert!(any_covers_html);
}

#[test]
fn nested_list_with_code() {
    let md = "- 項目一\n  - `code` 子項目\n- 項目二\n";
    let ranges = build_markdown_excluded_ranges(md);
    let any_covers_code = ranges.iter().any(|r| md[r.start..r.end].contains("code"));
    assert!(any_covers_code);
    // List text should not be excluded.
    let any_covers_item = ranges.iter().any(|r| md[r.start..r.end].contains("項目一"));
    assert!(!any_covers_item);
}

#[test]
fn blockquote_with_code() {
    let md = "> 引用文字 `inline` 繼續\n";
    let ranges = build_markdown_excluded_ranges(md);
    let any_covers_inline = ranges.iter().any(|r| md[r.start..r.end].contains("inline"));
    assert!(any_covers_inline);
    let any_covers_quote = ranges
        .iter()
        .any(|r| md[r.start..r.end].contains("引用文字"));
    assert!(!any_covers_quote);
}

#[test]
fn empty_input() {
    let ranges = build_markdown_excluded_ranges("");
    assert!(ranges.is_empty());
}

#[test]
fn plain_text_no_exclusions() {
    let md = "這是純文字，沒有任何 Markdown 語法。\n";
    let ranges = build_markdown_excluded_ranges(md);
    assert!(ranges.is_empty());
}

#[test]
fn code_block_with_language_tag() {
    let md = "```python\nprint('hello')\n```\n";
    let ranges = build_markdown_excluded_ranges(md);
    assert!(!ranges.is_empty());
    let excluded: String = ranges
        .iter()
        .map(|r| &md[r.start..r.end])
        .collect::<Vec<_>>()
        .join("");
    assert!(excluded.contains("print('hello')"));
}

#[test]
fn multiple_code_blocks() {
    let md = "文字\n```\nblock1\n```\n中間\n```\nblock2\n```\n結尾\n";
    let ranges = build_markdown_excluded_ranges(md);
    assert!(ranges.len() >= 2);
}

#[test]
fn container_fence_lines_excluded() {
    // The :::warning and ::: fence lines must be excluded. The prose content
    // between them must NOT be excluded.
    let md = "前言\n:::warning\n這是警告內容，請注意：細節。\n:::\n後語\n";
    let ranges = build_markdown_excluded_ranges(md);
    let any_covers_open_fence = ranges
        .iter()
        .any(|r| md[r.start..r.end].contains(":::warning"));
    assert!(
        any_covers_open_fence,
        "opening fence line should be excluded"
    );
    let any_covers_close_fence = ranges.iter().any(|r| {
        let s = &md[r.start..r.end];
        s.trim() == ":::"
    });
    assert!(
        any_covers_close_fence,
        "closing fence line should be excluded"
    );
    // Prose content between fences must remain scannable.
    let prose_excluded = ranges
        .iter()
        .any(|r| md[r.start..r.end].contains("警告內容"));
    assert!(
        !prose_excluded,
        "prose content between fences must not be excluded"
    );
}

#[test]
fn container_fence_four_colons() {
    // :::: (4-colon) fences must also be excluded.
    let md = "文字\n::::note\n注意事項\n::::\n後語\n";
    let ranges = build_markdown_excluded_ranges(md);
    let any_covers_open = ranges
        .iter()
        .any(|r| md[r.start..r.end].contains("::::note"));
    assert!(any_covers_open, "4-colon opening fence should be excluded");
}

// Tests for build_yaml_excluded_ranges

#[test]
fn yaml_key_colon_excluded() {
    let yaml = "title: 繁體中文文件\nsummary: 說明文字\n";
    let ranges = build_yaml_excluded_ranges(yaml);
    // "title:" should be excluded.
    let any_covers_title_colon = ranges
        .iter()
        .any(|r| yaml[r.start..r.end].contains("title:"));
    assert!(any_covers_title_colon, "YAML key colon must be excluded");
    // The value "繁體中文文件" must NOT be excluded.
    let value_excluded = ranges
        .iter()
        .any(|r| yaml[r.start..r.end].contains("繁體中文文件"));
    assert!(!value_excluded, "YAML value must remain scannable");
}

#[test]
fn yaml_key_with_spaces_before_colon() {
    let yaml = "title  : 文字\n";
    let ranges = build_yaml_excluded_ranges(yaml);
    let covers_key = ranges
        .iter()
        .any(|r| yaml[r.start..r.end].contains("title  :"));
    assert!(covers_key, "key with spaces before colon must be excluded");
}

#[test]
fn yaml_pure_list_items_not_excluded() {
    // Pure list items with no key-value colon are not excluded.
    let yaml = "- 項目一\n- 項目二\n";
    let ranges = build_yaml_excluded_ranges(yaml);
    assert!(
        ranges.is_empty(),
        "pure list items (no colon) must not be excluded"
    );
}

#[test]
fn yaml_list_mapping_key_excluded() {
    // - key: value, the key colon inside a list item must be excluded.
    let yaml = "- name: 測試\n- label: 標籤\n";
    let ranges = build_yaml_excluded_ranges(yaml);

    // Each list mapping line should have one excluded range covering "- name:"
    // / "- label:".
    let covers_name = ranges
        .iter()
        .any(|r| yaml[r.start..r.end].contains("name:"));
    assert!(covers_name, "key colon inside list item must be excluded");
    // Values must remain scannable.
    let value_excluded = ranges.iter().any(|r| yaml[r.start..r.end].contains("測試"));
    assert!(!value_excluded, "list mapping value must remain scannable");
}

#[test]
fn yaml_hyphenated_key_excluded() {
    let yaml = "key-name: 值\n";
    let ranges = build_yaml_excluded_ranges(yaml);
    let covers = ranges
        .iter()
        .any(|r| yaml[r.start..r.end].contains("key-name:"));
    assert!(covers, "hyphenated key must be excluded");
}

#[test]
fn yaml_indented_key_excluded() {
    let yaml = "outer:\n  inner: 值\n";
    let ranges = build_yaml_excluded_ranges(yaml);
    // "outer:" at col 0 and "  inner:" (indented) both excluded.
    let covers_outer = ranges
        .iter()
        .any(|r| yaml[r.start..r.end].contains("outer:"));
    let covers_inner = ranges
        .iter()
        .any(|r| yaml[r.start..r.end].contains("inner:"));
    assert!(covers_outer, "top-level key must be excluded");
    assert!(covers_inner, "indented key must be excluded");
}

// lang-scoped exclusion. The tracker's own rules are covered in html_lang.rs;
// what these check is this file's seam, that a pulldown-cmark HTML event
// reaches it with the document offsets it needs.

/// Whether one excluded range covers the given substring.
fn excluded_covers(md: &str, needle: &str) -> bool {
    let ranges = build_markdown_excluded_ranges(md);
    let start = md.find(needle).expect("needle present in fixture");
    let end = start + needle.len();
    ranges.iter().any(|r| r.start <= start && r.end >= end)
}

#[test]
fn block_level_lang_en_excludes_the_prose_between_the_tags() {
    let md = "<div lang=\"en\">\n\nDo one thing, and do it well, 對吧。\n\n</div>\n";
    assert!(excluded_covers(md, "Do one thing, and do it well, 對吧。"));
}

#[test]
fn inline_lang_en_span_excludes_its_run() {
    let md = "他說<span lang=\"en\">I agree, 但</span>結束\n";
    assert!(excluded_covers(md, "I agree, 但"));
    assert!(!excluded_covers(md, "結束"));
}

#[test]
fn a_zh_tw_span_is_still_scanned() {
    let md = "他說<span lang=\"zh-TW\">好, 對</span>結束\n";
    assert!(!excluded_covers(md, "好, 對"));
}

#[test]
fn a_lang_tag_written_inside_a_fence_scopes_nothing() {
    let md = "```html\n<span lang=\"en\">\n```\n\n他說, 對吧。\n";
    assert!(!excluded_covers(md, "他說, 對吧。"));
}

// Link syntax exclusion
//
// A destination is an address, and the spacing rule writing the space it is
// right to want between CJK and Latin turns a working anchor into a dead one.
// These hold the line between what a reader sees, which stays in the scan, and
// what a parser reads, which comes out of it.

/// True when every byte of `needle` in `md` falls inside some excluded range.
fn excluded(md: &str, needle: &str) -> bool {
    let at = md
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not in fixture"));
    let ranges = build_markdown_excluded_ranges(md);
    (at..at + needle.len()).all(|i| ranges.iter().any(|r| i >= r.start && i < r.end))
}

/// True when no byte of `needle` in `md` falls inside any excluded range.
fn scanned(md: &str, needle: &str) -> bool {
    let at = md
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not in fixture"));
    let ranges = build_markdown_excluded_ranges(md);
    (at..at + needle.len()).all(|i| !ranges.iter().any(|r| i >= r.start && i < r.end))
}

#[test]
fn inline_link_destination_is_excluded_and_its_text_is_not() {
    let md = "見 [說明文字](#中文錨點abc) 一節。\n";
    assert!(excluded(md, "#中文錨點abc"), "the address is not prose");
    assert!(
        scanned(md, "說明文字"),
        "the link text is what a reader sees"
    );
    assert!(scanned(md, "一節"), "prose after the link is untouched");
}

#[test]
fn image_destination_is_excluded_and_its_alt_text_is_not() {
    let md = "![圖說abc](圖片檔abc.png)\n";
    assert!(excluded(md, "圖片檔abc.png"));
    assert!(scanned(md, "圖說abc"), "alt text is prose");
}

#[test]
fn angle_bracketed_destination_is_excluded_with_its_brackets() {
    // The bracketed form is the only one that may hold a space, and the
    // brackets are what say so.
    let md = "[文字](<有 空格abc.md>)\n";
    assert!(excluded(md, "<有 空格abc.md>"));
}

#[test]
fn destination_holding_balanced_parens_is_excluded_whole() {
    // The backward scan has to pass the link text's own parens without
    // stopping, and stop at the delimiter rather than inside the address.
    let md = "[a (b) c](路徑abc(x).md)\n";
    assert!(excluded(md, "路徑abc(x).md"));
    assert!(scanned(md, "a (b) c"));
}

#[test]
fn escaped_paren_does_not_close_the_destination_early() {
    let md = "[文字](路徑abc\\).md)\n";
    assert!(excluded(md, "路徑abc\\).md"));
}

#[test]
fn title_words_stay_in_the_scan_and_its_quotes_do_not() {
    // The words are a tooltip a reader sees. The quotes are syntax, and the
    // punctuation rule rewriting them to corner brackets stops the link parsing
    // at all.
    let md = "[文字](路徑.md \"標題裡的軟件\")\n";
    assert!(scanned(md, "標題裡的軟件"), "a title is prose");
    let at = md.find('"').unwrap();
    let ranges = build_markdown_excluded_ranges(md);
    assert!(
        ranges.iter().any(|r| r.start == at && r.end == at + 1),
        "the opening quote is syntax: {ranges:?}"
    );
}

#[test]
fn reference_label_is_excluded_and_the_link_text_is_not() {
    let md = "見 [說明文字][錨點abc] 一節。\n\n[錨點abc]: docs/x.md\n";
    assert!(excluded(md, "[錨點abc]"), "a label is an identifier");
    assert!(scanned(md, "說明文字"));
}

#[test]
fn a_shortcut_reference_is_its_own_text_and_stays_in_the_scan() {
    // [label] with no second bracket renders the label itself, so excluding it
    // would hide the one place the reader actually reads it.
    let md = "見 [中文標籤abc] 一節。\n\n[中文標籤abc]: docs/x.md\n";
    let at = md.find("中文標籤abc").expect("fixture");
    let ranges = build_markdown_excluded_ranges(md);
    assert!(
        !ranges.iter().any(|r| at >= r.start && at < r.end),
        "the visible occurrence must stay scannable: {ranges:?}"
    );
}

#[test]
fn definition_label_and_destination_are_excluded() {
    // The definition never reaches the event stream, so without its own pass
    // the reference form is only half covered: breaking the definition breaks
    // the link exactly as breaking the reference does.
    let md = "見 [說明][錨點abc] 一節。\n\n[錨點abc]: docs/中文檔abc.md\n";
    assert!(excluded(md, "[錨點abc]: docs/中文檔abc.md"));
}

#[test]
fn definition_title_words_stay_in_the_scan() {
    let md = "[標籤]: docs/x.md \"定義裡的軟件\"\n";
    assert!(scanned(md, "定義裡的軟件"));
}

#[test]
fn prose_that_merely_looks_like_a_link_is_untouched() {
    // Every range here comes from a link pulldown-cmark resolved, so a bracket
    // and a paren sitting next to each other in ordinary prose cannot produce
    // one.
    let md = "這個軟件（見附錄）在 [方括號] 之後還有 (圓括號abc) 的文字。\n";
    assert!(scanned(md, "軟件"));
    assert!(scanned(md, "圓括號abc"));
    assert!(scanned(md, "方括號"));
}
