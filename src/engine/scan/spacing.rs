// CJK spacing rules from Chinese Copywriting Guidelines.
//
// 1. Space between CJK and half-width Latin characters
// 2. Space between CJK and digits (except °, %)
// 3. No space adjacent to full-width punctuation
// 4. No repeated full-width punctuation marks
// 5. Full-width digits → half-width
//
// Rules 1 and 2 are the two halves of one boundary policy, so SpacingPolicy
// decides which way they read: Require reports the missing space, Strip reports
// the stored space as removable. Rules 3, 4 and 5 are not boundary policy and
// run identically under both.

use super::emit::Emitter;
use std::iter::Peekable;
use std::str::CharIndices;

use crate::engine::excluded::{is_excluded, ByteRange};
use crate::rules::ruleset::{Issue, IssueType, ProfileConfig, Severity, SpacingPolicy};

use super::{is_cjk_ideograph, punct_issue_sev};

/// Create a zero-length insertion issue (missing space) at the given boundary.
fn missing_space_issue(boundary: usize, context: &str) -> Issue {
    punct_issue_sev(boundary, "", " ", context, Severity::Info)
}

/// Create an issue for unwanted spaces that should be removed.
fn unwanted_space_issue(offset: usize, space_len: usize, context: &str) -> Issue {
    Issue::new(
        offset,
        space_len,
        " ".repeat(space_len),
        vec!["".into()],
        IssueType::Punctuation,
        Severity::Info,
    )
    .with_context(context)
}

/// True if ch is a full-width CJK punctuation mark (，。！？；：、「」
/// 『』（）【】《》〈〉——…… etc.).
fn is_fullwidth_punct(ch: char) -> bool {
    matches!(ch,
        '\u{3001}'..='\u{3003}' | // 、。〃
        '\u{3008}'..='\u{3011}' | // 〈〉《》「」『』【】
        '\u{3014}'..='\u{301B}' | // 〔〕〖〗〘〙〚〛
        '\u{FF01}' | // ！
        '\u{FF08}' | // （
        '\u{FF09}' | // ）
        '\u{FF0C}' | // ，
        '\u{FF0E}' | // ．
        '\u{FF1A}' | // ：
        '\u{FF1B}' | // ；
        '\u{FF1F}' | // ？
        '\u{2014}' | // —
        '\u{2026}'   // …
    )
}

/// True if ch is a full-width digit (０-９).
fn is_fullwidth_digit(ch: char) -> bool {
    matches!(ch, '\u{FF10}'..='\u{FF19}')
}

/// Convert a full-width digit to its half-width equivalent.
fn fullwidth_to_halfwidth_digit(ch: char) -> char {
    debug_assert!(is_fullwidth_digit(ch));
    // Safe: fullwidth digits U+FF10..U+FF19 map to U+0030..U+0039.
    char::from_u32(ch as u32 - 0xFF10 + '0' as u32).expect("U+FF10..U+FF19 maps into ASCII 0-9")
}

impl super::Scanner {
    /// Scan for CJK spacing violations (Chinese Copywriting Guidelines).
    ///
    /// Detects:
    /// - Missing space between CJK and Latin characters (rule 1)
    /// - Missing space between CJK and digits (rule 2, except °/%)
    /// - Unwanted space adjacent to full-width punctuation (rule 3)
    /// - Repeated full-width punctuation marks (rule 4)
    /// - Full-width digits that should be half-width (rule 5)
    ///
    /// Takes the whole config rather than the one axis it reads today, the
    /// same shape as scan_punctuation: a second spacing axis then costs a
    /// field rather than a parameter at every call site.
    pub(crate) fn scan_spacing(&self, em: &mut Emitter<'_>, cfg: &ProfileConfig) {
        let text = em.text;
        let excluded = em.excluded;
        let issues = &mut *em.issues;

        if text.is_empty() {
            return;
        }

        // Sliding window: prev/curr/next chars with byte offsets. Avoids
        // materializing the full Vec<(usize, char)>.
        let mut iter = text.char_indices().peekable();
        let mut prev: Option<(usize, char)> = None;
        // Track consecutive identical punct run length for rule 4.
        let mut same_punct_run: usize = 0;

        while let Some((offset, ch)) = iter.next() {
            let ch_len = ch.len_utf8();
            let excluded_ch = is_excluded(offset, offset + ch_len, excluded);

            // Update punct run tracking (independent of exclusion).
            if !excluded_ch && is_fullwidth_punct(ch) {
                if prev.is_some_and(|(_, pc)| pc == ch) {
                    same_punct_run += 1;
                } else {
                    same_punct_run = 0;
                }
            } else {
                same_punct_run = 0;
            }

            if !excluded_ch {
                let ctx = CharCtx {
                    offset,
                    ch,
                    prev,
                    same_punct_run,
                    excluded,
                    policy: cfg.spacing_policy,
                };
                check_char(&mut iter, &ctx, issues);
            }

            // Single update point for prev, which is why the per-character
            // checks live in a function rather than a continue statement.
            prev = Some((offset, ch));
        }
    }
}

