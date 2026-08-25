#!/bin/sh
#
# usage.sh - one snapshot of locron's public distribution-channel usage.
#
# Sections:
#   1. GitHub Releases - cumulative asset download counts, per release and in
#      total, from the releases list API. Counts reset when an asset is
#      deleted and re-uploaded, so they are a floor, not an exact ledger.
#   2. Stars - stargazers_count from the repository endpoint.
#   3. Homebrew - install counts for the whitekiwi/tap/locron formula over
#      30/90/365 days from formulae.brew.sh analytics. Analytics are
#      anonymous and opt-out, so the counts understate real installs, and a
#      formula with no recorded installs has no entry (rendered as 0).
#   4. crates.io - N/A before the first registry bootstrap; once published, the
#      sum of the /api/v1/crates/locron/downloads series (trailing 90 days).
#   5. GitHub traffic - 14-day views/clones totals and uniques via gh api,
#      printed only when the GitHub CLI is present and authenticated
#      (owner-only data; a one-line note explains how to enable it).
#
# Rate-limit awareness: when the unauthenticated GitHub REST quota is
# exhausted, the GitHub sections print the limit message with retry guidance
# (GITHUB_TOKEN or gh auth login) instead of raw API errors.
#
# Runtime dependencies: curl plus standard grep/sed/awk. jq is NOT required.
# gh is optional and enables only the traffic section. When GITHUB_TOKEN is
# set (e.g. CI), it is used for the GitHub REST calls to get the
# authenticated quota.
#
# Usage: sh scripts/usage.sh [--json]
#   --json  print the same snapshot as one flat JSON object:
#             snapshot_at            snapshot timestamp (UTC)
#             releases_total         grand total of asset download counts
#             release_<tag>          per-release asset download total
#             releases_truncated     true when the page cap was reached
#             stars                  stargazers_count
#             brew_30d/90d/365d      Homebrew install counts
#             crates_io              download sum, or null before bootstrap
#             traffic_views_total / traffic_views_uniques
#             traffic_clones_total / traffic_clones_uniques
#                                   (present only when gh is authenticated)
#           A failed section contributes a "<section>_error" string key
#           instead of its numeric keys.
#
# Exit status: 0 only when every section succeeded, otherwise 1. A section
# that has no data source (e.g. gh not installed) is not a failure.

set -eu

REPO='WhiteKiwi/locron'
GITHUB_API="https://api.github.com/repos/$REPO"
RELEASES_URL="$GITHUB_API/releases?per_page=100"
STARS_URL="$GITHUB_API"
BREW_URL_PREFIX='https://formulae.brew.sh/api/analytics/install/'
BREW_FORMULA='whitekiwi/tap/locron'
CRATES_URL='https://crates.io/api/v1/crates/locron'
CRATES_UA='locron-usage/0.1 (maintainer; github.com/WhiteKiwi/locron)'
PAGE_CAP=10   # maximum release-list pages to follow (100 releases each)

RATE_LIMIT_MSG='GitHub API rate limit exceeded (unauthenticated quota is 60 requests/hour). Set GITHUB_TOKEN or run gh auth login and retry.'

JSON_MODE=0
case "${1:-}" in
  --json) JSON_MODE=1 ;;
  -h | --help)
    echo "usage: $0 [--json]"
    echo '  --json  print the snapshot as one flat JSON object'
    exit 0
    ;;
  '') ;;
  *)
    echo "usage: $0 [--json]" >&2
    exit 2
    ;;
esac

tmp=$(mktemp -d "${TMPDIR:-/tmp}/locron-usage.XXXXXX") || exit 1
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

# GitHub REST through curl, authenticated when GITHUB_TOKEN is present (CI).
if [ -n "${GITHUB_TOKEN:-}" ]; then
  curl_gh() { curl -sS -H "Authorization: Bearer $GITHUB_TOKEN" "$@"; }
else
  curl_gh() { curl -sS "$@"; }
fi

# Per-section status: ok_* is 1 on success, err_* holds the failure message.
ok_releases=0; err_releases=''
ok_stars=0;    err_stars=''
ok_brew=0;     err_brew=''
ok_crates=0;   err_crates=''
ok_traffic=0;  err_traffic=''; traffic_note=''
gh_limited=0; truncated=0
rel_total=0; stars=''
brew_30d=0; brew_90d=0; brew_365d=0
crates_published=0; crates_value=0
traffic_views_total=''; traffic_views_uniques=''
traffic_clones_total=''; traffic_clones_uniques=''

