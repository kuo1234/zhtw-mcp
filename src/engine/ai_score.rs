// Document-level AI signature scoring.
//
// Aggregates per-occurrence issues, density data, and structural pattern counts
// into a single composite score. Deterministic, no ML.
//
// The score is a weighted sum of normalized density ratios and structural
// indicators. Each signal contributes proportionally to how far it exceeds its
// human baseline threshold.

use serde::{Deserialize, Serialize};

use crate::engine::excluded::{is_excluded, ByteRange};
use crate::engine::scan::is_cjk_ideograph;
use crate::rules::ruleset::{Issue, IssueType, StructuralFamily};

// Density thresholds: (phrase, human_baseline, threshold, weight). Weight
// controls contribution to the composite score.
const DENSITY_SIGNALS: &[(&str, f32, f32, f32)] = &[
    ("更重要的是", 0.3, 0.5, 1.0),
    ("值得注意的是", 0.2, 0.3, 1.0),
    ("這意味著", 0.3, 0.5, 0.8),
    ("不容忽視", 0.1, 0.2, 0.7),
    ("深刻影響", 0.2, 0.3, 0.8),
    ("從某種意義上", 0.1, 0.2, 0.6),
    ("從某種程度上", 0.1, 0.2, 0.6),
    ("需要注意的是", 0.2, 0.3, 0.8),
    ("在某種程度上", 0.1, 0.2, 0.6),
    ("在這個過程中", 0.2, 0.3, 0.7),
];

/// A single signal contributing to the AI signature score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMarker {
    pub pattern: String,
    pub count: usize,
    pub density: f32,
    pub threshold: f32,
    pub expected_baseline: f32,
}

/// Aggregated AI writing signature report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSignatureReport {
    /// Composite density of the tracked signals, 0.0 to 1.0.
    ///
    /// Not an authorship verdict. It measures phrase density and rhythm
    /// uniformity, both of which a careful writer or an over-edited corporate
    /// document produces without any model involved. "top_signals" carries the
    /// evidence, and the evidence is the point: read this as a diagnosis of
    /// what the prose does, not as a claim about who wrote it.
    pub score: f32,
    /// Individual signals that contributed to the score.
    pub markers: Vec<AiMarker>,
    /// Top 3 contributing signal descriptions.
    pub top_signals: Vec<String>,
    /// Sentence length variability (standard deviation in chars).
    /// Low values indicate AI monotony. None if too few sentences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentence_variability: Option<f32>,
    /// Count of zero-width tokenizer artifacts detected.
    pub zero_width_count: usize,
    /// Punctuation density matrix with per-type CV.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub punctuation_profile: Option<PunctuationProfile>,
}

// Terminal punctuation marks that delimit Chinese sentences.
const SENTENCE_TERMINATORS: &[char] = &['。', '！', '？', '!', '?'];

/// Whether a code point can legally participate in an emoji ZWJ sequence.
/// Kept deliberately small and range-based: this is a guard against deleting
/// a valid family/profession emoji, not an attempt to classify every emoji.
///
/// Digits, "#" and "*" are deliberately absent even though they are emoji
/// bases. They are keycap bases, and a keycap is "base FE0F 20E3" with no
/// joiner anywhere, so admitting them buys no valid sequence and lets real
/// residue through: "2<ZWJ>024" would read as an emoji rather than as the
/// tokenizer artifact it is.
fn is_emoji_base(ch: char) -> bool {
    // Sub-blocks rather than the whole plane. 1F100 to 1F1E5 is enclosed
    // alphanumerics, 1F700 to 1F8FF is alchemical and geometric extensions, and
    // 1FA00 to 1FA6F is chess: none of them appear in emoji joiner sequences,
    // so accepting them would read a stray joiner between two such symbols as a
    // valid glyph.
    matches!(ch as u32,
        0x1F000..=0x1F0FF          // mahjong and playing cards
            | 0x1F1E6..=0x1F1FF    // regional indicators
            | 0x1F300..=0x1F6FF    // pictographs, faces, transport
            | 0x1F900..=0x1F9FF    // supplemental pictographs
            | 0x1FA70..=0x1FAFF    // extended-A pictographs
            | 0x2600..=0x27BF      // misc symbols and dingbats
            | 0x2B00..=0x2BFF      // stars and geometric shapes
            | 0x00A9 | 0x00AE | 0x2122 | 0x2139 | 0x24C2)
}

