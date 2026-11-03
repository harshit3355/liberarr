#!/bin/sh
# Fails if a tracked file (other than .env.example) carries a secret value, or if .env is tracked.
# shortcut: pattern-based, not a full scanner; upgrade to gitleaks if the repo grows contributors.
set -u
cd "$(dirname "$0")/.."
status=0
if git ls-files --error-unmatch .env >/dev/null 2>&1; then echo ".env is tracked!"; status=1; fi
files=$(git ls-files | grep -v -e '^\.env\.example$' -e 'package-lock\.json$' -e 'uv\.lock$')
# shellcheck disable=SC2086
if grep -nE '^[[:space:]]*[A-Z0-9_]*(_KEY|_SECRET|_PASSWORD)[[:space:]]*=[[:space:]]*[^[:space:]$]' $files; then status=1; fi
# shellcheck disable=SC2086
if grep -nE 'sk-[A-Za-z0-9_-]{20,}' $files; then status=1; fi
[ $status -eq 0 ] && echo "no secrets found in tracked files"
exit $status
