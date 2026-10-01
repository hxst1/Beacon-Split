/*
 * Three small things, and deliberately no more:
 *
 *  1. The nav gains its material once the page has moved.
 *  2. Sections settle in as they arrive — from a visible default, so the page
 *     reads fine with this file blocked.
 *  3. The hero's one authored moment: a project stops, its tab turns amber,
 *     and the notification arrives. That sequence is the product's argument,
 *     so it is the only thing on the page that performs.
 */

document.documentElement.classList.add('js');

/* ── nav ──────────────────────────────────────────────────────────── */

const nav = document.getElementById('nav');
if (nav) {
  const sync = () => nav.classList.toggle('is-stuck', window.scrollY > 12);
  sync();
  addEventListener('scroll', sync, { passive: true });
}

/* ── reveal ───────────────────────────────────────────────────────── */

const reduced = matchMedia('(prefers-reduced-motion: reduce)');

if ('IntersectionObserver' in window && !reduced.matches) {
  const targets = document.querySelectorAll('.band .wrap, .row2, .stage');
  targets.forEach((el, i) => {
    el.classList.add('reveal');
    // A short stagger inside a row, never a queue down the whole page.
    el.style.transitionDelay = `${(i % 2) * 70}ms`;
  });

  const show = (el) => {
    el.classList.add('is-in');
    io.unobserve(el);
  };

  // Any overlap at all, rather than a fraction of the element.
  //
  // A fraction is the wrong test for something taller than the window: six per
  // cent of a long section is hundreds of pixels, so arriving at that section
  // from a link — `#faq`, say — could put its top just below the line and
  // leave the whole thing invisible until you scrolled. The negative bottom
  // margin is what delays the reveal on the way up; the threshold was only
  // ever meant to do the same job twice.
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) if (e.isIntersecting) show(e.target);
    },
    { rootMargin: '0px 0px -12% 0px', threshold: 0 },
  );
  targets.forEach((el) => io.observe(el));

  /*
   * Somebody who arrived at a section rather than scrolled to it.
   *
   * The nav links jump straight to `#what`, `#platforms`, `#faq` and `#open`,
   * and so does anyone following a link to one. That must never be a blank
   * screen waiting for a scroll it will not get: what was asked for is shown
   * at once, and only what is still ahead of them animates.
   */
  const revealHash = () => {
    const target = location.hash && document.querySelector(location.hash);
    if (!target) return;
    for (const el of targets) if (target.contains(el) || el.contains(target)) show(el);
  };

  revealHash();
  addEventListener('hashchange', revealHash);
}

/* ── the hero moment ──────────────────────────────────────────────── */

const dot = document.getElementById('dotB');
const note = document.getElementById('note');

if (dot && note && !reduced.matches) {
  const steps = [
    // [ms held, tab state, notification visible]
    [4200, 'working', false],
    [1000, 'waiting', false], // amber first; the notification follows a beat later
    [5200, 'waiting', true],
    [1400, 'idle', false],
  ];

  let i = 0;
  let timer;

  const play = () => {
    const [hold, state, showNote] = steps[i];
    dot.dataset.state = state;
    note.classList.toggle('is-in', showNote);
    i = (i + 1) % steps.length;
    timer = setTimeout(play, hold);
  };

  // Only run while the hero is actually on screen; a background tab should not
  // be animating, and neither should a section nobody is looking at.
  const stage = document.querySelector('.stage');
  const gate = new IntersectionObserver(
    ([e]) => {
      if (e.isIntersecting && !timer) {
        play();
      } else if (!e.isIntersecting && timer) {
        clearTimeout(timer);
        timer = null;
        note.classList.remove('is-in');
      }
    },
    { threshold: 0.15 },
  );
  gate.observe(stage);

  document.addEventListener('visibilitychange', () => {
    if (document.hidden && timer) {
      clearTimeout(timer);
      timer = null;
      note.classList.remove('is-in');
    }
  });
}

/* ── copy ─────────────────────────────────────────────────────────── */

for (const btn of document.querySelectorAll('[data-copy]')) {
  btn.addEventListener('click', async () => {
    const src = document.querySelector(btn.dataset.copy);
    if (!src) return;
    try {
      await navigator.clipboard.writeText(src.textContent.trim());
      btn.textContent = 'Copied';
      btn.classList.add('is-done');
    } catch {
      // Clipboard refused — say so rather than claiming a copy that never happened.
      btn.textContent = 'Select it';
      const range = document.createRange();
      range.selectNodeContents(src);
      const sel = getSelection();
      sel.removeAllRanges();
      sel.addRange(range);
    }
    setTimeout(() => {
      btn.textContent = 'Copy';
      btn.classList.remove('is-done');
    }, 2000);
  });
}

/* ── downloads ────────────────────────────────────────────────────── */

