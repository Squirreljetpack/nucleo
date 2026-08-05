use std::cell::UnsafeCell;
use std::mem::take;
use std::sync::Arc;
use std::sync::atomic::{self, AtomicBool, AtomicU32};

use crate::matcher_inner::Config;
use parking_lot::Mutex;
use rayon::{ThreadPool, prelude::*};

use crate::par_sort::par_quicksort;
use crate::pattern::{self, MultiPattern};
use crate::{Match, boxcar};

struct Matchers(Box<[UnsafeCell<crate::matcher_inner::Matcher>]>);

impl Matchers {
    // this is not a true mut from ref, we use a cell here
    #[allow(clippy::mut_from_ref)]
    unsafe fn get(&self) -> &mut crate::matcher_inner::Matcher {
        &mut *self.0[rayon::current_thread_index().unwrap()].get()
    }
}

unsafe impl Sync for Matchers {}
unsafe impl Send for Matchers {}

pub(crate) struct Worker<T: Sync + Send + 'static> {
    pub(crate) running: bool,
    matchers: Matchers,
    pub(crate) matches: Vec<Match>,
    pub(crate) pattern: MultiPattern,
    pub(crate) stability_threshold: u32,
    pub(crate) reverse_items: bool,
    pub(crate) canceled: Arc<AtomicBool>,
    pub(crate) should_notify: Arc<AtomicBool>,
    pub(crate) was_canceled: bool,
    pub(crate) last_snapshot: u32,
    notify: Arc<dyn Fn() + Sync + Send>,
    pub(crate) items: Arc<boxcar::Vec<T>>,
    in_flight: Vec<u32>,
    pub(crate) custom_sort: Option<Arc<dyn Fn((u32, &T), (u32, &T)) -> bool + Send + Sync>>,
}

