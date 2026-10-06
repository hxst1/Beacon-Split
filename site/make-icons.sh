#!/bin/sh
# Draws every icon the page offers from `favicon.svg`, which is the only one
# anybody should edit.
#
# Google Search does not accept an SVG favicon — its formats are BMP, GIF, ICO,
# PNG, JPEG, PPM and TIFF — so a page offering only one is a page with no icon
# beside it in the results. That is what this exists to prevent.
#
#   ./make-icons.sh
#
# Needs librsvg and Pillow:  brew install librsvg && pip install pillow

set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

# iOS rounds the corners itself and puts a transparent icon on white, so its
# one is drawn square and full-bleed.
/usr/bin/sed 's|<rect width="64" height="64" rx="14.3"/>|<rect width="64" height="64"/>|' \
  favicon.svg > /tmp/beacon-icon-square.svg

rsvg-convert -w 192 -h 192 favicon.svg -o favicon-192.png
rsvg-convert -w 512 -h 512 favicon.svg -o favicon-512.png
rsvg-convert -w 180 -h 180 /tmp/beacon-icon-square.svg -o apple-touch-icon.png

# One .ico holding the sizes a browser or a crawler asks for, drawn down from
# the largest so the bar stays sharp.
python3 -c "
from PIL import Image
Image.open('favicon-512.png').save('favicon.ico', sizes=[(16,16),(32,32),(48,48)])
"

rm -f /tmp/beacon-icon-square.svg
echo "drawn from favicon.svg:"
echo "  favicon.ico            16, 32 and 48, for search engines and browsers"
echo "  favicon-192.png        the manifest"
echo "  favicon-512.png        the manifest"
echo "  apple-touch-icon.png   180, square, for iOS"
