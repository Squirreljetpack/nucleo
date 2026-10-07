use std::sync::Arc;

use crate::matcher::Config;

use crate::Nucleo;

#[test]
fn active_injector_count() {
    let mut nucleo: Nucleo<()> = Nucleo::new(Config::DEFAULT, Arc::new(|| ()), Some(1), 1);
    assert_eq!(nucleo.active_injectors(), 0);
    let injector = nucleo.injector();
    assert_eq!(nucleo.active_injectors(), 1);
    let injector2 = nucleo.injector();
    assert_eq!(nucleo.active_injectors(), 2);
    drop(injector2);
    assert_eq!(nucleo.active_injectors(), 1);
    nucleo.restart(false);
    assert_eq!(nucleo.active_injectors(), 0);
    let injector3 = nucleo.injector();
    assert_eq!(nucleo.active_injectors(), 1);
    nucleo.tick(0);
    assert_eq!(nucleo.active_injectors(), 1);
    drop(injector);
    assert_eq!(nucleo.active_injectors(), 1);
    drop(injector3);
    assert_eq!(nucleo.active_injectors(), 0);
}

#[test]
fn custom_sorting() {
    let mut nucleo: Nucleo<String> = Nucleo::new(Config::DEFAULT, Arc::new(|| ()), Some(1), 1);

    // Set custom sorting function: sort by String length ascending
    nucleo.sort_with(Some(Arc::new(|(_, a), (_, b)| {
        a.len().cmp(&b.len()) == std::cmp::Ordering::Less
    })));

    let injector = nucleo.injector();
    injector.push("banana".to_string(), |s, cols| cols[0] = s.clone().into());
    injector.push("apple".to_string(), |s, cols| cols[0] = s.clone().into());
    injector.push("pear".to_string(), |s, cols| cols[0] = s.clone().into());

    // Trigger tick to process items
    nucleo.tick(50);

    let snapshot = nucleo.snapshot();
    let matches = snapshot.matches();

    // With custom sort by length, matches should be in order of length: pear (4), apple (5), banana (6)
    assert_eq!(matches.len(), 3);
    assert_eq!(snapshot.get_item(matches[0].idx).unwrap().data, "pear");
    assert_eq!(snapshot.get_item(matches[1].idx).unwrap().data, "apple");
    assert_eq!(snapshot.get_item(matches[2].idx).unwrap().data, "banana");
}

#[test]
fn custom_sorting_immediate() {
    let mut nucleo: Nucleo<String> = Nucleo::new(Config::DEFAULT, Arc::new(|| ()), Some(1), 1);
    nucleo.sort_results(false);

    let injector = nucleo.injector();
    injector.push("banana".to_string(), |s, cols| cols[0] = s.clone().into());
    injector.push("apple".to_string(), |s, cols| cols[0] = s.clone().into());
    injector.push("pear".to_string(), |s, cols| cols[0] = s.clone().into());

    // First tick: items are processed and sorted by default (insertion order since sort_results is false)
    nucleo.tick(50);

    {
        let snapshot = nucleo.snapshot();
        let matches = snapshot.matches();
        assert_eq!(matches.len(), 3);
        // By default, they should be in the order they were pushed: banana, apple, pear
        assert_eq!(snapshot.get_item(matches[0].idx).unwrap().data, "banana");
    }

    // Now set custom sorting function: sort by String length ascending
    nucleo.sort_with(Some(Arc::new(|(_, a), (_, b)| {
        a.len().cmp(&b.len()) == std::cmp::Ordering::Less
    })));
    nucleo.resort();

    // Second tick: should start the resort in the background
    let status = nucleo.tick(50);
    if status.running {
        // Wait for the background resort to finish and update the snapshot
        nucleo.tick(50);
    }

    let snapshot = nucleo.snapshot();
    let matches = snapshot.matches();
    assert_eq!(matches.len(), 3);
    assert_eq!(snapshot.get_item(matches[0].idx).unwrap().data, "pear");
    assert_eq!(snapshot.get_item(matches[1].idx).unwrap().data, "apple");
    assert_eq!(snapshot.get_item(matches[2].idx).unwrap().data, "banana");
}

#[test]
fn frizbee_backend_e2e() {
    let mut nucleo: Nucleo<String> = Nucleo::new_with_backend(
        crate::MatcherBackend::Frizbee(crate::frizbee::Config::default()),
        Arc::new(|| ()),
        Some(1),
        1,
    );

    let injector = nucleo.injector();
    injector.push("foo/bar/baz.rs".to_string(), |s, cols| cols[0] = s.clone().into());
    injector.push("qux/bar/xyz.rs".to_string(), |s, cols| cols[0] = s.clone().into());
    injector.push("unrelated.txt".to_string(), |s, cols| cols[0] = s.clone().into());

    nucleo.pattern.reparse(
        0,
        "bar",
        crate::pattern::CaseMatching::Ignore,
        crate::pattern::Normalization::Smart,
        false,
    );

    while nucleo.tick(50).running {}

    let snapshot = nucleo.snapshot();
    let matches = snapshot.matches();
    assert_eq!(matches.len(), 2);

    let mut matcher = crate::Matcher::new_frizbee(crate::matcher::frizbee::Config::default());
    let mut indices = Vec::new();

    let item0 = snapshot.get_matched_item(0).unwrap();
    let score = snapshot.indices(0, &item0, &mut matcher, &mut indices);
    assert!(score.is_some());
    assert_eq!(&indices, &[4, 5, 6]);

    // Test updating backend to Nucleo
    nucleo.update_backend(crate::MatcherBackend::Nucleo(crate::Config::DEFAULT));
    nucleo.pattern.reparse(
        0,
        "bar",
        crate::pattern::CaseMatching::Ignore,
        crate::pattern::Normalization::Smart,
        false,
    );
    while nucleo.tick(50).running {}
    let snapshot2 = nucleo.snapshot();
    assert_eq!(snapshot2.matches().len(), 2);
}
