//! Finding text in a document.
//!
//! PDFium's own search (`FPDFText_FindStart`) compares characters exactly, which
//! is a poor fit for Arabic: a word typed without diacritics wouldn't find the
//! same word written with them, and "اسم" wouldn't find "إسم". So the search is
//! done here, on the page's characters in PDFium's text order:
//!
//! - both the page and the query are *folded* (see [`fold`]): diacritics and
//!   tatweel dropped, letter variants unified, digits of every script made
//!   Western, presentation forms and ligatures decomposed, case ignored;
//! - every folded character remembers which page character it came from, so a
//!   match maps back to a range of characters the UI can highlight and select.
//!
//! A document is searched in batches of pages, so renders aren't held up while
//! a long document is searched (the UI asks for the next batch).

use std::time::{Duration, Instant};

use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unicode_normalization::char::decompose_compatible;

use crate::text::TextPage;

/// What to look for.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    /// Upper and lower case must match (otherwise "pdf" finds "PDF").
    pub match_case: bool,
    /// Only whole words ("cat" doesn't find "category").
    pub whole_word: bool,
}

/// One place the text was found.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct SearchHit {
    pub page: u32,
    /// The matching characters: `start..end` (character positions, as in a selection).
    pub start: u32,
    pub end: u32,
    /// Some text around the match, for the list of results.
    pub before: String,
    pub text: String,
    pub after: String,
}

/// The results of searching some pages.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct SearchBatch {
    pub hits: Vec<SearchHit>,
    /// Where to continue, or `None` when the last page has been searched.
    pub next_page: Option<u32>,
}

/// A batch stops after this many pages or this long, whichever comes first.
pub(crate) const BATCH_PAGES: u32 = 40;
pub(crate) const BATCH_TIME: Duration = Duration::from_millis(60);

/// Characters of context shown on each side of a match.
const CONTEXT: usize = 32;

/// Text folded for matching, with each folded character's source position.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Folded {
    pub chars: Vec<char>,
    pub sources: Vec<u32>,
}

/// Folds text for matching. Returns the folded characters and, for each one,
/// the index of the input character it came from.
pub(crate) fn fold(input: impl IntoIterator<Item = char>, match_case: bool) -> Folded {
    let mut out = Folded::default();
    for (index, c) in input.into_iter().enumerate() {
        let mut push = |c: char| {
            let c = if c.is_whitespace() { ' ' } else { c };
            // Line breaks and runs of spaces all count as one space, so a phrase
            // still matches where the PDF wraps it onto the next line.
            if c == ' ' && out.chars.last().is_none_or(|&last| last == ' ') {
                return;
            }
            out.chars.push(c);
            out.sources.push(index as u32);
        };
        // Compatibility decomposition: presentation forms (ﻣ → م), ligatures
        // (ﻻ → لا, ﬁ → fi), full-width letters, and accents split off letters.
        decompose_compatible(c, |d| {
            let Some(d) = fold_char(d) else { return };
            if match_case {
                push(d);
            } else {
                d.to_lowercase().for_each(&mut push);
            }
        });
    }
    if out.chars.last() == Some(&' ') {
        out.chars.pop();
        out.sources.pop();
    }
    out
}

/// One character after decomposition: `None` drops it, otherwise its folded form.
fn fold_char(c: char) -> Option<char> {
    Some(match c {
        // Arabic diacritics (harakat, tanween, shadda, sukun, Quranic marks), the
        // superscript alef, and tatweel (the stretching line).
        '\u{0610}'..='\u{061A}'
        | '\u{064B}'..='\u{065F}'
        | '\u{0670}'
        | '\u{06D6}'..='\u{06ED}'
        | '\u{0640}' => return None,
        // Alef with hamza or madda, and alef wasla → bare alef.
        'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
        // Alef maksura and Persian yeh → yeh; Persian keheh → kaf.
        'ى' | 'ی' => 'ي',
        'ک' => 'ك',
        // Arabic-Indic and Persian digits → 0–9.
        '\u{0660}'..='\u{0669}' => char::from(b'0' + (c as u32 - 0x0660) as u8),
        '\u{06F0}'..='\u{06F9}' => char::from(b'0' + (c as u32 - 0x06F0) as u8),
        // Accents split off Latin letters (é → e + ◌́): ignored, so "cafe" finds "café".
        '\u{0300}'..='\u{036F}' => return None,
        // Characters PDFium uses internally (e.g. a soft hyphen marker).
        c if c.is_control() && !c.is_whitespace() => return None,
        c => c,
    })
}

