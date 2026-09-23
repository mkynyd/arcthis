#!/usr/bin/env bash
# Deploy the Arcthis site to arcthis.mkynstudio.top.
#
# Cache strategy: HTML pages are revalidated by the browser as usual, while the
# shared assets (/assets/style.css, /assets/main.js) carry a ?v=<timestamp>
# query. Every deploy bumps that version in all pages, so returning visitors
# always pick up the latest CSS/JS without a hard refresh.
#
# Usage: site/deploy.sh

set -euo pipefail

cd "$(dirname "$0")"

V="$(date +%Y%m%d%H%M%S)"
REMOTE_HOST="remoteDev"
REMOTE_ROOT="/www/wwwroot/arcthis.mkynstudio.top"

PAGES="index.html download.html docs.html en/index.html en/download.html en/docs.html"

for f in $PAGES; do
  sed -i '' -E "s#/assets/(style\.css|main\.js)(\?v=[0-9]+)?#/assets/\1?v=$V#g" "$f"
done

rsync -avz --exclude '.DS_Store' --exclude 'deploy.sh' ./ "$REMOTE_HOST:$REMOTE_ROOT/"

# Keep deployed files owned by the web user; .user.ini is immutable on BT Panel.
ssh "$REMOTE_HOST" "
  set -eu
  chown -R www:www \
    '$REMOTE_ROOT/index.html' '$REMOTE_ROOT/download.html' '$REMOTE_ROOT/docs.html' \
    '$REMOTE_ROOT/assets' '$REMOTE_ROOT/en'
  if find \
    '$REMOTE_ROOT/index.html' '$REMOTE_ROOT/download.html' '$REMOTE_ROOT/docs.html' \
    '$REMOTE_ROOT/assets' '$REMOTE_ROOT/en' \
    \( ! -user www -o ! -group www \) -print -quit | grep -q .; then
    echo 'deployment ownership verification failed' >&2
    exit 1
  fi
"

echo "Deployed with asset version $V"
