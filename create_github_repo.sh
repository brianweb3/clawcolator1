#!/bin/bash
# Create GitHub repo "clawcolator" under the authenticated user.
# Usage: GITHUB_TOKEN=your_token ./create_github_repo.sh

set -e
if [ -z "$GITHUB_TOKEN" ]; then
  echo "Set GITHUB_TOKEN first: export GITHUB_TOKEN=ghp_..."
  exit 1
fi

NAME="clawcolator"
DESCRIPTION="Clawcolator is an agent-first fork of Percolator where all market decisions are delegated to an autonomous OpenClaw agent, while the protocol strictly enforces financial invariants and system safety."

RESP=$(curl -sS -w "\n%{http_code}" -X POST -H "Authorization: token $GITHUB_TOKEN" -H "Accept: application/vnd.github.v3+json" \
  https://api.github.com/user/repos \
  -d "{\"name\":\"$NAME\",\"description\":\"$DESCRIPTION\",\"private\":false}")

HTTP_CODE=$(echo "$RESP" | tail -n1)
BODY=$(echo "$RESP" | sed '$d')

if [ "$HTTP_CODE" = "201" ]; then
  echo "Repo created successfully."
  echo "$BODY" | grep -q '"html_url"' && echo "$BODY" | sed -n 's/.*"html_url": "\([^"]*\)".*/\1/p' | head -1
else
  echo "Error (HTTP $HTTP_CODE): $BODY"
  exit 1
fi
