#!/usr/bin/env bash
set -euo pipefail

git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'

TAG=$(gh api "repos/${UPSTREAM_REPOSITORY:?}/releases/latest" --jq .tag_name)
git fetch --no-tags "${UPSTREAM_GIT_URL:?}" "refs/tags/$TAG"
git merge --no-edit FETCH_HEAD

printf 'tag=%s\n' "$TAG" >> "${GITHUB_OUTPUT:?}"
printf 'commit=%s\n' "$(git rev-parse HEAD)" >> "$GITHUB_OUTPUT"