# ---------------------------------------------------------------------------
# Section 1: GitHub Releases (cumulative asset download counts)
# ---------------------------------------------------------------------------
fetch_releases() {
  stream="$tmp/releases.stream"
  : > "$stream"
  url="$RELEASES_URL"
  page=0
  while [ -n "$url" ]; do
    page=$((page + 1))
    if [ "$page" -gt "$PAGE_CAP" ]; then
      truncated=1
      break
    fi
    if ! body=$(curl_gh -D "$tmp/releases.headers" "$url"); then
      err_releases="failed to fetch $url"
      return 1
    fi
    case "$body" in
      *'API rate limit exceeded'*)
        gh_limited=1
        err_releases="$RATE_LIMIT_MSG"
        return 1
        ;;
    esac
    # Within each release object the tag_name field precedes the assets
    # array, and download_count appears only on asset objects, so the token
    # stream of tag_name/download_count pairs associates every count with
    # the release that precedes it. (git refs cannot contain ':' so a
    # colon-split awk keeps tags intact.) Patterns tolerate the optional
    # whitespace of pretty-printed responses.
    printf '%s\n' "$body" \
      | grep -oE '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"|"download_count"[[:space:]]*:[[:space:]]*[0-9]+' >> "$stream"
    # Follow the Link header to the next page, if any.
    url=$(sed -n 's/.*<\([^>]*\)>; rel="next".*/\1/p' "$tmp/releases.headers")
  done

  if [ ! -s "$stream" ]; then
    err_releases='no release data returned by the GitHub API'
    return 1
  fi
  awk -v entries="$tmp/releases.entries" -v total_file="$tmp/releases.total" '
    BEGIN { FS = ":" }
    /^"tag_name"/ {
      tag = $2
      gsub(/"/, "", tag)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", tag)
      if (!(tag in seen)) { seen[tag] = 1; order[n++] = tag }
    }
    /^"download_count"/ { sum[tag] += $2; total += $2 }
    END {
      for (i = 0; i < n; i++) printf "%s %d\n", order[i], sum[order[i]] > (entries)
      printf "%d\n", total > (total_file)
    }
  ' "$stream"
  rel_total=$(cat "$tmp/releases.total")
  [ -n "$rel_total" ] || rel_total=0
}

# ---------------------------------------------------------------------------
# Section 2: Stars
# ---------------------------------------------------------------------------
fetch_stars() {
  if [ "$gh_limited" -eq 1 ]; then
    err_stars="$RATE_LIMIT_MSG"
    return 1
  fi
  if ! body=$(curl_gh "$STARS_URL"); then
    err_stars="failed to fetch $STARS_URL"
    return 1
  fi
  case "$body" in
    *'API rate limit exceeded'*)
      gh_limited=1
      err_stars="$RATE_LIMIT_MSG"
      return 1
      ;;
  esac
  stars=$(printf '%s\n' "$body" \
    | sed -n 's/.*"stargazers_count"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' | head -n 1)
  if [ -z "$stars" ]; then
    err_stars='could not parse stargazers_count from the GitHub API response'
    return 1
  fi
}

# ---------------------------------------------------------------------------
# Section 3: Homebrew analytics (whitekiwi/tap/locron)
# ---------------------------------------------------------------------------
# Print the install count for the formula in one analytics payload, or 0
# when the formula has no entry (zero recorded installs). Counts carry
# thousands separators, e.g. "count":"1,234".
brew_count() {
  v=$(printf '%s\n' "$1" \
    | grep -o "\"formula\"[[:space:]]*:[[:space:]]*\"$BREW_FORMULA\",\"count\"[[:space:]]*:[[:space:]]*\"[0-9,]*\"" \
    | sed 's/.*"count"[[:space:]]*:[[:space:]]*"\([0-9,]*\)".*/\1/; s/,//g' | head -n 1)
  if [ -n "$v" ]; then
    printf '%s\n' "$v"
  else
    printf '0\n'
  fi
}

fetch_brew() {
  for period in 30 90 365; do
    if ! body=$(curl -sS "${BREW_URL_PREFIX}${period}d.json"); then
      err_brew="failed to fetch brew analytics (${period}d)"
      return 1
    fi
    case "$period" in
      30)  brew_30d=$(brew_count "$body") ;;
      90)  brew_90d=$(brew_count "$body") ;;
      365) brew_365d=$(brew_count "$body") ;;
    esac
  done
}