/// Viramas, the dead-consonant sign that Indic conjuncts place before a joiner.
///
/// Canonical combining class 9 is the Unicode definition, and
/// "unicode-normalization" is already linked for NFC, so this recognises every
/// virama rather than the ten a hand-written list happened to name. Which ones
/// reach it is decided by "joining_script": only U+0900 to U+0DFF, so the two
/// Malayalam forms qualify while Khmer coeng, Myanmar and Tibetan do not.
fn is_virama(ch: char) -> bool {
    unicode_normalization::char::canonical_combining_class(ch) == 9
}

/// Skin-tone modifiers. Valid directly after a base, never after a joiner:
/// "👩🏻‍🚀" is well formed and "👩‍🏻" is not, so a modifier on the right of a
/// joiner is residue.
fn is_emoji_modifier(ch: char) -> bool {
    matches!(ch as u32, 0x1F3FB..=0x1F3FF)
}

/// Script families that use a joiner orthographically. They use it
/// differently, and the difference decides what counts as residue.
#[derive(Clone, Copy, PartialEq, Eq)]
enum JoiningScript {
    /// Arabic, Persian and Urdu, where a joiner sits between two letters.
    Arabic,
    /// Devanagari through Sinhala, where it follows a virama. The payload is
    /// the block index, so two different Indic scripts do not match.
    Indic(u32),
}

fn joining_script(ch: char) -> Option<JoiningScript> {
    match ch as u32 {
        // Arabic Presentation Forms-B stops at FEFE: FEFF is the byte-order
        // mark, which another arm already owns.
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFE => {
            Some(JoiningScript::Arabic)
        }

        // One variant per 128-point block, so Devanagari and Bengali are not
        // treated as the same script by a joiner that must not cross scripts.
        cp @ 0x0900..=0x0DFF => Some(JoiningScript::Indic((cp - 0x0900) / 0x80)),
        _ => None,
    }
}

/// Whether a joiner sits inside an Indic conjunct: a virama on the left and a
/// letter of the same script on the right. Shared by both joiners, which use
/// the identical form, while the Arabic rule below applies only to the
/// non-joiner.
fn indic_conjunct(prev: Option<char>, next: Option<char>) -> bool {
    let (Some(prev), Some(next)) = (prev, next) else {
        return false;
    };
    matches!(
        (joining_script(prev), joining_script(next)),
        (Some(JoiningScript::Indic(a)), Some(JoiningScript::Indic(b))) if a == b
    ) && is_virama(prev)
        && next.is_alphabetic()
}

/// Whether a zero-width non-joiner between these two neighbours is spelling.
///
/// Both sides must belong to the same script family, or a joiner between an
/// Arabic letter and a Devanagari one would excuse itself. Arabic wants two
/// letters; Indic wants a virama on the left, since that is the conjunct form
/// and a joiner between two bare Indic letters is not.
fn zwnj_is_orthographic(prev: Option<char>, next: Option<char>) -> bool {
    let (Some(prev), Some(next)) = (prev, next) else {
        return false;
    };
    match (joining_script(prev), joining_script(next)) {
        (Some(JoiningScript::Arabic), Some(JoiningScript::Arabic)) => {
            prev.is_alphabetic() && next.is_alphabetic()
        }
        (Some(JoiningScript::Indic(_)), Some(JoiningScript::Indic(_))) => {
            indic_conjunct(Some(prev), Some(next))
        }
        _ => false,
    }
}

/// Whether the text holds any of the code points "is_suspicious_zero_width_at"
/// can report. Every other code point falls through its catch-all arm, so a
/// document without one cannot produce a hit and the caller can skip building
/// the "Vec<char>" that the neighbour tests need.
///
/// One decoding pass rather than a memchr per code point. With six candidates
/// the memchr form measured 1.13ms against 1.97ms over 1.9 MB, but the set is
/// now ranges rather than a short list, and keeping a separate prefilter set in
/// sync with the classifier is how the two come to disagree.
pub(crate) fn has_zero_width(text: &str) -> bool {
    text.chars().any(is_zero_width_candidate)
}

