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

  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        e.target.classList.add('is-in');
        io.unobserve(e.target);
      }
    },
    { rootMargin: '0px 0px -12% 0px', threshold: 0.06 },
  );
  targets.forEach((el) => io.observe(el));
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
