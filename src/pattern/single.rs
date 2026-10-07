use std::cmp::Reverse;

use super::atom::{Atom, AtomKind, CaseMatching, Normalization};
use crate::matcher::{Matcher, Utf32Str};

fn pattern_atoms(pattern: &str) -> impl Iterator<Item = &str> + '_ {
    let mut saw_backslash = false;
    pattern.split(move |c| {
        saw_backslash = match c {
            c if c.is_whitespace() && !saw_backslash => return true,
            '\\' => true,
            _ => false,
        };
        false
    })
}

#[derive(Debug, Default)]
/// A text pattern made up of (potentially multiple) [atoms](crate::pattern::Atom).
#[non_exhaustive]
pub struct Pattern {
    /// The individual pattern (words) in this pattern
    pub atoms: Vec<Atom>,
}

impl Pattern {
    /// Creates a pattern where each word is matched individually (whitespaces
    /// can be escaped with `\`). Otherwise no parsing is performed (so `$`, `!`,
    /// `'` and `^` don't receive special treatment). If you want to match the entire
    /// pattern as a single needle use a single [`Atom`] instead.
    pub fn new(
        pattern: &str,
        case_matching: CaseMatching,
        normalize: Normalization,
        kind: AtomKind,
    ) -> Pattern {
        let atoms = pattern_atoms(pattern)
            .filter_map(|pat| {
                let pat = Atom::new(pat, case_matching, normalize, kind, true);
                (!pat.needle.is_empty()).then_some(pat)
            })
            .collect();
        Pattern { atoms }
    }

    /// Creates a pattern where each word is matched individually (whitespaces
    /// can be escaped with `\`). And `$`, `!`, `'` and `^` at word boundaries will
    /// cause different matching behaviour (see [`AtomKind`]). These can be
    /// escaped with backslash.
    pub fn parse(pattern: &str, case_matching: CaseMatching, normalize: Normalization) -> Pattern {
        let atoms = pattern_atoms(pattern)
            .filter_map(|pat| {
                let pat = Atom::parse(pat, case_matching, normalize);
                (!pat.needle.is_empty()).then_some(pat)
            })
            .collect();
        Pattern { atoms }
    }

    /// Convenience function to easily match (and sort) a (relatively small)
    /// list of inputs.
    pub fn match_list<T: AsRef<str>>(
        &self,
        items: impl IntoIterator<Item = T>,
        matcher: &mut Matcher,
    ) -> Vec<(T, u32)> {
        if self.atoms.is_empty() {
            return items.into_iter().map(|item| (item, 0)).collect();
        }
        let mut buf = Vec::new();
        let mut items: Vec<_> = items
            .into_iter()
            .filter_map(|item| {
                self.score(Utf32Str::new(item.as_ref(), &mut buf), matcher)
                    .map(|score| (item, score))
            })
            .collect();
        items.sort_by_key(|(_, score)| Reverse(*score));
        items
    }

    /// Matches this pattern against `haystack` (using the allocation and configuration
    /// from `matcher`) and calculates a ranking score.
    pub fn score(&self, haystack: Utf32Str<'_>, matcher: &mut Matcher) -> Option<u32> {
        if self.atoms.is_empty() {
            return Some(0);
        }
        let mut score = 0;
        for pattern in &self.atoms {
            score += pattern.score(haystack, matcher)? as u32;
        }
        Some(score)
    }

    /// Matches this pattern against `haystack` (using the allocation and
    /// configuration from `matcher`), calculates a ranking score and the match
    /// indices.
    pub fn indices(
        &self,
        haystack: Utf32Str<'_>,
        matcher: &mut Matcher,
        indices: &mut Vec<u32>,
    ) -> Option<u32> {
        if self.atoms.is_empty() {
            return Some(0);
        }
        let mut score = 0;
        for pattern in &self.atoms {
            score += pattern.indices(haystack, matcher, indices)? as u32;
        }
        Some(score)
    }

    /// Refreshes this pattern by reparsing it from a string.
    pub fn reparse(
        &mut self,
        pattern: &str,
        case_matching: CaseMatching,
        normalize: Normalization,
    ) {
        self.atoms.clear();
        let atoms = pattern_atoms(pattern).filter_map(|atom| {
            let atom = Atom::parse(atom, case_matching, normalize);
            if atom.needle.is_empty() {
                return None;
            }
            Some(atom)
        });
        self.atoms.extend(atoms);
    }
}

impl Clone for Pattern {
    fn clone(&self) -> Self {
        Self {
            atoms: self.atoms.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.atoms.clone_from(&source.atoms);
    }
}
