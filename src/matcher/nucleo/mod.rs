pub mod config;
pub mod debug;
pub mod exact;
pub mod greedy;
pub mod matrix;
pub mod optimal;
pub mod prefilter;
pub mod score;

pub use config::Config;
pub(crate) use matrix::MatrixSlab;

use crate::matcher::chars::{AsciiChar, Char};
use crate::matcher::utf32_str::Utf32Str;

/// Nucleo's default banded Smith-Waterman matrix matcher.
pub struct NucleoMatcher {
    #[allow(missing_docs)]
    pub config: Config,
    pub(crate) slab: MatrixSlab,
}

impl Clone for NucleoMatcher {
    fn clone(&self) -> Self {
        NucleoMatcher {
            config: self.config.clone(),
            slab: MatrixSlab::new(),
        }
    }
}

impl std::fmt::Debug for NucleoMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NucleoMatcher")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Default for NucleoMatcher {
    fn default() -> Self {
        NucleoMatcher {
            config: Config::DEFAULT,
            slab: MatrixSlab::new(),
        }
    }
}

impl NucleoMatcher {
    /// Creates a new Nucleo matcher instance, note that this will eagerly allocate a
    /// fairly large chunk of heap memory (around 135KB currently but subject to
    /// change) so matchers should be reused if called often (like in a loop).
    pub fn new(config: Config) -> Self {
        Self {
            config,
            slab: MatrixSlab::new(),
        }
    }