/// Whether "is_suspicious_zero_width_at" could possibly report this code point.
/// Every other one falls through its catch-all arm, so testing this first keeps
/// the neighbour analysis off the 99.99% of characters that are ordinary prose.
///
/// The private use areas are deliberately absent. Icon fonts put glyphs there,
/// and the technical documents this linter reads carry them legitimately.
pub(crate) fn is_zero_width_candidate(ch: char) -> bool {
    matches!(ch,
        '\u{00AD}'                    // soft hyphen
        | '\u{034F}'                  // combining grapheme joiner
        | '\u{180E}'                  // Mongolian vowel separator
        | '\u{200B}'..='\u{200F}'     // ZWSP, ZWNJ, ZWJ, LRM, RLM
        | '\u{202A}'..='\u{202E}'     // bidi embedding and override
        | '\u{2060}'..='\u{2064}'     // word joiner, invisible math operators
        | '\u{FDD0}'..='\u{FDEF}'     // noncharacters
        | '\u{FEFF}'                  // byte-order mark
        | '\u{FFF9}'..='\u{FFFB}'     // interlinear annotation
        | '\u{E0020}'..='\u{E007F}'   // tag characters
        | '\u{E0100}'..='\u{E01EF}'   // variation selectors supplement
    ) || u32::from(ch) & 0xFFFE == 0xFFFE // plane-final noncharacters
}

/// Human-readable name for a candidate, used in the issue context. Families
/// that span a range share one label; the code point number carries the rest.
fn zero_width_label(ch: char) -> &'static str {
    match ch {
        '\u{00AD}' => "soft hyphen",
        '\u{034F}' => "combining grapheme joiner",
        '\u{180E}' => "Mongolian vowel separator",
        '\u{200B}' => "zero-width space",
        '\u{200C}' => "zero-width non-joiner",
        '\u{200D}' => "zero-width joiner",
        '\u{200E}' => "left-to-right mark",
        '\u{200F}' => "right-to-left mark",
        '\u{202A}'..='\u{202E}' => "bidi embedding or override",
        '\u{2060}' => "word joiner",
        '\u{2061}'..='\u{2064}' => "invisible math operator",
        '\u{FEFF}' => "byte-order mark",
        '\u{FFF9}'..='\u{FFFB}' => "interlinear annotation",
        '\u{E0020}'..='\u{E007F}' => "tag character",
        '\u{E0100}'..='\u{E01EF}' => "variation selector",
        _ => "noncharacter",
    }
}

/// Render a candidate as "U+XXXX name" for the issue context.
pub(crate) fn describe_zero_width(ch: char) -> String {
    format!("U+{:04X} {}", u32::from(ch), zero_width_label(ch))
}

/// The tag payloads that spell a flag.
///
/// UTS #51 builds an emoji tag sequence from U+1F3F4, a tag_spec and the
/// U+E007F terminator, and every sequence in the recommended set is one of
/// these three. Matching the payload rather than merely bounding its length is
/// what closes the channel: a black flag, any six tag characters and a
/// terminator otherwise passed as a flag and carried six hidden characters.
///
/// A closed list dates, but the trade is the right way round. A subdivision
/// flag added to a future Unicode release is reported until this list grows,
/// which costs one line; the alternative leaves a smuggling channel open for
/// every document.
const FLAG_TAG_SPECS: [&str; 3] = ["gbeng", "gbsct", "gbwls"];

/// Longest payload in the list above. Both walks stop there, which is what
/// keeps them from being quadratic: without a bound, a run of 200k tag
/// characters took 59 seconds on 781 KB against 40 ms for 1.9 MB of ordinary
/// prose, because every character in the run rescanned the whole run.
///
/// Derived, so adding a longer spec above cannot leave the bound behind and
/// silently stop matching it.
const MAX_FLAG_TAG_SPEC: usize = {
    let (mut max, mut i) = (0, 0);
    while i < FLAG_TAG_SPECS.len() {
        if FLAG_TAG_SPECS[i].len() > max {
            max = FLAG_TAG_SPECS[i].len();
        }
        i += 1;
    }
    max
};

