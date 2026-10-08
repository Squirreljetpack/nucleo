#![allow(missing_docs)]

use std::cmp::Ordering;

pub mod r#const;
pub mod matcher;
pub mod prefilter;
pub mod simd;
pub mod smith_waterman;

pub use matcher::Matcher;
use r#const::*;

pub fn match_list<S1: AsRef<str>, S2: AsRef<str>>(
    needle: S1,
    haystacks: &[S2],
    config: &Config,
) -> Vec<Match> {
    Matcher::new(needle.as_ref(), config).match_list(haystacks)
}

pub fn match_list_indices<S1: AsRef<str>, S2: AsRef<str>>(
    needle: S1,
    haystacks: &[S2],
    config: &Config,
) -> Vec<MatchIndices> {
    Matcher::new(needle.as_ref(), config).match_list_indices(haystacks)
}

#[derive(Debug, Clone)]
pub struct Match {
    pub score: u16,
    /// Index of the match in the original list of haystacks
    pub index: u32,
    /// Matched the needle exactly (e.g. "foo" on "foo")
    pub exact: bool,
}

impl Match {
    pub fn from_index(index: usize) -> Self {
        Self {
            score: 0,
            index: index as u32,
            exact: false,
        }
    }
}

impl PartialOrd for Match {
    fn partial_cmp(&self, other: &Match) -> Option<Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}
impl Ord for Match {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.score as u64)
            .cmp(&(other.score as u64))
            .reverse()
            .then_with(|| self.index.cmp(&other.index))
    }
}
impl PartialEq for Match {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.index == other.index
    }
}
impl Eq for Match {}

#[derive(Debug, Clone)]
pub struct MatchIndices {
    pub score: u16,
    /// Index of the match in the original list of haystacks
    pub index: u32,
    /// Matched the needle exactly (e.g. "foo" on "foo")
    pub exact: bool,
    /// Indices of the chars in the haystack that matched the needle in reverse order
    pub indices: Vec<usize>,
}

impl MatchIndices {
    pub fn from_index(index: usize) -> Self {
        Self {
            score: 0,
            index: index as u32,
            exact: false,
            indices: vec![],
        }
    }
}

