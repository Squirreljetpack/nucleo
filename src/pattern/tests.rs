use crate::pattern::{Atom, AtomKind, CaseMatching, MultiPattern, Normalization, Pattern, Status};

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
