//! The tarball, which is the first artefact that runs without the repository.
//!
//! Both tables this face holds are `include_str!`-compiled into the binary, so the copies
//! that ship beside it in the tarball are for the reader — and a copy is a second source that
//! can disagree with the first. The disagreement that matters is the compatibility table: a
//! tarball carrying a table wider than the one its binary was compiled against would tell a
//! reader that a machine satisfies the stack while the binary refuses to dispatch on it, and
//! the reader would be consulting the wrong document to find out why. The exit table has the
//! same shape of failure one step further on, where a caller maps an exit by a rule the binary
//! does not use.
//!
//! So the copies are not trusted to have been copied from the right place. Each one is
//! searched for, byte for byte, inside the binary that ships beside it, and so is the version
//! the archive's own name claims. A drifted copy does not make a tarball that is merely
//! unlikely to be wrong; it makes a tarball that cannot be built.
//!
//! Nothing here executes what it packaged. The release workflow builds for a target the runner
//! may not be able to run, and an assertion that only holds for a native build is an assertion
//! the tarballs that matter never get.
//!
//! CI substitutes three things: the binary to package (`FRAISE_PACKAGE_BINARY`, the release
//! build rather than this test's debug one), the target it was built for
//! (`FRAISE_PACKAGE_TARGET`, since a cross-built binary is not the host's), and where to leave
//! the archive (`FRAISE_PACKAGE_OUT`, so that the job uploads the very file these assertions
//! passed on rather than a second one built beside it). The version is not among them: it comes
//! from `CARGO_PKG_VERSION`, which is the one the binary itself carries, and the assertions
//! below are what hold the name to it.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

/// The packaging script, which is the only thing that decides what a release tarball is.
fn script() -> PathBuf {
    let path = repository().join("tools/package.sh");
    assert!(path.is_file(), "there is no {} to package with", path.display());
    path
}

/// The repository root, from the crate this test belongs to.
fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the root")
}

/// The binary to package: the release build when CI hands one over, otherwise this test's own.
fn binary_to_package() -> PathBuf {
    env::var_os("FRAISE_PACKAGE_BINARY")
        .map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_fraise")), PathBuf::from)
}

/// Where to leave the archive: the directory CI collects from when it names one.
fn out_directory() -> PathBuf {
    let path = env::var_os("FRAISE_PACKAGE_OUT")
        .map_or_else(|| Path::new(env!("CARGO_TARGET_TMPDIR")).join("package"), PathBuf::from);
    fs::create_dir_all(&path).expect("the output directory is created");
    path
}

/// The target triple the binary being packaged was built for.
///
/// The release workflow says which, because it cross-builds and the answer is not this
/// machine's. Otherwise it is the host as `rustc` names it, read from the toolchain rather than
/// assembled from what this process knows about the machine, since it ends up in the artefact's
/// name.
fn target_of_binary() -> String {
    if let Some(target) = env::var_os("FRAISE_PACKAGE_TARGET") {
        return target.into_string().expect("the target triple is utf-8");
    }
    let output = Command::new("rustc").arg("-vV").output().expect("rustc runs");
    let report = String::from_utf8(output.stdout).expect("rustc's report is utf-8");
    report
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("rustc names its host triple")
        .to_owned()
}

/// Whether `haystack` holds `needle` exactly as it is, anywhere in it.
fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

/// The same bytes with one of them changed, which is what a drifted copy amounts to.
fn drifted(document: &[u8]) -> Vec<u8> {
    let mut bytes = document.to_owned();
    let last = bytes.last_mut().expect("the document is not empty");
    *last = last.wrapping_add(1);
    bytes
}