/// A regional-indicator flag written as a tag sequence opens with U+1F3F4 and
/// closes with U+E007F, and every tag character between the two belongs to it.
/// Both ends are checked: walking left to the base alone excuses an
/// unterminated run, which is exactly the shape hidden instructions take.
fn inside_flag_tag_sequence(chars: &[char], index: usize) -> bool {
    let is_spec = |c: char| ('\u{E0020}'..='\u{E007E}').contains(&c);

    // Find the base first, then judge the whole sequence from there. Testing
    // the payload from "index" forward instead would excuse the tail of an
    // over-long run, because from far enough in, what remains fits the bound.
    let back = chars[..index]
        .iter()
        .rev()
        .take(MAX_FLAG_TAG_SPEC)
        .take_while(|&&c| is_spec(c))
        .count();
    let Some(base) = index.checked_sub(back + 1) else {
        return false;
    };
    if chars[base] != '\u{1F3F4}' {
        return false;
    }
    let payload: String = chars[base + 1..]
        .iter()
        .take(MAX_FLAG_TAG_SPEC)
        .take_while(|&&c| is_spec(c))
        // Tag characters mirror ASCII 0x20 to 0x7E at U+E0020.
        .filter_map(|&c| char::from_u32(u32::from(c) - 0xE0000))
        .collect();
    FLAG_TAG_SPECS.contains(&payload.as_str())
        && chars.get(base + 1 + payload.chars().count()) == Some(&'\u{E007F}')
}

/// A text-style/emoji-style selector can sit between an emoji base and its
/// joiner (for example in "👩‍❤️‍👩"). It changes presentation, not the identity
/// of the preceding emoji, so skip it when validating a ZWJ's left side.
fn emoji_base_before(chars: &[char], index: usize) -> Option<char> {
    let mut i = index.checked_sub(1)?;
    if matches!(chars[i], '\u{FE0E}' | '\u{FE0F}') {
        i = i.checked_sub(1)?;
    }
    chars.get(i).copied()
}

/// Return whether "chars[index]" is suspicious invisible text rather than a
/// valid formatting sequence.  The reference Unicode guard distinguishes
/// encoding hygiene from style: a ZWJ joining two emoji and directional marks
/// in bidirectional text are legitimate, while a stray ZWJ or mid-text BOM is
/// useful evidence of copy/paste or tokenizer residue.
pub fn is_suspicious_zero_width_at(chars: &[char], index: usize) -> bool {
    let Some(&ch) = chars.get(index) else {
        return false;
    };
    let prev = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
    let next = chars.get(index + 1).copied();
    match ch {
        '\u{200B}' => true,

        // ZWNJ is orthography in Persian, Urdu and the Indic scripts, by the
        // same argument that exempts LRM/RLM below. Elsewhere it is residue.
        '\u{200C}' => !zwnj_is_orthographic(prev, next),
        '\u{FEFF}' => index != 0, // A file-start BOM is an encoding marker.
        '\u{200D}' => {
            let indic = indic_conjunct(prev, next);

            // A modifier belongs directly on a base, so it is never what a
            // joiner introduces, though it may be what precedes one.
            let emoji = !next.is_some_and(is_emoji_modifier)
                && emoji_base_before(chars, index).is_some_and(is_emoji_base)
                && next.is_some_and(is_emoji_base);
            !(indic || emoji)
        }

        // LRM/RLM are required for some mixed-direction text. Do not turn their
        // mere presence into an AI signal or a rewrite instruction.
        '\u{200E}' | '\u{200F}' => false,

        // A tag character is orthographic only inside a flag sequence. Loose
        // ones are the payload of the hidden-instruction trick.
        '\u{E0020}'..='\u{E007F}' => !inside_flag_tag_sequence(chars, index),

        // The supplement encodes ideographic variants, so it is orthographic on
        // a CJK base and residue anywhere else. The shared predicate also
        // admits bopomofo, which is not a valid variation base, so a selector
        // after one is excused. One predicate for "is this Han" beats a second
        // spelling of it that disagrees, and the miss needs a bopomofo letter
        // and a stray selector in the same document.
        '\u{E0100}'..='\u{E01EF}' => !prev.is_some_and(is_cjk_ideograph),

        // Soft hyphen, CGJ, word joiner, invisible operators, bidi overrides,
        // interlinear annotation and noncharacters have no role in zh-TW prose.
        _ => is_zero_width_candidate(ch),
    }
}