/*
 * Fills the install section from the latest GitHub release.
 *
 * The links in the HTML are written out for a real release and work on their
 * own — with this file blocked, with no network beyond GitHub itself, and for
 * anything that reads the page without running scripts. What they cannot be is
 * current: they were true the day somebody typed them, and the version,
 * download links and file sizes went stale one release later every time.
 *
 * So this asks, and only replaces what it got an answer for. Anything missing
 * or refused — the API is rate limited by IP and says so plainly — leaves the
 * written-out release in place, which is a slightly old download rather than a
 * broken one.
 */
const fillDownloads = async (dl) => {
  const repo = dl.dataset.dlRepo;
  if (!repo) return;

  const response = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, {
    headers: { Accept: 'application/vnd.github+json' },
  });
  if (!response.ok) return;

  const release = await response.json();
  const tag = typeof release.tag_name === 'string' ? release.tag_name : '';
  const version = tag.replace(/^v/, '');
  const assets = Array.isArray(release.assets) ? release.assets : [];
  if (!version || assets.length === 0) return;

  for (const card of dl.querySelectorAll('[data-dl-suffix]')) {
    // Matched on the suffix the build produces rather than on the whole name,
    // which carries the version and would need this to know it in advance:
    // `_aarch64.dmg` and `_x64.dmg` for macOS, `_x64-setup.exe` for Windows.
    const suffix = card.dataset.dlSuffix;
    const asset = assets.find((a) => typeof a.name === 'string' && a.name.endsWith(suffix));
    if (!asset?.browser_download_url) continue;

    card.href = asset.browser_download_url;
    const size = card.querySelector('[data-dl-size]');
    if (size && typeof asset.size === 'number') {
      size.textContent = ` · ${(asset.size / 1024 / 1024).toFixed(1)} MB`;
    }
  }

  for (const el of document.querySelectorAll('[data-dl-version]')) el.textContent = version;
};

const downloads = document.querySelector('[data-dl-repo]');

if (downloads) {
  // Asked for only once somebody has scrolled to the downloads.
  //
  // This page says it has no telemetry and no server, and it should not then
  // reach a third party on load for something most visitors never look at. A
  // request made because you went looking for the download is part of getting
  // the download; one made because you opened the page is not.
  //
  // Nothing waits for it and a failure is not worth a word: the written-out
  // release is already on screen and already works.
  const ask = () => fillDownloads(downloads).catch(() => {});

  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        io.disconnect();
        ask();
      },
      { rootMargin: '400px 0px' },
    );
    io.observe(downloads);
  } else {
    ask();
  }
}

/* ── contributors ─────────────────────────────────────────────────── */

/*
 * A credit list belongs to the people who made the work, not to a manually
 * maintained sentence that will be forgotten at the next merge. GitHub's
 * contributors endpoint reports commits that reached the default branch; that
 * is an honest, reproducible ranking, but deliberately not a claim about the
 * value of any person's work.
 */
const fillContributors = async (section) => {
  const repo = section.dataset.contributorsRepo;
  const list = section.querySelector('.contributors__list');
  if (!repo || !list) return;

  const limit = Number.parseInt(section.dataset.contributorsLimit ?? '', 10) || 8;
  const response = await fetch(`https://api.github.com/repos/${repo}/contributors?per_page=${limit}`, {
    headers: { Accept: 'application/vnd.github+json' },
  });
  if (!response.ok) return;

  const contributors = await response.json();
  if (!Array.isArray(contributors) || contributors.length === 0) return;

  const rows = contributors
    .filter((contributor) =>
      typeof contributor?.login === 'string' &&
      typeof contributor?.html_url === 'string' &&
      typeof contributor?.contributions === 'number',
    )
    .map((contributor, index) => {
      const row = document.createElement('li');
      row.className = 'contributor';

      const rank = document.createElement('span');
      rank.className = 'contributor__rank';
      rank.textContent = String(index + 1);

      const profile = document.createElement('a');
      profile.className = 'contributor__profile';
      profile.href = contributor.html_url;
      profile.target = '_blank';
      profile.rel = 'noopener';
      profile.textContent = `@${contributor.login}`;

      const count = document.createElement('span');
      count.className = 'contributor__count';
      count.textContent = `${contributor.contributions} ${contributor.contributions === 1 ? 'commit' : 'commits'}`;

      row.append(rank, profile, count);
      return row;
    });

  if (rows.length > 0) list.replaceChildren(...rows);
};

const contributorSection = document.querySelector('[data-contributors-repo]');

if (contributorSection) {
  // Like download metadata, this only asks GitHub once the visitor can see why
  // it is being asked for. The fallback link remains useful with scripts off.
  const ask = () => fillContributors(contributorSection).catch(() => {});

  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        io.disconnect();
        ask();
      },
      { rootMargin: '300px 0px' },
    );
    io.observe(contributorSection);
  } else {
    ask();
  }
}