# ---------------------------------------------------------------------------
# Section 4: crates.io (N/A before the first registry bootstrap)
# ---------------------------------------------------------------------------
fetch_crates() {
  if ! resp=$(curl -sS -A "$CRATES_UA" -w '\n%{http_code}' "$CRATES_URL"); then
    err_crates="failed to fetch $CRATES_URL"
    return 1
  fi
  code=$(printf '%s\n' "$resp" | tail -n 1)
  body=$(printf '%s\n' "$resp" | sed '$d')
  case "$code" in
    200)
      if ! dresp=$(curl -sS -A "$CRATES_UA" -w '\n%{http_code}' "$CRATES_URL/downloads"); then
        err_crates='failed to fetch crates.io download counts'
        return 1
      fi
      dcode=$(printf '%s\n' "$dresp" | tail -n 1)
      downloads=$(printf '%s\n' "$dresp" | sed '$d')
      if [ "$dcode" != 200 ] || [ -z "$downloads" ]; then
        err_crates="unexpected crates.io downloads response (HTTP $dcode)"
        return 1
      fi
      # /downloads is the per-day per-version series for the trailing
      # 90 days; the count is the sum of its download fields.
      crates_value=$(printf '%s\n' "$downloads" \
        | grep -o '"downloads"[[:space:]]*:[[:space:]]*[0-9]*' \
        | awk -F: '{ s += $2 } END { print s }')
      crates_published=1
      ;;
    404)
      crates_published=0
      crates_value=0
      ;;
    *)
      err_crates="unexpected crates.io response (HTTP $code)"
      return 1
      ;;
  esac
}

# ---------------------------------------------------------------------------
# Section 5: GitHub traffic (14-day views/clones; gh present + authenticated)
# ---------------------------------------------------------------------------
fetch_traffic() {
  if ! command -v gh >/dev/null 2>&1; then
    traffic_note="not shown - GitHub CLI not found; install gh and run 'gh auth login' (owner-only 14-day views/clones)"
    return 0
  fi
  if ! gh auth status >/dev/null 2>&1; then
    traffic_note="not shown - gh is not authenticated; run 'gh auth login' or export GITHUB_TOKEN (owner-only 14-day views/clones)"
    return 0
  fi
  if ! views=$(gh api "repos/$REPO/traffic/views" 2>"$tmp/gh.err"); then
    err_traffic="gh api traffic/views failed: $(sed 's/^gh: //' "$tmp/gh.err" | head -n 1)"
    return 1
  fi
  traffic_views_total=$(printf '%s\n' "$views" \
    | sed 's/"views"[[:space:]]*:.*//; s/.*"count"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\),"uniques"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/')
  traffic_views_uniques=$(printf '%s\n' "$views" \
    | sed 's/"views"[[:space:]]*:.*//; s/.*"count"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\),"uniques"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\2/')
  if ! clones=$(gh api "repos/$REPO/traffic/clones" 2>"$tmp/gh.err"); then
    err_traffic="gh api traffic/clones failed: $(sed 's/^gh: //' "$tmp/gh.err" | head -n 1)"
    return 1
  fi
  traffic_clones_total=$(printf '%s\n' "$clones" \
    | sed 's/"clones"[[:space:]]*:.*//; s/.*"count"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\),"uniques"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/')
  traffic_clones_uniques=$(printf '%s\n' "$clones" \
    | sed 's/"clones"[[:space:]]*:.*//; s/.*"count"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\),"uniques"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\2/')
  if [ -z "$traffic_views_total" ] || [ -z "$traffic_views_uniques" ] \
     || [ -z "$traffic_clones_total" ] || [ -z "$traffic_clones_uniques" ]; then
    err_traffic='could not parse gh api traffic response'
    return 1
  fi
}

# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------
# JSON-escape a string for embedding in the emitted object (error messages
# are the only strings that can contain quotes).
json_str() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