/// One character and everything the rules at that position read. The rules
/// took a parameter each before the boundary policy joined them, which put
/// the function over the argument limit; a context also mirrors how the
/// lexical and structural passes carry their state.
struct CharCtx<'a> {
    offset: usize,
    ch: char,
    prev: Option<(usize, char)>,
    same_punct_run: usize,
    excluded: &'a [ByteRange],
    policy: SpacingPolicy,
}

/// Apply rules 1 to 5 at one character. Rules 4 and 5 are mutually exclusive
/// with the adjacency rules, so each match returns; the boundary policy is
/// not, so it never returns from here and rule 3 always gets its turn.
fn check_char(iter: &mut Peekable<CharIndices<'_>>, ctx: &CharCtx<'_>, issues: &mut Vec<Issue>) {
    // Rule 5: full-width digits should be half-width.
    if is_fullwidth_digit(ctx.ch) {
        let hw = fullwidth_to_halfwidth_digit(ctx.ch);
        issues.push(punct_issue_sev(
            ctx.offset,
            &ctx.ch.to_string(),
            &hw.to_string(),
            "數字應使用半形字元",
            Severity::Warning,
        ));
        return;
    }

    // Rule 4: repeated full-width punctuation. Paired punct (…… and ——) is
    // allowed at exactly two, so it only trips from the third onward.
    if is_fullwidth_punct(ctx.ch) && ctx.same_punct_run > 0 {
        let limit = if is_paired_punct(ctx.ch) { 2 } else { 1 };
        if ctx.same_punct_run >= limit {
            issues.push(punct_issue_sev(
                ctx.offset,
                &ctx.ch.to_string(),
                "",
                "不重複使用標點符號",
                Severity::Warning,
            ));
        }
        return;
    }

    // Rules 1 to 3 all compare against the following character.
    let Some(&(next_offset, next_ch)) = iter.peek() else {
        return;
    };
    if is_excluded(next_offset, next_offset + next_ch.len_utf8(), ctx.excluded) {
        return;
    }

    match ctx.policy {
        SpacingPolicy::Require => check_missing_boundary_space(ctx, next_ch, issues),
        SpacingPolicy::Strip => {
            check_stored_boundary_space(iter, ctx, next_offset, next_ch, issues)
        }
    }

    // Rule 3: no space on either side of full-width punctuation. Reached under
    // both policies, whatever the boundary check above made of this character.
    check_space_before_punct(iter, ctx, issues);
    check_space_after_punct(iter, ctx.ch, next_offset, next_ch, issues);
}

/// Rules 1 and 2 under `Require`: the boundary is missing its space.
fn check_missing_boundary_space(ctx: &CharCtx<'_>, next_ch: char, issues: &mut Vec<Issue>) {
    // Rule 1: CJK immediately adjacent to Latin.
    if is_cjk_latin_boundary(ctx.ch, next_ch) {
        issues.push(missing_space_issue(
            ctx.offset + ctx.ch.len_utf8(),
            "中英文之間需要增加空格",
        ));
    }

    // Rule 2: CJK immediately adjacent to a digit.
    if is_cjk_digit_boundary(ctx.ch, next_ch) {
        issues.push(missing_space_issue(
            ctx.offset + ctx.ch.len_utf8(),
            "中文與數字之間需要增加空格",
        ));
    }
}