/// Searches pages from `first`, until the batch's page or time limit.
/// `chars_of` gives a page's characters (see [`page_chars`]).
pub(crate) fn search_pages(
    page_count: u32,
    first: u32,
    query: &SearchQuery,
    mut chars_of: impl FnMut(u32) -> Vec<char>,
) -> SearchBatch {
    let needle = fold(query.text.chars(), query.match_case).chars;
    if needle.is_empty() {
        return SearchBatch {
            hits: Vec::new(),
            next_page: None,
        };
    }
    let started = Instant::now();
    let mut hits = Vec::new();
    let mut page = first;
    while page < page_count {
        hits.extend(find_in_page(page, &chars_of(page), &needle, query));
        page += 1;
        if page - first >= BATCH_PAGES || started.elapsed() >= BATCH_TIME {
            break;
        }
    }
    SearchBatch {
        hits,
        next_page: (page < page_count).then_some(page),
    }
}

/// The characters of a page in PDFium's order (one per character position).
pub(crate) fn page_chars(pdfium: &Pdfium, page: &PdfPage) -> Vec<char> {
    let Some(text) = TextPage::load(pdfium, page) else {
        return Vec::new();
    };
    crate::text::raw_chars(&text)
        .into_iter()
        .map(|c| char::from_u32(c.code).unwrap_or('\u{FFFD}'))
        .collect()
}

/// Every match of the folded `needle` in a page's characters.
pub(crate) fn find_in_page(
    page: u32,
    chars: &[char],
    needle: &[char],
    query: &SearchQuery,
) -> Vec<SearchHit> {
    if needle.is_empty() {
        return Vec::new();
    }
    let hay = fold(chars.iter().copied(), query.match_case);
    let mut hits = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.chars.len() {
        if hay.chars[i..i + needle.len()] != *needle
            || (query.whole_word && !is_whole_word(&hay.chars, i, i + needle.len()))
        {
            i += 1;
            continue;
        }
        let start = hay.sources[i] as usize;
        let mut end = hay.sources[i + needle.len() - 1] as usize + 1;
        // Include diacritics written on the last letter.
        while end < chars.len() && is_ignored(chars[end]) {
            end += 1;
        }
        hits.push(SearchHit {
            page,
            start: start as u32,
            end: end as u32,
            before: snippet(&chars[start.saturating_sub(CONTEXT)..start], true),
            text: snippet(&chars[start..end], false),
            after: snippet(&chars[end..(end + CONTEXT).min(chars.len())], false),
        });
        i += needle.len();
    }
    hits
}

