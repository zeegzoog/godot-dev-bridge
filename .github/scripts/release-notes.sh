#!/usr/bin/env bash
# Prints the CHANGELOG.md section of a release tag (vX.Y.Z) and fails if there is none or it is
# empty, so a release can never go out without notes.
#
# Usage: .github/scripts/release-notes.sh v0.1.0
set -euo pipefail

tag="${1:?usage: release-notes.sh vX.Y.Z}"
version="${tag#v}"

notes=$(awk -v version="$version" '
    index($0, "## [" version "]") == 1 { found = 1; next }
    found && /^## \[/ { exit }
    found && /^\[[0-9][^]]*\]: / { exit }
    found { print }
' CHANGELOG.md)

if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
    echo "CHANGELOG.md has no entry for $version: add a '## [$version] - YYYY-MM-DD' section" >&2
    exit 1
fi

printf '%s\n' "$notes"