    /// Find the fuzzy match with the highest score in the `haystack`.
    ///
    /// This functions has `O(mn)` time complexity for short inputs.
    /// To avoid slowdowns it automatically falls back to
    /// [greedy matching](crate::Matcher::fuzzy_match_greedy) for large
    /// needles and haystacks.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn fuzzy_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        assert!(haystack.len() <= u32::MAX as usize);
        self.fuzzy_matcher_impl::<false>(haystack, needle, &mut Vec::new())
    }

    /// Find the fuzzy match with the highest score in the `haystack` and
    /// compute its indices.
    ///
    /// This functions has `O(mn)` time complexity for short inputs. To
    /// avoid slowdowns it automatically falls back to
    /// [greedy matching](crate::Matcher::fuzzy_match_greedy) for large needles
    /// and haystacks
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn fuzzy_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        assert!(haystack.len() <= u32::MAX as usize);
        self.fuzzy_matcher_impl::<true>(haystack, needle, indices)
    }

    fn fuzzy_matcher_impl<const INDICES: bool>(
        &mut self,
        haystack_: Utf32Str<'_>,
        needle_: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle_.len() > haystack_.len() {
            return None;
        }
        if needle_.is_empty() {
            return Some(0);
        }
        if needle_.len() == haystack_.len() {
            return self.exact_match_impl::<INDICES>(
                haystack_,
                needle_,
                0,
                haystack_.len(),
                indices,
            );
        }
        assert!(
            haystack_.len() <= u32::MAX as usize,
            "fuzzy matching is only support for up to 2^32-1 codepoints"
        );
        match (haystack_, needle_) {
            (Utf32Str::Ascii(haystack), Utf32Str::Ascii(needle)) => {
                if let &[needle] = needle {
                    return self.substring_match_1_ascii::<INDICES>(haystack, needle, indices);
                }
                let (start, greedy_end, end) = self.prefilter_ascii(haystack, needle, false)?;
                if needle_.len() == end - start {
                    return Some(self.calculate_score::<INDICES, _, _>(
                        AsciiChar::cast(haystack),
                        AsciiChar::cast(needle),
                        start,
                        greedy_end,
                        indices,
                    ));
                }
                self.fuzzy_match_optimal::<INDICES, AsciiChar, AsciiChar>(
                    AsciiChar::cast(haystack),
                    AsciiChar::cast(needle),
                    start,
                    greedy_end,
                    end,
                    indices,
                )
            }
            (Utf32Str::Ascii(_), Utf32Str::Unicode(_)) => {
                // a purely ascii haystack can never be transformed to match
                // a needle that contains non-ascii chars since we don't allow gaps
                None
            }
            (Utf32Str::Unicode(haystack), Utf32Str::Ascii(needle)) => {
                if let &[needle] = needle {
                    let (start, _) = self.prefilter_non_ascii(haystack, needle_, true)?;
                    let res = self.substring_match_1_non_ascii::<INDICES>(
                        haystack,
                        needle as char,
                        start,
                        indices,
                    );
                    return Some(res);
                }
                let (start, end) = self.prefilter_non_ascii(haystack, needle_, false)?;
                if needle_.len() == end - start {
                    return self
                        .exact_match_impl::<INDICES>(haystack_, needle_, start, end, indices);
                }
                self.fuzzy_match_optimal::<INDICES, char, AsciiChar>(
                    haystack,
                    AsciiChar::cast(needle),
                    start,
                    start + 1,
                    end,
                    indices,
                )
            }
            (Utf32Str::Unicode(haystack), Utf32Str::Unicode(needle)) => {
                if let &[needle] = needle {
                    let (start, _) = self.prefilter_non_ascii(haystack, needle_, true)?;
                    let res = self
                        .substring_match_1_non_ascii::<INDICES>(haystack, needle, start, indices);
                    return Some(res);
                }
                let (start, end) = self.prefilter_non_ascii(haystack, needle_, false)?;
                if needle_.len() == end - start {
                    return self
                        .exact_match_impl::<INDICES>(haystack_, needle_, start, end, indices);
                }
                self.fuzzy_match_optimal::<INDICES, char, char>(
                    haystack,
                    needle,
                    start,
                    start + 1,
                    end,
                    indices,
                )
            }
        }
    }

    /// Greedly find a fuzzy match in the `haystack`.
    ///
    /// This functions has `O(n)` time complexity but may provide unintutive (non-optimal)
    /// indices and scores. Usually [fuzzy_match](crate::Matcher::fuzzy_match) should
    /// be preferred.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn fuzzy_match_greedy(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
    ) -> Option<u16> {
        assert!(haystack.len() <= u32::MAX as usize);
        self.fuzzy_match_greedy_impl::<false>(haystack, needle, &mut Vec::new())
    }

    /// Greedly find a fuzzy match in the `haystack` and compute its indices.
    ///
    /// This functions has `O(n)` time complexity but may provide unintuitive (non-optimal)
    /// indices and scores. Usually [fuzzy_indices](crate::Matcher::fuzzy_indices) should
    /// be preferred.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn fuzzy_indices_greedy(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        assert!(haystack.len() <= u32::MAX as usize);
        self.fuzzy_match_greedy_impl::<true>(haystack, needle, indices)
    }

    fn fuzzy_match_greedy_impl<const INDICES: bool>(
        &mut self,
        haystack: Utf32Str<'_>,
        needle_: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle_.len() > haystack.len() {
            return None;
        }
        if needle_.is_empty() {
            return Some(0);
        }
        if needle_.len() == haystack.len() {
            return self.exact_match_impl::<INDICES>(haystack, needle_, 0, haystack.len(), indices);
        }
        assert!(
            haystack.len() <= u32::MAX as usize,
            "matching is only support for up to 2^32-1 codepoints"
        );
        match (haystack, needle_) {
            (Utf32Str::Ascii(haystack), Utf32Str::Ascii(needle)) => {
                let (start, greedy_end, _) = self.prefilter_ascii(haystack, needle, true)?;
                if needle_.len() == greedy_end - start {
                    return Some(self.calculate_score::<INDICES, _, _>(
                        AsciiChar::cast(haystack),
                        AsciiChar::cast(needle),
                        start,
                        greedy_end,
                        indices,
                    ));
                }
                self.fuzzy_match_greedy_::<INDICES, AsciiChar, AsciiChar>(
                    AsciiChar::cast(haystack),
                    AsciiChar::cast(needle),
                    start,
                    greedy_end,
                    indices,
                )
            }
            (Utf32Str::Ascii(_), Utf32Str::Unicode(_)) => {
                // a purely ascii haystack can never be transformed to match
                // a needle that contains non-ascii chars since we don't allow gaps
                None
            }
            (Utf32Str::Unicode(haystack), Utf32Str::Ascii(needle)) => {
                let (start, _) = self.prefilter_non_ascii(haystack, needle_, true)?;
                self.fuzzy_match_greedy_::<INDICES, char, AsciiChar>(
                    haystack,
                    AsciiChar::cast(needle),
                    start,
                    start + 1,
                    indices,
                )
            }
            (Utf32Str::Unicode(haystack), Utf32Str::Unicode(needle)) => {
                let (start, _) = self.prefilter_non_ascii(haystack, needle_, true)?;
                self.fuzzy_match_greedy_::<INDICES, char, char>(
                    haystack,
                    needle,
                    start,
                    start + 1,
                    indices,
                )
            }
        }
    }

    /// Finds the substring match with the highest score in the `haystack`.
    ///
    /// This functions has `O(nm)` time complexity. However many cases can
    /// be significantly accelerated using prefilters so it's usually very fast
    /// in practice.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn substring_match(
        &mut self,
        haystack: Utf32Str<'_>,
        needle_: Utf32Str<'_>,
    ) -> Option<u16> {
        self.substring_match_impl::<false>(haystack, needle_, &mut Vec::new())
    }

    /// Finds the substring match with the highest score in the `haystack` and
    /// compute its indices.
    ///
    /// This functions has `O(nm)` time complexity. However many cases can
    /// be significantly accelerated using prefilters so it's usually fast
    /// in practice.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn substring_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle_: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        self.substring_match_impl::<true>(haystack, needle_, indices)
    }

    fn substring_match_impl<const INDICES: bool>(
        &mut self,
        haystack: Utf32Str<'_>,
        needle_: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle_.len() > haystack.len() {
            return None;
        }
        if needle_.is_empty() {
            return Some(0);
        }
        if needle_.len() == haystack.len() {
            return self.exact_match_impl::<INDICES>(haystack, needle_, 0, haystack.len(), indices);
        }
        assert!(
            haystack.len() <= u32::MAX as usize,
            "matching is only support for up to 2^32-1 codepoints"
        );
        match (haystack, needle_) {
            (Utf32Str::Ascii(haystack), Utf32Str::Ascii(needle)) => {
                if let &[needle] = needle {
                    return self.substring_match_1_ascii::<INDICES>(haystack, needle, indices);
                }
                self.substring_match_ascii::<INDICES>(haystack, needle, indices)
            }
            (Utf32Str::Ascii(_), Utf32Str::Unicode(_)) => {
                // a purely ascii haystack can never be transformed to match
                // a needle that contains non-ascii chars since we don't allow gaps
                None
            }
            (Utf32Str::Unicode(haystack), Utf32Str::Ascii(needle)) => {
                if let &[needle] = needle {
                    let (start, _) = self.prefilter_non_ascii(haystack, needle_, true)?;
                    let res = self.substring_match_1_non_ascii::<INDICES>(
                        haystack,
                        needle as char,
                        start,
                        indices,
                    );
                    return Some(res);
                }
                let (start, _) = self.prefilter_non_ascii(haystack, needle_, false)?;
                self.substring_match_non_ascii::<INDICES, _>(
                    haystack,
                    AsciiChar::cast(needle),
                    start,
                    indices,
                )
            }
            (Utf32Str::Unicode(haystack), Utf32Str::Unicode(needle)) => {
                if let &[needle] = needle {
                    let (start, _) = self.prefilter_non_ascii(haystack, needle_, true)?;
                    let res = self
                        .substring_match_1_non_ascii::<INDICES>(haystack, needle, start, indices);
                    return Some(res);
                }
                let (start, _) = self.prefilter_non_ascii(haystack, needle_, false)?;
                self.substring_match_non_ascii::<INDICES, _>(haystack, needle, start, indices)
            }
        }
    }

    /// Checks whether needle and haystack match exactly.
    ///
    /// This functions has `O(n)` time complexity.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn exact_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut leading_space = 0;
        let mut trailing_space = 0;
        if !needle.first().is_whitespace() {
            leading_space = haystack.leading_white_space()
        }
        if !needle.last().is_whitespace() {
            trailing_space = haystack.trailing_white_space()
        }
        // avoid wraparound in size check
        if trailing_space == haystack.len() {
            return None;
        }
        self.exact_match_impl::<false>(
            haystack,
            needle,
            leading_space,
            haystack.len() - trailing_space,
            &mut Vec::new(),
        )
    }

    /// Checks whether needle and haystack match exactly and compute the matches indices.
    ///
    /// This functions has `O(n)` time complexity.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
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
            leading_space = haystack.leading_white_space()
        }
        if !needle.last().is_whitespace() {
            trailing_space = haystack.trailing_white_space()
        }
        // avoid wraparound in size check
        if trailing_space == haystack.len() {
            return None;
        }
        self.exact_match_impl::<true>(
            haystack,
            needle,
            leading_space,
            haystack.len() - trailing_space,
            indices,
        )
    }

    /// Checks whether needle is a prefix of the haystack.
    ///
    /// This functions has `O(n)` time complexity.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn prefix_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut leading_space = 0;
        if !needle.first().is_whitespace() {
            leading_space = haystack.leading_white_space()
        }
        if haystack.len() - leading_space < needle.len() {
            None
        } else {
            self.exact_match_impl::<false>(
                haystack,
                needle,
                leading_space,
                needle.len() + leading_space,
                &mut Vec::new(),
            )
        }
    }

    /// Checks whether needle is a prefix of the haystack and compute the matches indices.
    ///
    /// This functions has `O(n)` time complexity.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
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
            leading_space = haystack.leading_white_space()
        }
        if haystack.len() - leading_space < needle.len() {
            None
        } else {
            self.exact_match_impl::<true>(
                haystack,
                needle,
                leading_space,
                needle.len() + leading_space,
                indices,
            )
        }
    }

    /// Checks whether needle is a postfix of the haystack.
    ///
    /// This functions has `O(n)` time complexity.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn postfix_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut trailing_spaces = 0;
        if !needle.last().is_whitespace() {
            trailing_spaces = haystack.trailing_white_space()
        }
        if haystack.len() - trailing_spaces < needle.len() {
            None
        } else {
            self.exact_match_impl::<false>(
                haystack,
                needle,
                haystack.len() - needle.len() - trailing_spaces,
                haystack.len() - trailing_spaces,
                &mut Vec::new(),
            )
        }
    }

    /// Checks whether needle is a postfix of the haystack and compute the matches indices.
    ///
    /// This functions has `O(n)` time complexity.
    ///
    /// See the [matcher documentation](crate::Matcher) for more details.
    pub fn postfix_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle.is_empty() {
            return Some(0);
        }
        let mut trailing_spaces = 0;
        if !needle.last().is_whitespace() {
            trailing_spaces = haystack.trailing_white_space()
        }
        if haystack.len() - trailing_spaces < needle.len() {
            None
        } else {
            self.exact_match_impl::<true>(
                haystack,
                needle,
                haystack.len() - needle.len() - trailing_spaces,
                haystack.len() - trailing_spaces,
                indices,
            )
        }
    }

    fn exact_match_impl<const INDICES: bool>(
        &mut self,
        haystack: Utf32Str<'_>,
        needle_: Utf32Str<'_>,
        start: usize,
        end: usize,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        if needle_.len() != end - start {
            return None;
        }
        assert!(
            haystack.len() <= u32::MAX as usize,
            "matching is only support for up to 2^32-1 codepoints"
        );
        let score = match (haystack, needle_) {
            (Utf32Str::Ascii(haystack), Utf32Str::Ascii(needle)) => {
                let matched = if self.config.ignore_case {
                    AsciiChar::cast(haystack)[start..end]
                        .iter()
                        .map(|c| c.normalize(&self.config))
                        .eq(AsciiChar::cast(needle)
                            .iter()
                            .map(|c| c.normalize(&self.config)))
                } else {
                    &haystack[start..end] == needle
                };
                if !matched {
                    return None;
                }
                self.calculate_score::<INDICES, _, _>(
                    AsciiChar::cast(haystack),
                    AsciiChar::cast(needle),
                    start,
                    end,
                    indices,
                )
            }
            (Utf32Str::Ascii(_), Utf32Str::Unicode(_)) => {
                // a purely ascii haystack can never be transformed to match
                // a needle that contains non-ascii chars since we don't allow gaps
                return None;
            }
            (Utf32Str::Unicode(haystack), Utf32Str::Ascii(needle)) => {
                let matched = haystack[start..end]
                    .iter()
                    .map(|c| c.normalize(&self.config))
                    .eq(AsciiChar::cast(needle)
                        .iter()
                        .map(|c| c.normalize(&self.config)));
                if !matched {
                    return None;
                }

                self.calculate_score::<INDICES, _, _>(
                    haystack,
                    AsciiChar::cast(needle),
                    start,
                    end,
                    indices,
                )
            }
            (Utf32Str::Unicode(haystack), Utf32Str::Unicode(needle)) => {
                let matched = haystack[start..end]
                    .iter()
                    .map(|c| c.normalize(&self.config))
                    .eq(needle.iter().map(|c| c.normalize(&self.config)));
                if !matched {
                    return None;
                }
                self.calculate_score::<INDICES, _, _>(haystack, needle, start, end, indices)
            }
        };
        Some(score)
    }
}

