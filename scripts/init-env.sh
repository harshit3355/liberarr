#!/bin/sh
# Create/complete .env: generates internal secrets, never overwrites a value already set. Idempotent.
set -eu
cd "$(dirname "$0")/.."
[ -f .env ] || cp .env.example .env
umask 077

gen() { # $1=var, $2=value — replace only a change-me or empty value
  if grep -Eq "^$1=(change-me)?\$" .env; then
    awk -v k="$1" -v v="$2" 'BEGIN{FS=OFS="="} $1==k{$0=k"="v} {print}' .env > .env.tmp && mv .env.tmp .env
    echo "generated $1"
    [ "$1" = LIBERARR_ADMIN_PASSWORD ] && echo "  admin password (shown once): $2"
  fi
  return 0
}

gen LIBERARR_SECRET_KEY "$(openssl rand -hex 32)"
gen LIBERARR_API_KEY "$(openssl rand -hex 32)"
gen LITELLM_MASTER_KEY "sk-$(openssl rand -hex 32)"
gen LIBERARR_ADMIN_PASSWORD "$(openssl rand -base64 18 | tr -d '/+=')"
chmod 600 .env

missing=$(grep -E '^(OPENAI|ANTHROPIC|GROQ)_API_KEY=$' .env | cut -d= -f1 | tr '\n' ' ')
[ -n "$missing" ] && echo "still empty (fill those used in litellm.yaml): $missing"
exit 0