impl<T: Sync + Send + 'static> Worker<T> {
    pub(crate) fn item_count(&self) -> u32 {
        self.last_snapshot - self.in_flight.len() as u32
    }
    pub(crate) fn update_config(&mut self, config: Config) {
        for matcher in self.matchers.0.iter_mut() {
            matcher.get_mut().config = config.clone();
        }
    }
    pub(crate) fn sort_results(&mut self, stability_threshold: u32) {
        self.stability_threshold = stability_threshold;
    }
    pub(crate) fn reverse_items(&mut self, reverse_items: bool) {
        self.reverse_items = reverse_items;
    }

    pub(crate) fn new(
        worker_threads: Option<usize>,
        config: Config,
        notify: Arc<dyn Fn() + Sync + Send>,
        cols: u32,
    ) -> (ThreadPool, Self) {
        let worker_threads = worker_threads
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |it| it.get()));
        let pool = rayon::ThreadPoolBuilder::new()
            .thread_name(|i| format!("nucleo worker {i}"))
            .num_threads(worker_threads)
            .build()
            .expect("creating threadpool failed");
        let matchers = (0..worker_threads)
            .map(|_| UnsafeCell::new(crate::matcher_inner::Matcher::new(config.clone())))
            .collect();
        let worker = Worker {
            running: false,
            matchers: Matchers(matchers),
            last_snapshot: 0,
            matches: Vec::new(),
            // just a placeholder
            pattern: MultiPattern::new(cols as usize),
            stability_threshold: 0,
            reverse_items: false,
            canceled: Arc::new(AtomicBool::new(false)),
            should_notify: Arc::new(AtomicBool::new(false)),
            was_canceled: false,
            notify,
            items: Arc::new(boxcar::Vec::with_capacity(2 * 1024, cols)),
            in_flight: Vec::with_capacity(64),
            custom_sort: None,
        };
        (pool, worker)
    }

    unsafe fn process_new_items(&mut self, unmatched: &AtomicU32) {
        let matchers = &self.matchers;
        let pattern = &self.pattern;
        self.matches.reserve(self.in_flight.len());
        self.in_flight.retain(|&idx| {
            let Some(item) = self.items.get(idx) else {
                return true;
            };
            if let Some(score) = pattern.score(item.matcher_columns, matchers.get()) {
                self.matches.push(Match { score, idx });
            };
            false
        });
        let new_snapshot = self.items.par_snapshot(self.last_snapshot);
        if new_snapshot.end() != self.last_snapshot {
            let end = new_snapshot.end();
            let in_flight = Mutex::new(&mut self.in_flight);
            let items = new_snapshot.map(|(idx, item)| {
                let Some(item) = item else {
                    in_flight.lock().push(idx);
                    unmatched.fetch_add(1, atomic::Ordering::Relaxed);
                    return Match {
                        score: 0,
                        idx: u32::MAX,
                    };
                };
                if self.canceled.load(atomic::Ordering::Relaxed) {
                    return Match { score: 0, idx };
                }
                let Some(score) = pattern.score(item.matcher_columns, matchers.get()) else {
                    unmatched.fetch_add(1, atomic::Ordering::Relaxed);
                    return Match {
                        score: 0,
                        idx: u32::MAX,
                    };
                };
                Match { score, idx }
            });
            self.matches.par_extend(items);
            self.last_snapshot = end;
        }
    }

    fn remove_in_flight_matches(&mut self) {
        let mut off = 0;
        self.in_flight.retain(|&i| {
            let is_in_flight = self.items.get(i).is_none();
            if is_in_flight {
                self.matches.remove((i - off) as usize);
                off += 1;
            }
            is_in_flight
        });
    }

    unsafe fn process_new_items_trivial(&mut self) {
        let new_snapshot = self.items.snapshot(self.last_snapshot);
        if new_snapshot.end() != self.last_snapshot {
            let end = new_snapshot.end();
            let items = new_snapshot.filter_map(|(idx, item)| {
                if item.is_none() {
                    self.in_flight.push(idx);
                    return None;
                };
                Some(Match { score: 0, idx })
            });
            self.matches.extend(items);
            self.last_snapshot = end;
        }
    }

    pub(crate) unsafe fn run(&mut self, pattern_status: pattern::Status, cleared: bool) {
        self.running = true;
        self.was_canceled = false;

        if cleared {
            self.last_snapshot = 0;
            self.in_flight.clear();
            self.matches.clear();
        }

        // TODO: be smarter around reusing past results for rescoring
        if self.pattern.is_empty() {
            self.reset_matches();
            self.process_new_items_trivial();
            let canceled = self.sort_matches();
            if canceled {
                self.was_canceled = true;
            } else if self.should_notify.load(atomic::Ordering::Relaxed) {
                (self.notify)();
            }
            return;
        }

        if pattern_status == pattern::Status::Rescore {
            self.reset_matches();
        }

        let mut unmatched = AtomicU32::new(0);
        if pattern_status != pattern::Status::Unchanged && !self.matches.is_empty() {
            self.process_new_items_trivial();
            let matchers = &self.matchers;
            let pattern = &self.pattern;
            self.matches
                .par_iter_mut()
                .take_any_while(|_| !self.canceled.load(atomic::Ordering::Relaxed))
                .for_each(|match_| {
                    if match_.idx == u32::MAX {
                        debug_assert_eq!(match_.score, 0);
                        unmatched.fetch_add(1, atomic::Ordering::Relaxed);
                        return;
                    }
                    // safety: in-flight items are never added to the matches
                    let item = self.items.get_unchecked(match_.idx);
                    if let Some(score) = pattern.score(item.matcher_columns, matchers.get()) {
                        match_.score = score;
                    } else {
                        unmatched.fetch_add(1, atomic::Ordering::Relaxed);
                        match_.score = 0;
                        match_.idx = u32::MAX;
                    }
                });
        } else {
            self.process_new_items(&unmatched);
        }

        let canceled = self.sort_matches();
        if canceled {
            self.was_canceled = true;
        } else {
            self.matches
                .truncate(self.matches.len() - take(unmatched.get_mut()) as usize);
            if self.should_notify.load(atomic::Ordering::Relaxed) {
                (self.notify)();
            }
        }
    }

    unsafe fn sort_matches(&mut self) -> bool {
        macro_rules! execute_sort {
            (|$m1:ident, $m2:ident| $logic:expr) => {
                par_quicksort(
                    &mut self.matches,
                    |$m1, $m2| {
                        if $m1.idx == u32::MAX {
                            return false;
                        }
                        if $m2.idx == u32::MAX {
                            return true;
                        }
                        $logic
                    },
                    &self.canceled,
                )
            };
        }

        match (self.custom_sort.as_ref(), self.stability_threshold) {
            // ---------------------------------------------------------
            // VARIANTS WITH CUSTOM SORT
            // ---------------------------------------------------------

            // Variant 1: Match based + Custom fallback
            (Some(custom_sort), 0) => {
                execute_sort!(|m1, m2| {
                    if m1.score == m2.score {
                        let item1 = self.items.get_unchecked(m1.idx);
                        let item2 = &self.items.get_unchecked(m2.idx);
                        let len1: u32 = item1.matcher_columns.iter().map(|h| h.len() as u32).sum();
                        let len2: u32 = item2.matcher_columns.iter().map(|h| h.len() as u32).sum();

                        if len1 == len2 {
                            custom_sort((m1.idx, item1.data), (m2.idx, item2.data))
                                ^ self.reverse_items
                        } else {
                            len1 < len2
                        }
                    } else {
                        m1.score > m2.score
                    }
                })
            }

            // Variant 2: Custom sort
            (Some(custom_sort), u32::MAX) => {
                execute_sort!(|m1, m2| {
                    let item1 = self.items.get_unchecked(m1.idx);
                    let item2 = self.items.get_unchecked(m2.idx);
                    custom_sort((m1.idx, item1.data), (m2.idx, item2.data)) ^ self.reverse_items
                })
            }

            // Variant 3: Custom Sort + Threshold > 0
            (Some(custom_sort), t) => {
                execute_sort!(|m1, m2| {
                    let s1 = m1.score / t;
                    let s2 = m2.score / t;
                    if s1 == s2 {
                        let item1 = self.items.get_unchecked(m1.idx);
                        let item2 = self.items.get_unchecked(m2.idx);
                        custom_sort((m1.idx, item1.data), (m2.idx, item2.data)) ^ self.reverse_items
                    } else {
                        s1 > s2
                    }
                })
            }

            // ---------------------------------------------------------
            // VARIANTS WITHOUT CUSTOM SORT
            // ---------------------------------------------------------

            // Variant 4: Match based
            (None, 0) => {
                execute_sort!(|m1, m2| {
                    if m1.score == m2.score {
                        let item1 = self.items.get_unchecked(m1.idx);
                        let item2 = &self.items.get_unchecked(m2.idx);
                        let len1: u32 = item1.matcher_columns.iter().map(|h| h.len() as u32).sum();
                        let len2: u32 = item2.matcher_columns.iter().map(|h| h.len() as u32).sum();

                        if len1 == len2 {
                            (m1.idx < m2.idx) ^ self.reverse_items
                        } else {
                            len1 < len2
                        }
                    } else {
                        m1.score > m2.score
                    }
                })
            }

            // Variant 5: Idx based
            (None, u32::MAX) => {
                execute_sort!(|m1, m2| (m1.idx < m2.idx) ^ self.reverse_items)
            }

            // Variant 6: Threshold (Idx)
            (None, t) => {
                execute_sort!(|m1, m2| {
                    let s1 = m1.score / t;
                    let s2 = m2.score / t;
                    if s1 == s2 {
                        (m1.idx < m2.idx) ^ self.reverse_items
                    } else {
                        s1 > s2
                    }
                })
            }
        }
    }

    fn reset_matches(&mut self) {
        self.matches.clear();
        self.matches
            .extend((0..self.last_snapshot).map(|idx| Match { score: 0, idx }));
        // there are usually only very few in flight items (one for each writer)
        self.remove_in_flight_matches();
    }
}