/// Rules 1 and 2 under `Strip`: the boundary stores a space that the policy
/// hands to the renderer instead. Rules 1 and 2 only see characters that are
/// already adjacent, so the boundary has to be found by looking past the
/// space run.
///
/// Gated on the left character first: the boundary predicates below both need
/// it to be CJK or ASCII alphanumeric, which is knowable without walking, and
/// under this policy every space in the document reaches here.
fn check_stored_boundary_space(
    iter: &Peekable<CharIndices<'_>>,
    ctx: &CharCtx<'_>,
    next_offset: usize,
    next_ch: char,
    issues: &mut Vec<Issue>,
) {
    if !is_boundary_space(next_ch) || !can_open_boundary(ctx.ch) {
        return;
    }
    let Some((following_offset, following_ch)) = space_run_target(iter) else {
        return;
    };
    if is_excluded(
        following_offset,
        following_offset + following_ch.len_utf8(),
        ctx.excluded,
    ) {
        return;
    }
    let context = if is_cjk_latin_boundary(ctx.ch, following_ch) {
        "中英文之間不加空格"
    } else if is_cjk_digit_boundary(ctx.ch, following_ch) {
        "中文與數字之間不加空格"
    } else {
        return;
    };
    issues.push(unwanted_space_issue(
        next_offset,
        following_offset - next_offset,
        context,
    ));
}

/// True for the one character this file treats as a stored space.
///
/// U+0020 and nothing else: it is what `Require` inserts, what rule 3 removes
/// and what `Strip` takes back out, so the three agree on what a space is.
/// U+00A0 and U+3000 are typography an author chose rather than a boundary
/// gap, and a tab or a newline is layout. Deliberately narrower than
/// `adjacent_cjk`, which skips every Unicode whitespace.
fn is_boundary_space(ch: char) -> bool {
    ch == ' '
}

/// True if `ch` can be the left side of a CJK/Latin or CJK/digit boundary.
fn can_open_boundary(ch: char) -> bool {
    is_cjk_ideograph(ch) || ch.is_ascii_alphanumeric()
}

/// What the space run starting at `iter`'s peeked position leads to.
///
/// The caller has already peeked the first space, so this skips it and reports
/// the first non-space character with its offset. Rules 1, 2 and 3 all measure
/// their span as that offset minus the run's start.
fn space_run_target(iter: &Peekable<CharIndices<'_>>) -> Option<(usize, char)> {
    let mut fwd = iter.clone();
    fwd.next();
    next_non_space(fwd)
}

fn is_cjk_latin_boundary(left: char, right: char) -> bool {
    (is_cjk_ideograph(left) && right.is_ascii_alphabetic())
        || (left.is_ascii_alphabetic() && is_cjk_ideograph(right))
}

fn is_cjk_digit_boundary(left: char, right: char) -> bool {
    (is_cjk_ideograph(left) && right.is_ascii_digit())
        || (left.is_ascii_digit() && is_cjk_ideograph(right))
}

/// Rule 3, leading half: a space run between content and full-width punct.
fn check_space_before_punct(
    iter: &Peekable<CharIndices<'_>>,
    ctx: &CharCtx<'_>,
    issues: &mut Vec<Issue>,
) {
    if !is_boundary_space(ctx.ch) {
        return;
    }
    let Some((_, content_ch)) = ctx.prev.filter(|&(_, pc)| !is_boundary_space(pc)) else {
        return;
    };
    if !can_open_boundary(content_ch) {
        return;
    }
    let Some((punct_offset, punct_ch)) = next_non_space(iter.clone()) else {
        return;
    };
    if !is_fullwidth_punct(punct_ch) {
        return;
    }
    issues.push(unwanted_space_issue(
        ctx.offset,
        punct_offset - ctx.offset,
        "全形標點與其他字元之間不加空格",
    ));
}

/// Rule 3, trailing half: a space run between full-width punct and content.
fn check_space_after_punct(
    iter: &Peekable<CharIndices<'_>>,
    ch: char,
    next_offset: usize,
    next_ch: char,
    issues: &mut Vec<Issue>,
) {
    if !is_fullwidth_punct(ch) || !is_boundary_space(next_ch) {
        return;
    }
    let Some((content_offset, content_ch)) = space_run_target(iter) else {
        return;
    };
    if !can_open_boundary(content_ch) {
        return;
    }
    issues.push(unwanted_space_issue(
        next_offset,
        content_offset - next_offset,
        "全形標點與其他字元之間不加空格",
    ));
}

/// First non-space character at or after the position of `fwd`.
fn next_non_space(fwd: Peekable<CharIndices<'_>>) -> Option<(usize, char)> {
    fwd.into_iter().find(|&(_, ch)| ch != ' ')
}

/// True for punctuation that is legitimately used in pairs (…… and ——).
fn is_paired_punct(ch: char) -> bool {
    ch == '\u{2026}' || ch == '\u{2014}'
}

#[cfg(test)]
#[path = "../../../tests/unit/engine/scan/spacing/tests.rs"]
mod tests;