/// Whether matching ignores this character entirely (a diacritic, tatweel…).
fn is_ignored(c: char) -> bool {
    let mut kept = false;
    decompose_compatible(c, |d| kept |= fold_char(d).is_some());
    !kept && !c.is_whitespace()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_whole_word(hay: &[char], start: usize, end: usize) -> bool {
    let before = start == 0 || !is_word_char(hay[start - 1]);
    let after = end == hay.len() || !is_word_char(hay[end]);
    before && after
}

/// Context text on one line, cut at whole words.
fn snippet(chars: &[char], before: bool) -> String {
    let text: String = chars
        .iter()
        .map(|&c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let keep_space = |s: &str, side_space: bool| {
        if side_space {
            format!("{s} ")
        } else {
            s.to_owned()
        }
    };
    if before {
        // Keep the space before the match, and drop a partial first word.
        let ends_with_space = chars.last().is_some_and(|c| c.is_whitespace());
        let trimmed = match text.split_once(' ') {
            Some((_, rest)) if chars.len() >= CONTEXT => rest,
            _ => &text,
        };
        keep_space(trimmed, ends_with_space && !trimmed.is_empty())
    } else {
        // Drop a partial last word when the context was cut short.
        let text = match text.rsplit_once(' ') {
            Some((kept, _)) if chars.len() >= CONTEXT => kept.to_owned(),
            _ => text,
        };
        let starts_with_space = chars.first().is_some_and(|c| c.is_whitespace());
        if starts_with_space && !text.is_empty() {
            format!(" {text}")
        } else {
            text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folded(s: &str) -> String {
        fold(s.chars(), false).chars.into_iter().collect()
    }

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            match_case: false,
            whole_word: false,
        }
    }

    fn find(page: &str, q: &SearchQuery) -> Vec<(u32, u32)> {
        let chars: Vec<char> = page.chars().collect();
        let needle = fold(q.text.chars(), q.match_case).chars;
        find_in_page(0, &chars, &needle, q)
            .into_iter()
            .map(|h| (h.start, h.end))
            .collect()
    }

    #[test]
    fn folds_arabic() {
        // Diacritics and tatweel are dropped.
        assert_eq!(folded("كَتَبَ"), "كتب");
        assert_eq!(folded("مـــحمد"), "محمد");
        // Hamza and madda forms of alef, alef maksura, Persian letters.
        assert_eq!(folded("أإآٱ"), "اااا");
        assert_eq!(folded("على"), "علي");
        assert_eq!(folded("کی"), "كي");
        // Presentation forms (old PDFs) and the lam-alef ligature.
        assert_eq!(folded("ﻣﺮﺣﺒﺎ"), "مرحبا");
        assert_eq!(folded("ﻻ"), "لا");
    }

    #[test]
    fn folds_digits_case_spaces_and_accents() {
        assert_eq!(folded("١٢٣ ۴۵۶"), "123 456");
        assert_eq!(folded("PDF Rivet"), "pdf rivet");
        assert_eq!(folded("one\r\n  two"), "one two");
        assert_eq!(folded("Café"), "cafe");
        assert_eq!(folded("ﬁle"), "file");
        assert_eq!(fold("PDF".chars(), true).chars, vec!['P', 'D', 'F']);
    }

    #[test]
    fn maps_matches_back_to_page_characters() {
        // "ب" with a shadda and a fatha: the match covers all three characters.
        assert_eq!(find("ا بَّ ت", &query("ب")), vec![(2, 5)]);
        // A phrase wrapped onto the next line.
        assert_eq!(find("the quick\r\nfox", &query("quick fox")), vec![(4, 14)]);
        // Diacritics in the query are ignored too.
        assert_eq!(find("كتب الدرس", &query("كَتَبَ")), vec![(0, 3)]);
        assert_eq!(find("رقم ١٢٣", &query("123")), vec![(4, 7)]);
    }

    #[test]
    fn match_case_and_whole_words() {
        let page = "PDF pdf category cat";
        let mut q = query("pdf");
        assert_eq!(find(page, &q).len(), 2);
        q.match_case = true;
        assert_eq!(find(page, &q), vec![(4, 7)]);
        let mut q = query("cat");
        assert_eq!(find(page, &q).len(), 2);
        q.whole_word = true;
        assert_eq!(find(page, &q), vec![(17, 20)]);
    }

    #[test]
    fn snippets_show_context_on_one_line() {
        let chars: Vec<char> = "first line\r\nThe needle is here".chars().collect();
        let needle = fold("needle".chars(), false).chars;
        let hit = &find_in_page(3, &chars, &needle, &query("needle"))[0];
        assert_eq!(hit.page, 3);
        assert_eq!(hit.before, "first line The ");
        assert_eq!(hit.text, "needle");
        assert_eq!(hit.after, " is here");
    }

    #[test]
    fn empty_query_finds_nothing() {
        assert!(find("text", &query("  ")).is_empty());
    }
}
