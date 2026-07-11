#!/usr/bin/env bash
# One-time repository setup: init git if needed, set the repo-local identity.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -d .git ]; then
  git init -b main
fi

git config user.name "Shola Ayeni"
git config user.email "ayenisholah@yahoo.com"

echo "setup: OK — commits will be authored by $(git config user.name) <$(git config user.email)>"