// Punctuation types tracked in the density matrix.
const PUNCT_MARKS: &[(char, &str)] = &[
    ('，', "comma"),
    ('。', "period"),
    ('；', "semicolon"),
    ('、', "dunhao"),
];

/// Per-type punctuation statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PunctuationStat {
    pub count: usize,
    /// Density per 1000 characters.
    pub density: f32,
    /// Coefficient of variation of inter-punctuation distances.
    /// None if count < 10 (insufficient data for stable CV).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cv: Option<f32>,
}

/// Punctuation density matrix across major zh-TW mark types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PunctuationProfile {
    pub comma: PunctuationStat,
    pub period: PunctuationStat,
    pub semicolon: PunctuationStat,
    pub dunhao: PunctuationStat,
    pub dash: PunctuationStat,
}

/// Compute CV (coefficient of variation) from inter-mark distances.
/// Returns None if fewer than 10 occurrences (fewer than 9 distances).
fn compute_cv(distances: &[usize]) -> Option<f32> {
    if distances.len() < 9 {
        return None;
    }
    let n = distances.len() as f64;
    let mean = distances.iter().map(|&d| d as f64).sum::<f64>() / n;
    if mean < 1.0 {
        return None;
    }
    let variance = distances
        .iter()
        .map(|&d| (d as f64 - mean).powi(2))
        .sum::<f64>()
        / n;
    Some((variance.sqrt() / mean) as f32)
}

/// Compute punctuation density matrix, respecting exclusion zones.
fn compute_punctuation_profile(
    text: &str,
    text_k: f32,
    excluded: &[ByteRange],
) -> PunctuationProfile {
    // Collect positions using a visible-char index that only advances outside
    // exclusion zones, so excluded content (code blocks, URLs) does not inflate
    // inter-punctuation distances.
    let mut positions: [Vec<usize>; 4] = [vec![], vec![], vec![], vec![]];
    let mut dash_positions: Vec<usize> = Vec::new();

    let mut byte_offset = 0;
    let mut visible_idx: usize = 0;
    let chars: Vec<char> = text.chars().collect();
    for (char_idx, &ch) in chars.iter().enumerate() {
        let ch_len = ch.len_utf8();
        if !is_excluded(byte_offset, byte_offset + ch_len, excluded) {
            for (slot, &(mark, _)) in PUNCT_MARKS.iter().enumerate() {
                if ch == mark {
                    positions[slot].push(visible_idx);
                }
            }
            // Em-dash: two consecutive '—' chars.
            if ch == '—' && char_idx + 1 < chars.len() && chars[char_idx + 1] == '—' {
                dash_positions.push(visible_idx);
            }
            visible_idx += 1;
        }
        byte_offset += ch_len;
    }

    // Build stats for each type.
    let build_stat = |pos: &[usize]| -> PunctuationStat {
        let count = pos.len();
        let density = count as f32 / text_k;
        let distances: Vec<usize> = pos.windows(2).map(|w| w[1].saturating_sub(w[0])).collect();
        let cv = compute_cv(&distances);
        PunctuationStat { count, density, cv }
    };

    PunctuationProfile {
        comma: build_stat(&positions[0]),
        period: build_stat(&positions[1]),
        semicolon: build_stat(&positions[2]),
        dunhao: build_stat(&positions[3]),
        dash: build_stat(&dash_positions),
    }
}