render() {
  echo "locron usage snapshot ($(date -u '+%Y-%m-%d %H:%M UTC'))"
  echo
  if [ "$ok_releases" -eq 1 ]; then
    echo 'GitHub Releases (cumulative download counts; reset when an asset is re-uploaded)'
    while read -r tag cnt; do
      printf '  %s: %s\n' "$tag" "$cnt"
    done < "$tmp/releases.entries"
    printf '  %s: %s\n' 'total' "$rel_total"
    if [ "$truncated" -eq 1 ]; then
      echo "  (stopped after $PAGE_CAP pages; more releases exist)"
    fi
  else
    echo "GitHub Releases: FAILED - $err_releases"
  fi
  echo
  if [ "$ok_stars" -eq 1 ]; then
    echo "Stars: $stars"
  else
    echo "Stars: FAILED - $err_stars"
  fi
  echo
  if [ "$ok_brew" -eq 1 ]; then
    echo "Homebrew installs ($BREW_FORMULA; 30/90/365 days): $brew_30d / $brew_90d / $brew_365d"
    echo '  note: brew analytics are anonymous and opt-out, so counts understate real installs'
  else
    echo "Homebrew: FAILED - $err_brew"
  fi
  echo
  if [ "$ok_crates" -eq 1 ]; then
    if [ "$crates_published" -eq 1 ]; then
      echo "crates.io downloads (last 90 days): $crates_value"
    else
      echo 'crates.io downloads: N/A (not published)'
    fi
  else
    echo "crates.io: FAILED - $err_crates"
  fi
  echo
  if [ "$ok_traffic" -eq 1 ]; then
    if [ -n "$traffic_note" ]; then
      echo "GitHub traffic (last 14 days, owner-only): $traffic_note"
    else
      echo 'GitHub traffic (last 14 days, owner-only)'
      echo "  views:  $traffic_views_total total, $traffic_views_uniques unique"
      echo "  clones: $traffic_clones_total total, $traffic_clones_uniques unique"
    fi
  else
    echo "GitHub traffic: FAILED - $err_traffic"
  fi
}

emit_json() {
  parts="\"snapshot_at\": \"$(date -u '+%Y-%m-%d %H:%M UTC')\""
  if [ "$ok_releases" -eq 1 ]; then
    parts="$parts, \"releases_total\": $rel_total"
    while read -r tag cnt; do
      parts="$parts, \"release_$tag\": $cnt"
    done < "$tmp/releases.entries"
    if [ "$truncated" -eq 1 ]; then
      parts="$parts, \"releases_truncated\": true"
    fi
  else
    parts="$parts, \"releases_error\": \"$(json_str "$err_releases")\""
  fi
  if [ "$ok_stars" -eq 1 ]; then
    parts="$parts, \"stars\": $stars"
  else
    parts="$parts, \"stars_error\": \"$(json_str "$err_stars")\""
  fi
  if [ "$ok_brew" -eq 1 ]; then
    parts="$parts, \"brew_30d\": $brew_30d, \"brew_90d\": $brew_90d, \"brew_365d\": $brew_365d"
  else
    parts="$parts, \"brew_error\": \"$(json_str "$err_brew")\""
  fi
  if [ "$ok_crates" -eq 1 ]; then
    if [ "$crates_published" -eq 1 ]; then
      parts="$parts, \"crates_io\": $crates_value"
    else
      parts="$parts, \"crates_io\": null"
    fi
  else
    parts="$parts, \"crates_error\": \"$(json_str "$err_crates")\""
  fi
  if [ "$ok_traffic" -eq 1 ]; then
    # Traffic keys are emitted only when gh is present and authenticated.
    if [ -z "$traffic_note" ]; then
      parts="$parts, \"traffic_views_total\": $traffic_views_total, \"traffic_views_uniques\": $traffic_views_uniques, \"traffic_clones_total\": $traffic_clones_total, \"traffic_clones_uniques\": $traffic_clones_uniques"
    fi
  else
    parts="$parts, \"traffic_error\": \"$(json_str "$err_traffic")\""
  fi
  printf '{%s}\n' "$parts"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
if fetch_releases; then ok_releases=1; fi
if fetch_stars; then ok_stars=1; fi
if fetch_brew; then ok_brew=1; fi
if fetch_crates; then ok_crates=1; fi
if fetch_traffic; then ok_traffic=1; fi

if [ "$JSON_MODE" -eq 1 ]; then
  emit_json
else
  render
fi

status=0
[ "$ok_releases" -eq 1 ] || status=1
[ "$ok_stars" -eq 1 ] || status=1
[ "$ok_brew" -eq 1 ] || status=1
[ "$ok_crates" -eq 1 ] || status=1
[ "$ok_traffic" -eq 1 ] || status=1
exit "$status"
