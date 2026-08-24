#!/bin/sh
set -eu

script_dir=$(CDPATH='' cd -P "$(dirname "$0")" && pwd)
renderer="$script_dir/render-homebrew-formula.sh"
fixture_dir=$(mktemp -d "${TMPDIR:-/tmp}/locron-formula-test.XXXXXX")
trap 'find "$fixture_dir" -depth -delete' EXIT HUP INT TERM

formula="$fixture_dir/locron.rb"
broken_formula="$fixture_dir/locron-broken.rb"
a_sha=$(printf '%064s' '' | tr ' ' a)
b_sha=$(printf '%064s' '' | tr ' ' b)
c_sha=$(printf '%064s' '' | tr ' ' c)
d_sha=$(printf '%064s' '' | tr ' ' d)

assert_literal_guidance() {
  candidate=$1
  # These quoted strings intentionally assert that literal Ruby backticks survive rendering.
  # shellcheck disable=SC2016
  grep -Fqx '    # `locron self-update` refuses to replace the binary and' "$candidate" &&
    grep -Fqx '    # directs users to `brew upgrade locron`.' "$candidate" &&
    grep -Fqx '      Installation never starts it automatically, and `brew upgrade`' "$candidate" &&
    grep -Fqx '      `brew services restart locron` after an upgrade.' "$candidate"
}

sh "$renderer" 1.2.3 "$a_sha" "$b_sha" "$c_sha" "$d_sha" > "$formula"

assert_literal_guidance "$formula"
grep -Fqx '      url "https://github.com/WhiteKiwi/locron/releases/download/v1.2.3/locron-v1.2.3-aarch64-apple-darwin.tar.gz"' "$formula"
grep -Fqx "      sha256 \"$a_sha\"" "$formula"
grep -Fqx "      sha256 \"$b_sha\"" "$formula"
grep -Fqx "      sha256 \"$c_sha\"" "$formula"
grep -Fqx "      sha256 \"$d_sha\"" "$formula"
grep -Fqx '    touch lib/".disable-self-update"' "$formula"

if grep -Eq '@[A-Z0-9_]+@|[[:blank:]]$' "$formula"; then
  echo "rendered formula contains a template token or trailing whitespace" >&2
  exit 1
fi

# This fixture intentionally models the command substitutions from the broken unquoted heredoc.
# shellcheck disable=SC2016
sed \
  -e 's/`locron self-update`//g' \
  -e 's/`brew upgrade locron`//g' \
  -e 's/`brew upgrade`//g' \
  -e 's/`brew services restart locron`//g' \
  "$formula" > "$broken_formula"
if assert_literal_guidance "$broken_formula" >/dev/null 2>&1; then
  echo "regression check accepted backtick-stripped guidance" >&2
  exit 1
fi

if sh "$renderer" '1.2.3; false' "$a_sha" "$b_sha" "$c_sha" "$d_sha" >/dev/null 2>&1; then
  echo "renderer accepted an invalid version" >&2
  exit 1
fi
if sh "$renderer" 1.2.3 invalid "$b_sha" "$c_sha" "$d_sha" >/dev/null 2>&1; then
  echo "renderer accepted an invalid checksum" >&2
  exit 1
fi