impl PartialOrd for MatchIndices {
    fn partial_cmp(&self, other: &MatchIndices) -> Option<Ordering> {
        Some(std::cmp::Ord::cmp(self, other))
    }
}
impl Ord for MatchIndices {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.score as u64)
            .cmp(&(other.score as u64))
            .reverse()
            .then_with(|| self.index.cmp(&other.index))
    }
}
impl PartialEq for MatchIndices {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.index == other.index
    }
}
impl Eq for MatchIndices {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The maximum number of characters missing from the needle, before an item in the
    /// haystack is filtered out
    pub max_typos: Option<u16>,
    /// Controls the scoring used by the smith waterman algorithm.
    pub scoring: Scoring,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_typos: Some(0),
            scoring: Scoring::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scoring {
    /// Score for a matching character between needle and haystack
    pub match_score: u16,
    /// Penalty for a mismatch (substitution)
    pub mismatch_penalty: u16,
    /// Penalty for opening a gap (deletion/insertion)
    pub gap_open_penalty: u16,
    /// Penalty for extending a gap (deletion/insertion)
    pub gap_extend_penalty: u16,

    /// Bonus for matching the first character of the haystack
    pub prefix_bonus: u16,
    /// Bonus for matching a capital letter after a lowercase letter
    pub capitalization_bonus: u16,
    /// Bonus for matching the case of the needle
    pub matching_case_bonus: u16,
    /// Bonus for matching the exact needle
    pub exact_match_bonus: u16,
    /// Bonus for matching after a delimiter character
    pub delimiter_bonus: u16,
}

impl Default for Scoring {
    fn default() -> Self {
        Scoring {
            match_score: MATCH_SCORE,
            mismatch_penalty: MISMATCH_PENALTY,
            gap_open_penalty: GAP_OPEN_PENALTY,
            gap_extend_penalty: GAP_EXTEND_PENALTY,

            prefix_bonus: PREFIX_BONUS,
            capitalization_bonus: CAPITALIZATION_BONUS,
            matching_case_bonus: MATCHING_CASE_BONUS,
            exact_match_bonus: EXACT_MATCH_BONUS,
            delimiter_bonus: DELIMITER_BONUS,
        }
    }
}

use crate::matcher::utf32_str::Utf32Str;
fn utf32_eq_ignore_case(a: Utf32Str<'_>, b: Utf32Str<'_>) -> bool {
    if a.len() != b.len() {
        return false;
    }
    match (a, b) {
        (Utf32Str::Ascii(a), Utf32Str::Ascii(b)) => a.eq_ignore_ascii_case(b),
        _ => a
            .chars()
            .map(crate::matcher::chars::to_lower_case)
            .eq(b.chars().map(crate::matcher::chars::to_lower_case)),
    }
}

fn utf32_contains_ignore_case(haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    match (haystack, needle) {
        (Utf32Str::Ascii(h), Utf32Str::Ascii(n)) => {
            h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
        }
        (Utf32Str::Unicode(h), Utf32Str::Unicode(n)) => h.windows(n.len()).any(|w| {
            w.iter()
                .zip(n.iter())
                .all(|(&hc, &nc)| crate::matcher::chars::to_lower_case(hc) == crate::matcher::chars::to_lower_case(nc))
        }),
        _ => {
            let n_len = needle.len();
            for i in 0..=haystack.len() - n_len {
                if haystack
                    .slice(i..i + n_len)
                    .chars()
                    .zip(needle.chars())
                    .all(|(hc, nc)| crate::matcher::chars::to_lower_case(hc) == crate::matcher::chars::to_lower_case(nc))
                {
                    return true;
                }
            }
            false
        }
    }
}

/// Frizbee SIMD Smith-Waterman matcher.
pub struct FrizbeeMatcher {
    #[allow(missing_docs)]
    pub config: Config,
    matcher: matcher::Matcher,
    haystack_bytes_buf: Vec<u8>,
    needle_bytes_buf: Vec<u8>,
}

impl Clone for FrizbeeMatcher {
    fn clone(&self) -> Self {
        FrizbeeMatcher {
            config: self.config.clone(),
            matcher: matcher::Matcher::new(&self.matcher.needle, &self.config),
            haystack_bytes_buf: Vec::new(),
            needle_bytes_buf: Vec::new(),
        }
    }
}

impl std::fmt::Debug for FrizbeeMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrizbeeMatcher")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Default for FrizbeeMatcher {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl FrizbeeMatcher {
    /// Creates a new Frizbee matcher instance.
    pub fn new(config: Config) -> Self {
        let matcher = matcher::Matcher::new("", &config);
        Self {
            config,
            matcher,
            haystack_bytes_buf: Vec::new(),
            needle_bytes_buf: Vec::new(),
        }
    }

    /// Sets the configuration for the Frizbee matching engine.
    pub fn set_config(&mut self, config: &Config) {
        self.config = config.clone();
        self.matcher.set_config(config);
    }

    fn ensure_needle(&mut self, needle: Utf32Str<'_>) {
        match needle {
            Utf32Str::Ascii(bytes) => {
                if self.matcher.needle.as_bytes() != bytes {
                    if let Ok(s) = std::str::from_utf8(bytes) {
                        self.matcher.set_needle(s);
                    }
                }
            }
            Utf32Str::Unicode(chars) => {
                if !self.matcher.needle.chars().eq(chars.iter().copied()) {
                    self.needle_bytes_buf.clear();
                    for &c in chars {
                        let mut b = [0u8; 4];
                        self.needle_bytes_buf.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                    }
                    if let Ok(s) = std::str::from_utf8(&self.needle_bytes_buf) {
                        self.matcher.set_needle(s);
                    }
                }
            }
        }
    }

