//! The crate's own source, as a test can read it.
//!
//! Two of this crate's promises are about the shape of the tree rather than about a value:
//! nothing starts a child process but the guard, and nothing turns a value into JSON text but
//! the envelope. Both are the kind of thing that holds by convention right up until the day a
//! new module quietly does it too, so both are tests, and this is what they read.
//!
//! Every file is cut at its own `#[cfg(test)]`: test code spawns processes and serialises
//! documents for its own reasons, and the claim is about the code that ships.

use std::fs;
use std::path::{Path, PathBuf};

/// Every shipped source file of this crate, as its name under `src/` and the source above its
/// tests.
///
/// # Panics
///
/// If the crate's own source cannot be read, or if it finds so few files that it would be
/// measuring nothing.
pub fn shipped() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    assert!(files.len() > 4, "the scan found almost nothing, so it is measuring nothing");
    files
}

fn collect(root: &Path, directory: &Path, into: &mut Vec<(String, String)>) {
    let entries = fs::read_dir(directory).expect("the source directory is readable");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = fs::read_to_string(&path).expect("a source file is read");
            let shipped = source
                .split_once("#[cfg(test)]")
                .map_or(source.as_str(), |(before, _)| before)
                .to_owned();
            let name = path.strip_prefix(root).expect("under src").display().to_string();
            into.push((name, shipped));
        }
    }
}

/// A path spelled in pieces, so that a test naming what it counts cannot be counted.
///
/// The cut above already keeps a test's own mentions out of the scan; this keeps the claim
/// true whichever side of the cut the test ends up on.
pub fn spelled(parts: &[&str]) -> String {
    parts.join("::")
}

/// How many of `spellings` appear in each shipped file, against how many are allowed there.
///
/// Anything not named in `allowed` may have none, which is the point: a new module is held to
/// the promise without anyone remembering to add it. `because` is what the reader of a red
/// test needs — where the thing belongs, rather than only that it is here.
///
/// # Panics
///
/// With the file, both counts and `because`, if a file does the thing more or fewer times
/// than it may.
pub fn is_confined_to(spellings: &[String], allowed: &[(&str, usize)], what: &str, because: &str) {
    for (name, source) in shipped() {
        let found: usize = spellings.iter().map(|spelling| source.matches(spelling).count()).sum();
        let expected =
            allowed.iter().find(|(file, _)| *file == name).map_or(0, |(_, count)| *count);
        assert_eq!(found, expected, "{name} {what} {found} times and may {expected}: {because}");
    }
}
