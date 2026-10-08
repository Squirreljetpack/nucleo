use crate::matcher::Matcher;
use crate::pattern::{
    Atom, AtomKind, CaseMatching, MultiPattern, Normalization, Pattern, Status, Variant,
};
use crate::Utf32String;

#[test]
fn negative() {
    let pat = Atom::parse("!foo", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.negative);
    assert_eq!(pat.kind, AtomKind::Substring);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("!^foo", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.negative);
    assert_eq!(pat.kind, AtomKind::Prefix);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("!foo$", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.negative);
    assert_eq!(pat.kind, AtomKind::Postfix);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("!^foo$", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.negative);
    assert_eq!(pat.kind, AtomKind::Exact);
    assert_eq!(pat.needle.to_string(), "foo");
}

#[test]
fn pattern_kinds() {
    let pat = Atom::parse("foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("'foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Substring);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("^foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Prefix);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("foo$", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Postfix);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("^foo$", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Exact);
    assert_eq!(pat.needle.to_string(), "foo");
}

#[test]
fn case_matching() {
    let pat = Atom::parse("foo", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("Foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "Foo");
    let pat = Atom::parse("Foo", CaseMatching::Ignore, Normalization::Smart);
    assert!(pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("Foo", CaseMatching::Respect, Normalization::Smart);
    assert!(!pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "Foo");
    let pat = Atom::parse("foo", CaseMatching::Respect, Normalization::Smart);
    assert!(!pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "foo");
}

#[test]
fn escape() {
    let pat = Atom::parse("\\!foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    assert_eq!(pat.needle.to_string(), "!foo");
    let pat = Atom::parse("\\^foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    assert_eq!(pat.needle.to_string(), "^foo");
    let pat = Atom::parse("\\'foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    assert_eq!(pat.needle.to_string(), "'foo");
    let pat = Atom::parse("foo\\$", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    assert_eq!(pat.needle.to_string(), "foo$");
}

#[test]
fn pattern_atoms() {
    let pat = Pattern::parse("foo\\ bar baz", CaseMatching::Smart, Normalization::Smart);
    let needles: Vec<_> = pat.atoms.iter().map(|pat| pat.needle.to_string()).collect();
    assert_eq!(needles, &["foo bar", "baz"]);
}

#[test]
fn append() {
    let mut pat = MultiPattern::new(1);
    pat.reparse(0, "!", CaseMatching::Smart, Normalization::Smart, true);
    assert_eq!(pat.status(), Status::Update);
    pat.reparse(0, "!f", CaseMatching::Smart, Normalization::Smart, true);
    assert_eq!(pat.status(), Status::Update);
    pat.reparse(0, "!fo", CaseMatching::Smart, Normalization::Smart, true);
    assert_eq!(pat.status(), Status::Rescore);
}

#[test]
fn multi_pattern_scoring_negative_and_optional() {
    let mut matcher = Matcher::default();
    let haystack_clean = [Utf32String::from("hello world"), Utf32String::from("extra")];
    let haystack_dirty = [Utf32String::from("hello foo"), Utf32String::from("extra")];

    // 1. Purely negative query on a single column
    let mut pat = MultiPattern::new(2);
    pat.reparse(0, "!foo", CaseMatching::Smart, Normalization::Smart, false);
    assert_eq!(pat.score(&haystack_clean, &mut matcher), Some(0));
    assert_eq!(pat.score(&haystack_dirty, &mut matcher), None);

    // 2. Purely negative on col 0 + optional positive on col 1
    let mut pat2 = MultiPattern::new(2);
    pat2.configure_column(1, Variant::Optional);
    pat2.reparse(0, "!foo", CaseMatching::Smart, Normalization::Smart, false);
    pat2.reparse(1, "bar", CaseMatching::Smart, Normalization::Smart, false);
    assert_eq!(pat2.score(&haystack_clean, &mut matcher), Some(0));
    assert_eq!(pat2.score(&haystack_dirty, &mut matcher), None);

    // 3. Optional column only, non-matching -> must reject
    let mut pat3 = MultiPattern::new(2);
    pat3.configure_column(1, Variant::Optional);
    pat3.reparse(1, "nomatch", CaseMatching::Smart, Normalization::Smart, false);
    assert_eq!(pat3.score(&haystack_clean, &mut matcher), None);

    // 4. Optional column only, matching -> must accept with score > 0
    let mut pat4 = MultiPattern::new(2);
    pat4.configure_column(1, Variant::Optional);
    pat4.reparse(1, "extra", CaseMatching::Smart, Normalization::Smart, false);
    assert!(pat4.score(&haystack_clean, &mut matcher).is_some_and(|s| s > 0));

    // 5. Empty pattern across all columns -> matches with Some(0)
    let pat5 = MultiPattern::new(2);
    assert_eq!(pat5.score(&haystack_clean, &mut matcher), Some(0));
}
