#!/usr/bin/env bash
# Build one release tarball: the binary, the two tables it carries, and the licence.
#
# `fraise` is the one command a person or an agent installs, so the tarball is the first
# artefact of this repository that runs without the repository. Alongside the binary it ships
# the two documents the binary was compiled against — `exit_table.vendored.json` and
# `compatibility.toml` — under the names they have in the tree, so that a reader can diff the
# copy they were given against the one a commit changed.
#
# Those copies are the risk this script does not try to manage on its own. Both are
# `include_str!`-compiled into the binary, so a copy is a second source: a shipped
# compatibility table wider than the binary's would call a machine ready that the binary
# refuses to dispatch on. `crates/fraise/tests/package.rs` runs this script and then searches
# the binary for each document byte for byte, which is a claim about the artefact rather than
# about this script's `install` lines. It is not done here because a shell version of it cannot
# be trusted to mean the same thing twice: `grep -F -f document binary` takes each *line* of
# the document as a pattern of its own and answers yes to a document sharing one line with the
# binary, and `grep` is ugrep on some of these machines, where `-z` searches inside archives
# instead of splitting records on NUL. So the script stages and tars, and the gate asserts.
#
# Nothing is built here and nothing is guessed. Which build, which target and which version are
# facts about the artefact's identity, and a default for any of them would name a tarball after
# something nobody checked — the release workflow cross-builds for a target this script could
# not have inferred, and the crate version is the one the binary itself carries.

set -euo pipefail

usage() {
	cat >&2 <<'USAGE'
usage: tools/package.sh --binary <path> --target <triple> --version <version> --out <dir>

  --binary   the built `fraise` to ship, already built for --target
  --target   the target triple that build was made for
  --version  the crate version that build carries
  --out      the directory to leave the archive in

Writes <out>/fraise-<version>-<target>.tar.gz and prints its path.
USAGE
	exit 2
}

fail() {
	printf 'package.sh: %s\n' "$1" >&2
	exit 2
}

binary=''
target=''
version=''
out=''

while [ "$#" -gt 0 ]; do
	case "$1" in
	--binary | --target | --version | --out)
		[ "$#" -ge 2 ] || fail "$1 needs a value"
		case "$1" in
		--binary) binary="$2" ;;
		--target) target="$2" ;;
		--version) version="$2" ;;
		--out) out="$2" ;;
		esac
		shift 2
		;;
	-h | --help) usage ;;
	*) fail "unknown argument: $1" ;;
	esac
done

if [ -z "$binary" ] || [ -z "$target" ] || [ -z "$version" ] || [ -z "$out" ]; then
	usage
fi
[ -f "$binary" ] || fail "no such binary: $binary"

# The repository this script is part of, so that it packages the tables of the tree it was run
# from whatever directory that was.
root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
name="fraise-$version-$target"
stage="$out/$name"
archive="$out/$name.tar.gz"

mkdir -p -- "$out"
rm -rf -- "$stage"
mkdir -- "$stage"

# One directory inside the archive rather than a bare binary at its root, which is what the
# other tools in the table ship: `tar xz -C ~/.local/bin` is the install line those tarballs
# document, and four loose files would land four things in a directory meant for one.
install -m 0755 -- "$binary" "$stage/fraise"
install -m 0644 -- "$root/crates/fraise/src/exit_table.vendored.json" "$stage/"
install -m 0644 -- "$root/crates/fraise/src/compatibility.toml" "$stage/"
install -m 0644 -- "$root/LICENSE" "$stage/"

rm -f -- "$archive"
tar czf "$archive" -C "$out" "$name"
rm -rf -- "$stage"

printf '%s\n' "$archive"
