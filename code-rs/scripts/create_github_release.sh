#!/bin/bash

set -euo pipefail

# This script creates a GitHub release by tagging the current commit.
# The GitHub Actions workflow will handle updating the version in Cargo.toml during build.
#
# Usage:
#     ./scripts/create_github_release.sh 0.1.0
#     ./scripts/create_github_release.sh 0.1.0-alpha.4
#
# If no version is specified, a timestamp-based version will be used.

# Change to the root of the repository (two levels up from scripts dir).
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

# Cancel if there are uncommitted changes.
if ! git diff --quiet || ! git diff --cached --quiet || [ -n "$(git ls-files --others --exclude-standard)" ]; then
  echo "ERROR: You have uncommitted or untracked changes." >&2
  echo "       Please commit or stash your changes before creating a release." >&2
  exit 1
fi

# Fail if in a detached HEAD state.
CURRENT_BRANCH=$(git symbolic-ref --short -q HEAD 2>/dev/null || true)
if [ -z "${CURRENT_BRANCH:-}" ]; then
  echo "ERROR: Could not determine the current branch (detached HEAD?)." >&2
  echo "       Please run this script from a checked-out branch." >&2
  exit 1
fi

# Ensure we are on the 'main' branch before proceeding.
if [ "${CURRENT_BRANCH}" != "main" ]; then
  echo "ERROR: Releases must be created from the 'main' branch (current: '${CURRENT_BRANCH}')." >&2
  echo "       Please switch to 'main' and try again." >&2
  exit 1
fi

# Ensure the current local commit on 'main' is present on 'origin/main'.
# This guarantees we only create releases from commits that are already on
# the canonical repository.
if ! git fetch --quiet origin main; then
  echo "ERROR: Failed to fetch 'origin/main'. Ensure the 'origin' remote is configured and reachable." >&2
  exit 1
fi

if ! git merge-base --is-ancestor HEAD origin/main; then
  echo "ERROR: Your local 'main' HEAD commit is not present on 'origin/main'." >&2
  echo "       Please push your commits first (git push origin main) or check out a commit on 'origin/main'." >&2
  exit 1
fi

# Determine version
if [ $# -ge 1 ]; then
  VERSION="$1"
else
  VERSION=$(printf '0.0.%d' "$(date +%y%m%d%H%M)")
fi

# Validate version format
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-(alpha|beta)(\.[0-9]+)?)?$ ]]; then
  echo "ERROR: Invalid version format: $VERSION" >&2
  echo "       Expected format: X.Y.Z or X.Y.Z-alpha.N or X.Y.Z-beta.N" >&2
  exit 1
fi

TAG="rust-v$VERSION"

# Check if tag already exists
if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "ERROR: Tag $TAG already exists." >&2
  echo "       Please choose a different version." >&2
  exit 1
fi

echo "Creating release for version: $VERSION"
echo "Tag: $TAG"
echo ""
echo "The GitHub Actions workflow will:"
echo "  1. Update version in Cargo.toml to $VERSION"
echo "  2. Build binaries with the correct version embedded"
echo "  3. Create a GitHub release with artifacts"
echo ""
read -p "Continue? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 1
fi

# Create and push the tag
git tag -a "$TAG" -m "Release $VERSION"
git push origin "refs/tags/$TAG"

echo ""
echo "OK: Tag $TAG pushed successfully!"
echo ""
echo "NOTE: The release workflow has been triggered."
echo "   View progress at: https://github.com/${GITHUB_REPOSITORY:-$(git remote get-url origin | sed 's/.*github.com[:/]\(.*\)\.git/\1/')}/actions"
echo ""
echo "Once the workflow completes, the release will be available at:"
echo "   https://github.com/${GITHUB_REPOSITORY:-$(git remote get-url origin | sed 's/.*github.com[:/]\(.*\)\.git/\1/')}/releases/tag/$TAG"
