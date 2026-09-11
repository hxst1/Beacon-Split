#!/bin/sh
# Stamps the site's public origin into the four places that need an absolute
# URL, and writes the sitemap.
#
# These are left out of the committed files on purpose. A canonical link, an
# og:url or a sitemap pointing at a domain that does not serve this page is
# worse than not having one: it tells a crawler the real copy is somewhere it
# cannot fetch. So they arrive with the deploy, when the answer is known.
#
#   ./set-origin.sh https://example.com
#
# Idempotent: run it again after changing domains and it replaces what it wrote.

set -eu

if [ $# -ne 1 ]; then
  echo "usage: $0 https://your-domain" >&2
  exit 2
fi

origin=$(printf '%s' "$1" | sed 's:/*$::')

case "$origin" in
  https://*) ;;
  *) echo "the origin must start with https:// — got '$origin'" >&2; exit 2 ;;
esac

here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

# ── index.html ──────────────────────────────────────────────────────────────
# Anything a previous run left behind goes first, so this never stacks.
/usr/bin/sed -i '' \
  -e '/<link rel="canonical"/d' \
  -e '/<meta property="og:url"/d' \
  index.html

/usr/bin/sed -i '' \
  -e "s|<link rel=\"icon\"|<link rel=\"canonical\" href=\"$origin/\">\\
<meta property=\"og:url\" content=\"$origin/\">\\
<link rel=\"icon\"|" \
  -e "s|content=\"[^\"]*/og\.png\"|content=\"$origin/og.png\"|g" \
  index.html

# ── robots.txt ──────────────────────────────────────────────────────────────
/usr/bin/sed -i '' '/^Sitemap:/d' robots.txt
printf '\nSitemap: %s/sitemap.xml\n' "$origin" >> robots.txt

# ── sitemap.xml ─────────────────────────────────────────────────────────────
# One page, so one entry. `lastmod` is the day it was stamped, which is the day
# it was deployed — the only date about this file that is true.
cat > sitemap.xml <<XML
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>$origin/</loc>
    <lastmod>$(date -u +%Y-%m-%d)</lastmod>
  </url>
</urlset>
XML

echo "origin set to $origin"
echo "  index.html   canonical, og:url, absolute og:image and twitter:image"
echo "  robots.txt   Sitemap line"
echo "  sitemap.xml  written"
