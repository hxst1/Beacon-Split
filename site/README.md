# The site

The landing page at the root of this directory. Static: HTML, one stylesheet,
one small script, no build step and no dependencies.

## Why there is no framework here

The page is one route with no state to manage. A framework would add a build,
a lockfile and a dependency surface to produce the same bytes that are already
written here, and would make the thing this page is selling harder to change
rather than easier.

## The visual system is the application's

`styles.css` lifts its surfaces, hairlines, radii and accent straight from
[`src/styles/tokens.css`](../src/styles/tokens.css), so the page and the product
are the same material. Two things deliberately differ:

- **Text colours.** The application's `--fg-3` sits near 2.9:1 — fine for dense
  chrome you scan, wrong for prose you read. The site keeps its own `--t-1`..
  `--t-4` scale, and everything carrying copy clears 4.5:1.
- **A display face.** `Instrument Sans` carries the headings. The application
  has no display voice because it never needed one.

If a token changes in the application, change it here too. Nothing imports
across, on purpose: the site must keep working without the app's build.

## The typefaces are served from here

`fonts/` holds the woff2 files Google Fonts serves, unchanged, in latin and
latin-ext only — the alphabet this page is written in. The `@font-face` rules
are at the top of `styles.css` and carry the same `unicode-range` values, so a
browser still fetches latin-ext only if some text needs it.

Three reasons for not linking to Google. A stylesheet on another origin blocks
the first paint on a DNS lookup, a handshake and a round trip before the
browser has learned which files it needs. Browsers partition their HTTP cache
per site now, so the shared-CDN argument bought nothing any more. And this page
tells people it has no server and no telemetry, which sits badly with sending
every visitor's address to a third party before rendering a word.

Two faces are preloaded from `index.html`: Inter 400 and Instrument Sans 500,
which is what the first screen is made of. To add a weight, take the file and
its `unicode-range` from the Google Fonts stylesheet and drop both in.

## The window in the hero

Drawn in HTML and CSS, not captured. It stays crisp at any resolution, weighs
nothing, and can be corrected in a text editor when the product moves on. Every
size inside it is in `em` off one `font-size` on `.win`, so the whole drawing
scales as a drawing rather than reflowing into a shape the product never has.

To replace it with a real screenshot later, swap `.win` for an `<img>` inside
`.stage` and keep `.win__accent` for the workspace edge.

## Running it

```sh
cd site && python3 -m http.server 4321
```

## Deploying

Vercel, from this directory:

```sh
vercel login          # once
cd site
./set-origin.sh https://your-domain
vercel deploy --prod
```

`vercel.json` sets the security headers, the content security policy and asset
caching. There is no build command and no output directory to configure — the
files are served as they are.

### `set-origin.sh` is not optional

Four things need an absolute URL and cannot be committed with one, because
until the site is deployed there is no domain to write: the canonical link,
`og:url`, the absolute `og:image`, and `sitemap.xml`. Pointing any of them at a
domain that does not serve this page is worse than leaving it out — it tells a
crawler the real copy is somewhere it cannot fetch.

So they arrive with the deploy. The script writes all four plus the `Sitemap:`
line in `robots.txt`, and running it again with a different domain replaces
what it wrote rather than stacking. `sitemap.xml` is git-ignored for the same
reason: it is a deploy artefact, not a source file.

## Found by machines

Three things exist for that and are easy to break by accident:

- **Two `application/ld+json` blocks** in the head. One describes the
  application; the other is the FAQ. The FAQ block repeats the `#faq` section
  word for word, which is the condition for using the markup at all — it may
  not claim an answer the page does not give. Change one, change both.
- **`llms.txt`**, for the engines that answer questions rather than list links.
  Same facts, no markup, and honest about what is not built.
- **Heading order.** One `h1`, then `h2` per section and `h3` beneath it, with
  no level skipped. The outline is what both a screen reader and an extractor
  read the page through.

## Keeping it honest

The install section used to go stale one release after anybody touched it: the
version, download links and file sizes were hand-written against a
specific tag. They are now filled in from the latest GitHub release by
`main.js`, asked for only once the download section comes into view — this page
says it has no telemetry, and reaching a third party on load for something most
visitors never scroll to would be a small lie.

The written-out links are still real and still work; they are the floor for
anyone with scripts blocked or the API rate-limited. **If they ever need
editing by hand, the fetch is broken — fix that instead.**

The contributor list follows the same rule. It is a lazy request to GitHub's
contributors endpoint only when the section approaches the viewport, and its
static fallback is the project's contributor graph. GitHub's count is commits
to the default branch: a reproducible form of credit, not a measure of the
value of anybody's work.

What is still hand-written and does go stale:

- The **platform section**, which currently says Linux is being built.
- The **FAQ**, in both places it lives.
- The **hero illustration**, which draws a version of the product that will
  eventually stop looking like it.
