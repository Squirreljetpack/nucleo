mod atom;
mod multi;
mod single;

pub use atom::{Atom, AtomKind, CaseMatching, Normalization};
pub use multi::{MultiPattern, Variant};
pub(crate) use multi::Status;
pub use single::Pattern;

#[cfg(test)]
mod tests;
