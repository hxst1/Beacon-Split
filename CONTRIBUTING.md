# Contributing to Beacon Split

Contributions are genuinely welcome. This document is the short version of how
the project thinks, so that time you spend on it is time well spent.

## Before anything: the scope is narrow on purpose

Beacon is an agent-first workspace, not an IDE. [`docs/ROADMAP.md`][roadmap] has
a section titled **Explicitly out of scope** — a debugger, a plugin marketplace,
a full LSP, remote SSH, collaboration, and any attempt to reimplement Claude
Code. Those are not "not yet". They are the shape of the thing.

The test a feature has to pass is not "would this be useful". Nearly anything
is useful. It is: **does this reduce the friction of moving between projects
with an agent?** A file tree does. A debugger does not, however much someone
might want one.

If you are unsure, open an issue before writing code. Nobody enjoys declining a
finished pull request.

## How the project explains itself

Read these two before a substantial change:

- [`docs/ARCHITECTURE.md`][arch] — how the pieces fit, and the constraints that
  put them there.
- [`docs/DECISIONS.md`][decisions] — every choice worth recording, why it was
  made, and what it cost. Nearly fifty of them.

**A change that contradicts a decision is welcome.** Several of those decisions
will turn out to be wrong. What is asked is that you say which one, and why it
was wrong — not that you avoid the subject. A pull request that quietly reverses
a decision is much harder to review than one that argues with it.

Two rules are load-bearing and worth knowing before you start:

1. **`beacon-core` never depends on Tauri.** It is why sessions could move into
   a background daemon without touching the interface.
2. **The frontend never imports `@tauri-apps/api` outside `src/ipc/`.** When the
   transport changes, one directory changes.

## Getting set up

You need Node 20+, pnpm 10+, Rust 1.85+, and the Xcode command line tools.

```sh
pnpm install
pnpm app:dev
```

Before opening a pull request:

```sh
pnpm check    # typecheck, tests, rustfmt, clippy — the same thing CI runs
```

Nothing merges with that failing. Clippy runs with `-D warnings`, so a warning
is an error here.

### Build it before you trust it

`pnpm app:dev` working does not mean a release does. Two bugs in this project's
history existed only in the packaged build and were invisible in development —
one of them left a completely blank window. If your change touches the frontend,
the build, or startup, run `pnpm app:build` and open the result.

## Tests

Not coverage for its own sake. Tests are wanted where logic is easy to get wrong
and hard to see wrong by reading:

- Parsing anything — git status, `.env`, the daemon protocol, version strings.
- Path handling, especially anything that must not escape a project.
- Persistence and migrations.
- Session behaviour, against real PTYs rather than mocks.

For interface work, tests are wanted where the logic is pure — layout maths,
fuzzy matching, selector stability. Not for rendering.

A test that would have caught the bug you are fixing is worth more than three
that describe what the code already does. Several tests here exist because
something broke silently and the test is the reason it cannot do so twice; their
comments say which.

## Style

Match the code around you. Beyond that:

- **Comments explain why, not what.** If a line needs a comment to say what it
  does, the line is the problem.
- **Names are ordinary words.** No abbreviations that need decoding.
- **Small files.** If a file is doing two things, it is two files.
- **Errors carry context.** A message without the path or the value is close to
  useless when someone is stuck at midnight.
- **Never log secrets.** `.env` values, PTY input and output, and commit
  messages do not go into logs. Some functions say so in a comment; leave those
  comments there.

## Commits and pull requests

Commit messages explain the change and the reasoning, in prose. The first line
is a summary; the body says why, and what it cost. Look at the history for the
tone.

A pull request should say what it changes, why, and what you are unsure about.
"I could not decide between X and Y" is useful information, not a weakness.

## Reporting a bug

The useful ones say what you did, what happened, and what you expected. Beyond
that:

- The version, from Settings → About or the bell at the bottom right.
- Anything in the log. Run Beacon from a terminal to see it:
  `"/Applications/Beacon Split.app/Contents/MacOS/beacon-split"`.
- If the window is blank or a panel is empty, say so explicitly — that class of
  bug has had several distinct causes here and they look identical.

## Licensing

Beacon is [AGPL-3.0](LICENSE). By contributing you agree that your contribution
is licensed under it, and you grant the copyright holder the right to also
license your contribution under other terms.

That second part exists so the project can stay open while remaining
commercially viable for its author. Without it, the AGPL would be permanent for
everyone including them, which in practice means the project could not fund
itself. If you are not comfortable with that grant, say so in the pull request —
it is a reasonable position, and better raised than assumed.

[roadmap]: docs/ROADMAP.md
[arch]: docs/ARCHITECTURE.md
[decisions]: docs/DECISIONS.md
