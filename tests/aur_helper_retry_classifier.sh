#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=../synsyu/lib/commands.sh
. "$REPO_ROOT/synsyu/lib/commands.sh"

assert_retryable() {
  local message="$1"
  if ! aur_helper_failure_retryable "$message"; then
    printf 'expected retryable AUR helper failure: %s\n' "$message" >&2
    exit 1
  fi
}

assert_terminal() {
  local message="$1"
  if aur_helper_failure_retryable "$message"; then
    printf 'expected terminal AUR helper failure: %s\n' "$message" >&2
    exit 1
  fi
}

assert_retryable "fatal: unable to access 'https://aur.archlinux.org/foo.git/': Could not resolve host"
assert_retryable "error: RPC failed; curl 56 HTTP/2 stream was not closed cleanly"
assert_retryable "curl: (28) Operation timed out after 30000 milliseconds"
assert_retryable "The requested URL returned error: 503"

assert_terminal "==> ERROR: One or more files did not pass the validity check!"
assert_terminal "==> ERROR: PGP signature verification failed!"
assert_terminal "error: could not satisfy dependencies"
assert_terminal "==> ERROR: PKGBUILD contains invalid syntax"
assert_terminal "==> ERROR: A failure occurred in build()."
assert_terminal "fatal: unable to access 'https://aur.archlinux.org/foo.git/': Could not resolve host; ==> ERROR: One or more files did not pass the validity check!"