/// Compute sentence length variability (standard deviation of char counts).
///
/// Splits on terminal punctuation, filters out fragments < 4 chars,
/// requires >= 10 sentences for statistical significance.
/// Characters whose byte offsets fall in `excluded` ranges are skipped.
fn compute_sentence_variability(text: &str, excluded: &[ByteRange]) -> Option<f32> {
    let mut lengths: Vec<usize> = Vec::new();
    let mut current = 0usize;
    let mut byte_offset = 0usize;
    let mut was_excluded = false;
    for ch in text.chars() {
        let ch_len = ch.len_utf8();
        let in_excluded = is_excluded(byte_offset, byte_offset + ch_len, excluded);
        byte_offset += ch_len;
        if in_excluded {
            // Treat exclusion boundaries as sentence breaks so adjacent
            // sentences are not fused when a code block sits between them.
            if !was_excluded && current >= 4 {
                lengths.push(current);
                current = 0;
            }
            was_excluded = true;
            continue;
        }
        was_excluded = false;
        if SENTENCE_TERMINATORS.contains(&ch) {
            if current >= 4 {
                lengths.push(current);
            }
            current = 0;
        } else {
            current += 1;
        }
    }
    if lengths.len() < 10 {
        return None;
    }
    // Accumulate in f64 to avoid catastrophic cancellation on large documents.
    let n = lengths.len() as f64;
    let mean = lengths.iter().map(|&l| l as f64).sum::<f64>() / n;
    let variance = lengths
        .iter()
        .map(|&l| (l as f64 - mean).powi(2))
        .sum::<f64>()
        / n;
    Some(variance.sqrt() as f32)
}

/// Count zero-width tokenizer artifact codepoints in text (excluding exclusion
/// zones).
fn count_zero_width(text: &str, excluded: &[ByteRange]) -> usize {
    if !has_zero_width(text) {
        return 0;
    }
    let mut count = 0;
    let mut byte_offset = 0;
    let chars: Vec<char> = text.chars().collect();
    for (index, ch) in chars.iter().copied().enumerate() {
        let ch_len = ch.len_utf8();
        if is_zero_width_candidate(ch)
            && is_suspicious_zero_width_at(&chars, index)
            && !is_excluded(byte_offset, byte_offset + ch_len, excluded)
        {
            count += 1;
        }
        byte_offset += ch_len;
    }
    count
}

/// Signal 1: how far each tracked phrase runs above its density threshold.
///
/// Returns the weighted excess and the total weight considered, and pushes one
/// marker per phrase that occurs at all, whether or not it is over threshold:
/// the report lists what was measured, not only what scored.
fn phrase_density_signal(
    text: &str,
    excluded: &[ByteRange],
    mentions: &[ByteRange],
    text_k: f32,
    threshold_multiplier: f32,
    markers: &mut Vec<AiMarker>,
) -> (f32, f32) {
    let mut weighted_sum: f32 = 0.0;
    let mut total_weight: f32 = 0.0;

    // Apply threshold_multiplier so low/high sensitivity affects the composite
    // score, not just per-issue generation.
    for &(phrase, baseline, raw_threshold, weight) in DENSITY_SIGNALS {
        let threshold = raw_threshold * threshold_multiplier;
        let phrase_len = phrase.len();
        let mut count: usize = 0;
        let mut start = 0;
        while let Some(pos) = text[start..].find(phrase) {
            let abs = start + pos;
            start = abs + phrase_len;

            // "mentions" is separate from "excluded" on purpose. A quoted
            // phrase is not being used, so it must not score; but the
            // invisible-character signal counts through "excluded", and
            // widening that would let a hidden payload inside 「…」 go
            // uncounted, which is the channel this layer exists to close.
            if !is_excluded(abs, abs + phrase_len, excluded)
                && !is_excluded(abs, abs + phrase_len, mentions)
            {
                count += 1;
            }
        }
        if count == 0 {
            continue;
        }
        let density = count as f32 / text_k;
        markers.push(AiMarker {
            pattern: phrase.to_string(),
            count,
            density,
            threshold,
            expected_baseline: baseline,
        });
        if density > threshold {
            // Normalized contribution: how far above threshold, capped at 1.0.
            let excess = ((density - threshold) / threshold).min(2.0);
            weighted_sum += excess * weight;
        }
        total_weight += weight;
    }
    (weighted_sum, total_weight)
}