#[test]
fn the_tarball_holds_the_binary_the_two_tables_and_the_licence() {
    let out = out_directory();
    let binary = binary_to_package();
    let target = target_of_binary();
    let version = env!("CARGO_PKG_VERSION");

    let packaged = Command::new(script())
        .args(["--binary".as_ref(), binary.as_os_str()])
        .args(["--target", &target])
        .args(["--version", version])
        .args(["--out".as_ref(), out.as_os_str()])
        .output()
        .expect("the packaging script runs");
    assert!(
        packaged.status.success(),
        "packaging exited with {:?}; stderr: {}",
        packaged.status.code(),
        String::from_utf8_lossy(&packaged.stderr)
    );

    // The script says where it left the archive, and that is the file every assertion below is
    // about — CI uploads what these assertions passed on, so the two cannot be different files.
    let archive = PathBuf::from(String::from_utf8_lossy(&packaged.stdout).trim());
    let directory = format!("fraise-{version}-{target}");
    assert_eq!(
        archive.file_name().and_then(|name| name.to_str()),
        Some(format!("{directory}.tar.gz").as_str()),
        "the archive names the version and the target it was built for"
    );
    assert!(archive.is_file(), "{} exists", archive.display());

    let listing = run("tar", &["tzf".as_ref(), archive.as_os_str()]);
    for entry in listing.lines() {
        assert!(entry.starts_with(&directory), "{entry} is outside {directory}");
    }
    let mut files: Vec<&str> = listing.lines().filter(|entry| !entry.ends_with('/')).collect();
    files.sort_unstable();
    assert_eq!(
        files,
        [
            format!("{directory}/LICENSE"),
            format!("{directory}/compatibility.toml"),
            format!("{directory}/exit_table.vendored.json"),
            format!("{directory}/fraise"),
        ]
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>(),
        "the tarball holds the binary, both tables and the licence, and nothing else"
    );

    let extracted = out.join("extracted");
    let _ = fs::remove_dir_all(&extracted);
    fs::create_dir_all(&extracted).expect("the extraction directory is created");
    run(
        "tar",
        &[
            "xzf".as_ref(),
            archive.as_os_str(),
            "-C".as_ref(),
            extracted.as_os_str(),
        ],
    );
    let root = extracted.join(&directory);

    let shipped_binary = fs::read(root.join("fraise")).expect("the binary is in the tarball");
    assert_eq!(
        shipped_binary,
        fs::read(&binary).expect("the binary handed to the script is readable"),
        "the packaged binary is the one the script was given"
    );
    let mode = fs::metadata(root.join("fraise"))
        .expect("the binary is there")
        .permissions()
        .mode();
    assert!(mode & 0o111 != 0, "the packaged binary is executable: mode {mode:o}");

    // The claim that makes the copies safe to read. Comparing them with `crates/fraise/src/`
    // would only prove the script copied from the place it was told to; comparing them with
    // the binary proves the reader and the binary are looking at the same document. The
    // drifted copy is checked too, so a search that found anything would fail here.
    for name in ["exit_table.vendored.json", "compatibility.toml"] {
        let document = fs::read(root.join(name)).expect("the table is in the tarball");
        assert!(
            holds(&shipped_binary, &document),
            "{name} in the tarball is not the one compiled into the binary beside it"
        );
        assert!(
            !holds(&shipped_binary, &drifted(&document)),
            "the search for {name} inside the binary finds documents that are not there"
        );
    }

    // A version in the name that the binary does not carry would misname every report made
    // from the tarball, and is the one claim about the archive a reader cannot check.
    assert!(
        holds(&shipped_binary, version.as_bytes()),
        "the binary does not carry the version {version} its archive is named for"
    );

    assert_eq!(
        fs::read(root.join("LICENSE")).expect("the licence is in the tarball"),
        fs::read(repository().join("LICENSE")).expect("the repository's licence is readable"),
        "the packaged licence is the repository's"
    );
}

#[test]
fn packaging_refuses_rather_than_guessing_what_to_package() {
    // Every input is a fact about the artefact's identity — which build, which target, which
    // version — and a default for any of them would be a tarball that names something nobody
    // checked. The release workflow passes all four; so does the test above.
    let output = Command::new(script()).output().expect("the packaging script runs");
    assert!(!output.status.success(), "packaging with nothing to package succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage:"), "the refusal says how to call it: {stderr}");
}

/// A command that has to work for the test to mean anything, and its standard output.
fn run(program: &str, args: &[&std::ffi::OsStr]) -> String {
    let output = Command::new(program).args(args).output().expect("the command runs");
    assert!(
        output.status.success(),
        "{program} exited with {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("the command's output is utf-8")
}
