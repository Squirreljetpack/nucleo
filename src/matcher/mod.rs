//! Low level fuzzy matching utilities and engines.

pub mod chars;
pub mod frizbee;
pub mod nucleo;
pub mod utf32_str;

#[cfg(test)]
mod tests;

pub use frizbee::FrizbeeMatcher;
pub use nucleo::{Config, NucleoMatcher};
pub use utf32_str::{Utf32Str, Utf32String};

/// Matching is limited to 2^32-1 codepoints, if the haystack is longer than
/// that the matcher **will panic**. The caller must decide whether it wants to
/// filter out long haystacks or truncate them.
#[derive(Debug, Clone)]
pub enum Matcher {
    /// Nucleo's default matrix matcher.
    Nucleo(NucleoMatcher),
    /// Frizbee's SIMD Smith-Waterman matcher.
    Frizbee(FrizbeeMatcher),
}

impl Default for Matcher {
    fn default() -> Self {
        Matcher::Nucleo(NucleoMatcher::default())
    }
}

impl Matcher {
    /// Creates a new Nucleo matcher instance.
    pub fn new(config: Config) -> Self {
        Matcher::Nucleo(NucleoMatcher::new(config))
    }

    /// Creates a new Frizbee matcher instance.
    pub fn new_frizbee(config: frizbee::Config) -> Self {
        Matcher::Frizbee(FrizbeeMatcher::new(config))
    }

    /// Creates a new matcher instance matching the provided backend.
    pub fn new_with_backend(backend: crate::MatcherBackend) -> Self {
        match backend {
            crate::MatcherBackend::Nucleo(cfg) => Self::new(cfg),
            crate::MatcherBackend::Frizbee(cfg) => Self::new_frizbee(cfg),
        }
    }

    /// Ensures this matcher matches the backend kind and updates its config.
    pub fn ensure_backend(&mut self, backend: &crate::MatcherBackend) {
        match (self, backend) {
            (Matcher::Nucleo(m), crate::MatcherBackend::Nucleo(cfg)) => {
                m.config = cfg.clone();
            }
            (Matcher::Frizbee(m), crate::MatcherBackend::Frizbee(cfg)) => {
                m.set_config(cfg);
            }
            (this, backend) => {
                *this = Matcher::new_with_backend(backend.clone());
            }
        }
    }

    /// Sets case matching and normalization flags on backends that support them.
    pub fn set_casing(&mut self, ignore_case: bool, normalize: bool) {
        match self {
            Matcher::Nucleo(m) => {
                m.config.ignore_case = ignore_case;
                m.config.normalize = normalize;
            }
            Matcher::Frizbee(_) => {}
        }
    }

    /// Find the fuzzy match with the highest score in the `haystack`.
    pub fn fuzzy_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.fuzzy_match(haystack, needle),
            Matcher::Frizbee(m) => m.fuzzy_match(haystack, needle),
        }
    }

    /// Find the fuzzy match with the highest score in the `haystack` and compute its indices.
    pub fn fuzzy_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.fuzzy_indices(haystack, needle, indices),
            Matcher::Frizbee(m) => m.fuzzy_indices(haystack, needle, indices),
        }
    }

    /// Greedily find a fuzzy match in the `haystack`.
    pub fn fuzzy_match_greedy(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.fuzzy_match_greedy(haystack, needle),
            Matcher::Frizbee(m) => m.fuzzy_match_greedy(haystack, needle),
        }
    }

    /// Greedily find a fuzzy match in the `haystack` and compute its indices.
    pub fn fuzzy_indices_greedy(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.fuzzy_indices_greedy(haystack, needle, indices),
            Matcher::Frizbee(m) => m.fuzzy_indices_greedy(haystack, needle, indices),
        }
    }

    /// Checks whether needle and haystack match exactly.
    pub fn exact_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.exact_match(haystack, needle),
            Matcher::Frizbee(m) => m.exact_match(haystack, needle),
        }
    }

    /// Checks whether needle and haystack match exactly and compute match indices.
    pub fn exact_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.exact_indices(haystack, needle, indices),
            Matcher::Frizbee(m) => m.exact_indices(haystack, needle, indices),
        }
    }

    /// Checks whether needle is a prefix of the haystack.
    pub fn prefix_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.prefix_match(haystack, needle),
            Matcher::Frizbee(m) => m.prefix_match(haystack, needle),
        }
    }

    /// Checks whether needle is a prefix of the haystack and compute match indices.
    pub fn prefix_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.prefix_indices(haystack, needle, indices),
            Matcher::Frizbee(m) => m.prefix_indices(haystack, needle, indices),
        }
    }

    /// Checks whether needle is a postfix of the haystack.
    pub fn postfix_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.postfix_match(haystack, needle),
            Matcher::Frizbee(m) => m.postfix_match(haystack, needle),
        }
    }

    /// Checks whether needle is a postfix of the haystack and compute match indices.
    pub fn postfix_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.postfix_indices(haystack, needle, indices),
            Matcher::Frizbee(m) => m.postfix_indices(haystack, needle, indices),
        }
    }

    /// Finds the substring match with the highest score in the `haystack`.
    pub fn substring_match(&mut self, haystack: Utf32Str<'_>, needle: Utf32Str<'_>) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.substring_match(haystack, needle),
            Matcher::Frizbee(m) => m.substring_match(haystack, needle),
        }
    }

    /// Finds the substring match with the highest score in the `haystack` and compute its indices.
    pub fn substring_indices(
        &mut self,
        haystack: Utf32Str<'_>,
        needle: Utf32Str<'_>,
        indices: &mut Vec<u32>,
    ) -> Option<u16> {
        match self {
            Matcher::Nucleo(m) => m.substring_indices(haystack, needle, indices),
            Matcher::Frizbee(m) => m.substring_indices(haystack, needle, indices),
        }
    }
}