    /// Find the fuzzy match with the highest score in the `haystack`.
    pub fn fuzzy_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        if haystack.is_empty() {
            return None;
        }
        self.ensure_needle(needle);
        let haystack_bytes = match haystack {
            Utf32Str::Ascii(bytes) => bytes,
            Utf32Str::Unicode(chars) => {
                self.haystack_bytes_buf.clear();
                for &c in chars {
                    let mut b = [0u8; 4];
                    self.haystack_bytes_buf.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                }
                &self.haystack_bytes_buf
            }
        };

        if haystack_bytes.is_empty() {
            return None;
        }

        let skipped_chunks = if let Some(max_typos) = self.config.max_typos {
            let min_len = self.matcher.needle.len().saturating_sub(max_typos as usize);
            if haystack_bytes.len() < min_len {
                return None;
            }
            let (matched, skipped) = self.matcher.prefilter.match_haystack(haystack_bytes, max_typos);
            if !matched {
                return None;
            }
            skipped
        } else {
            0
        };

        let chunk_offset = skipped_chunks * 16;
        if chunk_offset >= haystack_bytes.len() {
            return None;
        }
        let haystack_bytes = &haystack_bytes[chunk_offset..];

        let m = self.matcher.smith_waterman_one(haystack_bytes, 0, skipped_chunks == 0)?;
        Some(m.score)
    }

    /// Find the fuzzy match with the highest score in the `haystack` and compute its indices.
    pub fn fuzzy_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        if haystack.is_empty() {
            return None;
        }
        self.ensure_needle(needle);
        match haystack {
            Utf32Str::Ascii(bytes) => {
                if bytes.is_empty() {
                    return None;
                }
                let skipped_chunks = if let Some(max_typos) = self.config.max_typos {
                    let min_len = self.matcher.needle.len().saturating_sub(max_typos as usize);
                    if bytes.len() < min_len {
                        return None;
                    }
                    let (matched, skipped) = self.matcher.prefilter.match_haystack(bytes, max_typos);
                    if !matched {
                        return None;
                    }
                    skipped
                } else {
                    0
                };
                let chunk_offset = skipped_chunks * 16;
                if chunk_offset >= bytes.len() {
                    return None;
                }
                let slice = &bytes[chunk_offset..];
                let m = self.matcher.smith_waterman_indices_one(slice, skipped_chunks, 0, skipped_chunks == 0)?;
                indices.extend(m.indices.into_iter().rev().map(|i| i as u32));
                Some(m.score)
            }
            Utf32Str::Unicode(chars) => {
                if chars.is_empty() {
                    return None;
                }
                self.haystack_bytes_buf.clear();
                for &c in chars {
                    let mut b = [0u8; 4];
                    self.haystack_bytes_buf.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                }
                let bytes = &self.haystack_bytes_buf;
                if bytes.is_empty() {
                    return None;
                }
                let skipped_chunks = if let Some(max_typos) = self.config.max_typos {
                    let min_len = self.matcher.needle.len().saturating_sub(max_typos as usize);
                    if bytes.len() < min_len {
                        return None;
                    }
                    let (matched, skipped) = self.matcher.prefilter.match_haystack(bytes, max_typos);
                    if !matched {
                        return None;
                    }
                    skipped
                } else {
                    0
                };
                let chunk_offset = skipped_chunks * 16;
                if chunk_offset >= bytes.len() {
                    return None;
                }
                let slice = &bytes[chunk_offset..];
                let m = self.matcher.smith_waterman_indices_one(slice, skipped_chunks, 0, skipped_chunks == 0)?;
                let mut byte_pos = 0;
                let mut char_map = Vec::with_capacity(chars.len());
                for (c_idx, &c) in chars.iter().enumerate() {
                    let next = byte_pos + c.len_utf8();
                    char_map.push((byte_pos..next, c_idx as u32));
                    byte_pos = next;
                }
                for &byte_idx in m.indices.iter().rev() {
                    for (range, c_idx) in &char_map {
                        if range.contains(&byte_idx) {
                            if indices.last() != Some(c_idx) {
                                indices.push(*c_idx);
                            }
                            break;
                        }
                    }
                }
                Some(m.score)
            }
        }
    }

    /// Greedily find a fuzzy match in the `haystack`.
    pub fn fuzzy_match_greedy(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        self.fuzzy_match(haystack, needle)
    }

    /// Greedily find a fuzzy match in the `haystack` and compute its indices.
    pub fn fuzzy_indices_greedy(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        self.fuzzy_indices(haystack, needle, indices)
    }

    /// Checks whether needle and haystack match exactly.
    pub fn exact_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut leading_space = 0;
        let mut trailing_space = 0;
        if !needle.first().is_whitespace() {
            leading_space = haystack.leading_white_space();
        }
        if !needle.last().is_whitespace() {
            trailing_space = haystack.trailing_white_space();
        }
        if trailing_space == haystack.len() {
            return None;
        }
        let trimmed = haystack.slice(leading_space..haystack.len() - trailing_space);
        if utf32_eq_ignore_case(trimmed, needle) {
            self.fuzzy_match(haystack, needle)
        } else {
            None
        }
    }

    /// Checks whether needle and haystack match exactly and compute match indices.
    pub fn exact_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut leading_space = 0;
        let mut trailing_space = 0;
        if !needle.first().is_whitespace() {
            leading_space = haystack.leading_white_space();
        }
        if !needle.last().is_whitespace() {
            trailing_space = haystack.trailing_white_space();
        }
        if trailing_space == haystack.len() {
            return None;
        }
        let trimmed = haystack.slice(leading_space..haystack.len() - trailing_space);
        if utf32_eq_ignore_case(trimmed, needle) {
            self.fuzzy_indices(haystack, needle, indices)
        } else {
            None
        }
    }

    /// Checks whether needle is a prefix of the haystack.
    pub fn prefix_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut leading_space = 0;
        if !needle.first().is_whitespace() {
            leading_space = haystack.leading_white_space();
        }
        if haystack.len() - leading_space < needle.len() {
            return None;
        }
        let prefix = haystack.slice(leading_space..needle.len() + leading_space);
        if utf32_eq_ignore_case(prefix, needle) {
            self.fuzzy_match(haystack, needle)
        } else {
            None
        }
    }

    /// Checks whether needle is a prefix of the haystack and compute match indices.
    pub fn prefix_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut leading_space = 0;
        if !needle.first().is_whitespace() {
            leading_space = haystack.leading_white_space();
        }
        if haystack.len() - leading_space < needle.len() {
            return None;
        }
        let prefix = haystack.slice(leading_space..needle.len() + leading_space);
        if utf32_eq_ignore_case(prefix, needle) {
            self.fuzzy_indices(haystack, needle, indices)
        } else {
            None
        }
    }

    /// Checks whether needle is a postfix of the haystack.
    pub fn postfix_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut trailing_space = 0;
        if !needle.last().is_whitespace() {
            trailing_space = haystack.trailing_white_space();
        }
        if haystack.len() - trailing_space < needle.len() {
            return None;
        }
        let postfix = haystack.slice(haystack.len() - needle.len() - trailing_space..haystack.len() - trailing_space);
        if utf32_eq_ignore_case(postfix, needle) {
            self.fuzzy_match(haystack, needle)
        } else {
            None
        }
    }

    /// Checks whether needle is a postfix of the haystack and compute match indices.
    pub fn postfix_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut trailing_space = 0;
        if !needle.last().is_whitespace() {
            trailing_space = haystack.trailing_white_space();
        }
        if haystack.len() - trailing_space < needle.len() {
            return None;
        }
        let postfix = haystack.slice(haystack.len() - needle.len() - trailing_space..haystack.len() - trailing_space);
        if utf32_eq_ignore_case(postfix, needle) {
            self.fuzzy_indices(haystack, needle, indices)
        } else {
            None
        }
    }

    /// Finds the substring match with the highest score in the `haystack`.
    pub fn substring_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        if utf32_contains_ignore_case(haystack, needle) {
            self.fuzzy_match(haystack, needle)
        } else {
            None
        }
    }

    /// Finds the substring match with the highest score in the `haystack` and compute its indices.
    pub fn substring_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        if utf32_contains_ignore_case(haystack, needle) {
            self.fuzzy_indices(haystack, needle, indices)
        } else {
            None
        }
    }
}

