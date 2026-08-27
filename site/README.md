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
vercel deploy --prod
```

`vercel.json` sets the security headers and asset caching. There is no build
command and no output directory to configure — the files are served as they are.

## Keeping it honest

Two things go stale and both are visible from the front page:

- The **version and file sizes** in the install section, and the two `.dmg`
  links, which point at a specific tag.
- The **platform section**, which currently says Linux is being built.

Both are hand-written. When a release goes out, check them.
