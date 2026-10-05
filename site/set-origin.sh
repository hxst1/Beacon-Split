#!/bin/sh
# Stamps the site's public origin into the places that need an absolute URL,
# writes the sitemap, and stamps the version Beacon is at.
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
  -e "s|^  \"url\": \"[^\"]*\",\$|  \"url\": \"$origin/\",|" \
  -e "s|^  \"image\": \"[^\"]*/og\.png\",\$|  \"image\": \"$origin/og.png\",|" \
  -e "s|^  \"@id\": \"[^\"]*#software\",\$|  \"@id\": \"$origin/#software\",|" \
  index.html

# ── the version ─────────────────────────────────────────────────────────────
# Taken from package.json, which is the same version this deploy's release was
# cut from, so nobody has to remember to update the page. `main.js` replaces it
# with whatever GitHub says is latest; this is what a reader without
# JavaScript, and every crawler, is told.
version=$(/usr/bin/sed -n 's/^  "version": "\(.*\)",$/\1/p' ../package.json | head -1)
if [ -n "$version" ]; then
  /usr/bin/sed -i '' -e "s|\(data-dl-version[^>]*>\)[^<]*<|\1$version<|g" index.html
else
  echo "warning: could not read the version from ../package.json" >&2
fi

# ── robots.txt ──────────────────────────────────────────────────────────────
/usr/bin/sed -i '' '/^Sitemap:/d' robots.txt
# Trailing blank lines go too, or every run would leave one more behind.
/usr/bin/perl -0pi -e 's/\n+\z/\n/' robots.txt
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
echo "  index.html   canonical, og:url, og:image, twitter:image and the JSON-LD @id, url and image"
echo "  index.html   the version, from package.json"
echo "  robots.txt   Sitemap line"
echo "  sitemap.xml  written"