/// Signal 6: uniform punctuation rhythm, as an aggregate coefficient of
/// variation across the punctuation types with enough samples to mean
/// anything.  Low variation reads as machine cadence.  Caps at 0.1.
fn punctuation_contribution(profile: &PunctuationProfile, threshold_multiplier: f32) -> f32 {
    // Aggregate CV across types with sufficient samples (N >= 10), weighted by
    // occurrence count.
    let stats = [
        &profile.comma,
        &profile.period,
        &profile.semicolon,
        &profile.dunhao,
        &profile.dash,
    ];
    let mut weighted_cv_sum = 0.0f64;
    let mut total_count = 0usize;
    for stat in &stats {
        if let Some(cv) = stat.cv {
            weighted_cv_sum += cv as f64 * stat.count as f64;
            total_count += stat.count;
        }
    }
    if total_count == 0 {
        return 0.0;
    }
    let aggregate_cv = (weighted_cv_sum / total_count as f64) as f32;
    // Low CV (< threshold) = uniform rhythm = AI signal.  Max 0.1.
    let cv_threshold = 0.5 * threshold_multiplier;
    ((cv_threshold - aggregate_cv).max(0.0) / cv_threshold * 0.1).min(0.1)
}

/// Compute AI signature report from text and post-scan issues.
///
/// Combines five signal sources:
/// 1. Phrase density: count tracked phrases, compute density per 1000 chars.
/// 2. Structural patterns: count AiStyle issues from structural detectors.
/// 3. Per-occurrence: count non-structural/non-token AiStyle issues.
/// 4. Zero-width tokenizer artifacts: BPE/WordPiece residue.
/// 5. Punctuation density matrix: aggregate CV of inter-punctuation distances.
///
/// Sentence length variability is measured and serialized alongside these,
/// but contributes nothing to the score: see the note at its call site.
///
/// Returns None for texts too short to analyze (< 500 chars).
pub fn compute_ai_score(
    text: &str,
    issues: &[Issue],
    excluded: &[ByteRange],
    mentions: &[ByteRange],
    threshold_multiplier: f32,
) -> Option<AiSignatureReport> {
    // Guard against zero/negative multiplier to prevent div-by-zero in
    // thresholds.
    let threshold_multiplier = if threshold_multiplier <= 0.0 {
        1.0
    } else {
        threshold_multiplier
    };
    // Count only chars whose byte offsets fall outside excluded ranges.
    let char_count = {
        let mut count = 0usize;
        let mut byte_offset = 0usize;
        for ch in text.chars() {
            let ch_len = ch.len_utf8();
            if !is_excluded(byte_offset, byte_offset + ch_len, excluded) {
                count += 1;
            }
            byte_offset += ch_len;
        }
        count
    };
    if char_count < 500 {
        return None;
    }
    let text_k = char_count as f32 / 1000.0;

    let mut markers = Vec::new();

    // Signal 1: phrase density.
    let (weighted_sum, total_weight) = phrase_density_signal(
        text,
        excluded,
        mentions,
        text_k,
        threshold_multiplier,
        &mut markers,
    );

    // Signal 2: how many distinct structural families fired.
    //
    // Occurrences were counted before, so four hits of one detector saturated
    // the signal on their own. Over 278 human zh-TW documents that pinned 53%
    // at the cap; counting families drops it to 35%. Four hits of one rule is
    // one signal, which is what both reference skills state: an isolated device
    // is nothing, a cluster of different devices is the confession.
    //
    // Formatting is capped apart from prose. Bold runs, list shape and heading
    // form describe the Markdown rather than the writing, and they produced
    // 1,567 of the 2,529 structural hits in that corpus, so pooling them let
    // layout outvote the prose it was meant to weigh.
    let families: std::collections::BTreeSet<StructuralFamily> = issues
        .iter()
        .filter_map(|i| i.structural_family)
        .filter(|f| f.is_authorship_evidence())
        .collect();
    let formatting = families.iter().filter(|f| f.is_formatting()).count();
    let prose = families.len() - formatting;
    let structural_contribution =
        (prose as f32 * 0.1).min(0.3) + (formatting as f32 * 0.05).min(0.15);

    // Signal 3: non-structural, non-zero-width AiStyle issue density. Excludes
    // issues already counted by signals 1 (density phrases), 2 (structural),
    // and 5 (zero-width) to avoid double-counting.
    let ai_issue_count = issues
        .iter()
        .filter(|i| {
            i.rule_type == IssueType::AiStyle

                // Signal 2 owns anything carrying a structural family; the
                // invisible-character layer still identifies itself by prefix.
                && i.structural_family.is_none()
                && !i
                    .context
                    .as_ref()
                    .is_some_and(|c| c.starts_with("AI token:"))
                && !DENSITY_SIGNALS
                    .iter()
                    .any(|&(phrase, _, _, _)| i.found == phrase)
        })
        .count();
    let ai_issue_density = ai_issue_count as f32 / text_k;

    // High density of AI issues is itself a signal. Threshold: >2 per 1000
    // chars.
    let issue_density_contribution = if ai_issue_density > 2.0 {
        ((ai_issue_density - 2.0) / 5.0).min(0.3)
    } else {
        0.0
    };

    // Sentence length variability: measured and reported, but it contributes
    // nothing to the score.
    //
    // The threshold was 5.0 characters of standard deviation, and nothing
    // reaches it. All 158 corpus cases return None, because none has the ten
    // sentences the measurement needs, and across 405 real zh-TW documents the
    // minimum is 13.4, including deliberately machine-written ones. A term that
    // has never fired is not a signal, and scoring against an unfalsifiable
    // threshold is worse than not scoring at all.
    //
    // Kept as an observation because it is real, already serialized, and the
    // number a reader can act on. Reviving it as a signal needs a coefficient
    // of variation rather than a raw sigma, since sigma is scale-dependent, and
    // a long-form corpus to calibrate against. We have neither.
    let sentence_variability = compute_sentence_variability(text, excluded);

    // Signal 5: zero-width tokenizer artifacts.
    let zero_width_count = count_zero_width(text, excluded);
    let zw_scale = 3.0 * threshold_multiplier;
    let zero_width_contribution = if zero_width_count > 0 {
        // Any presence is suspicious; 3+ is strong signal. Max 0.2.
        ((zero_width_count as f32) / zw_scale * 0.2).min(0.2)
    } else {
        0.0
    };

    // Signal 6: punctuation density matrix, aggregate CV.
    let punctuation_profile = compute_punctuation_profile(text, text_k, excluded);
    let punct_contribution = punctuation_contribution(&punctuation_profile, threshold_multiplier);

    // Composite score: combine all five scoring signals (rebalanced per 40.11).
    // phrase ≤0.7, structural ≤0.45 (prose ≤0.3 plus formatting ≤0.15, capped
    // apart), issue ≤0.3, zero-width ≤0.2, punctuation ≤0.1. Max 1.75 before
    // clamp. No single signal exceeds 0.7; ≥0.8 requires ≥2 dimensions.
    let phrase_score = if total_weight > 0.0 {
        (weighted_sum / total_weight).min(1.0) * 0.7
    } else {
        0.0
    };
    let raw_score = phrase_score
        + structural_contribution
        + issue_density_contribution
        + zero_width_contribution
        + punct_contribution;
    let score = raw_score.min(1.0);

    // Build top signals list (sorted by density excess ratio).
    markers.sort_by(|a, b| {
        let a_ratio = a.density / a.threshold;
        let b_ratio = b.density / b.threshold;
        b_ratio
            .partial_cmp(&a_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut top_signals: Vec<String> = markers
        .iter()
        .filter(|m| m.density > m.threshold)
        .take(3)
        .map(|m| {
            format!(
                "\u{300C}{}\u{300D} {:.1}次/千字 (閾值 {})",
                m.pattern, m.density, m.threshold
            )
        })
        .collect();
    let structural_families = families.len();
    if structural_families > 0 {
        top_signals.push(format!("{structural_families} 類結構性 AI 特徵"));
    }
    if zero_width_count > 0 {
        top_signals.push(format!(
            "{zero_width_count} 個隱形字元（疑似分詞器或複製貼上殘留）"
        ));
    }
    if punct_contribution > 0.0 {
        top_signals.push("標點節奏過於均勻（疑似 AI 生成）".to_string());
    }
    top_signals.truncate(3);

    let punctuation_profile =
        if punctuation_profile.comma.cv.is_some() || punctuation_profile.period.cv.is_some() {
            Some(punctuation_profile)
        } else {
            None
        };

    Some(AiSignatureReport {
        score,
        markers,
        top_signals,
        sentence_variability,
        zero_width_count,
        punctuation_profile,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/engine/ai_score/tests.rs"]
mod tests;
