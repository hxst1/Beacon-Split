# Publishing Beacon

What has to happen once, and what happens every release.

## The part only you can do

Updates are signed. An installed Beacon will only accept an update signed with
the key it already trusts, which is what stops a download from somewhere else
replacing the application on somebody's machine.

Generate the key yourself:

```sh
pnpm tauri signer generate -w ~/.beacon-updater.key
```

It prints a **public** key and writes a **private** one.

- The public key goes in `src-tauri/tauri.conf.json`, under
  `plugins.updater.pubkey`. It is meant to be committed.
- The private key never leaves your machine except into a GitHub secret. Do not
  paste it into a chat, a commit, or an issue. Anything holding it can publish
  something every existing Beacon will install without question.

Add to the repository's secrets, under Settings → Secrets and variables →
Actions:

| Secret | What |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | The contents of `~/.beacon-updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The password you chose, or empty |

Keep a copy of the private key somewhere you would keep a password. Losing it
means nobody who already installed Beacon can be updated again — they would
have to download a new copy by hand, once, to start trusting a new key.

## Then, in `tauri.conf.json`

```jsonc
"plugins": {
  "updater": {
    "pubkey": "<the public key from above>",
    "endpoints": [
      "https://github.com/hxst1/Beacon-Split/releases/latest/download/latest.json"
    ]
  }
}
```

Until that is filled in, Beacon simply never finds an update — the check fails
quietly, which is right for a copy somebody built themselves. They update by
pulling.

## Every release

1. Write what changed in `release-notes.json`, newest entry first. This is what
   the application shows on first launch of a new version, and a test refuses to
   pass if the running version has no entry — so the notes cannot be forgotten.
2. Move the version in `package.json`, `Cargo.toml` (workspace) and
   `src-tauri/tauri.conf.json`. All three, and to the same number.
3. Commit, tag, push:

```sh
git tag v0.2.0
git push origin v0.2.0
```

The workflow builds for Apple Silicon and Intel, runs the full check, signs the
artefacts, and opens a **draft** release. Look at it, then publish it. A draft
by default because a release is the one build nobody gets to try again.

Publishing the release makes `latest.json` reachable, and every running Beacon
finds it the next time it starts.

## What a user sees

- On start, if their version is newer than the last one they were shown, the
  notes open. The bell at the bottom right silences that; it does not hide it —
  the bell keeps its mark, and clicking it shows everything.
- If a newer version exists, the status bar offers it. Pressing it downloads,
  installs and restarts. There is no second confirmation: the asking already
  happened, and an application that installs an update and then waits is one you
  have to remember to finish.

## Gatekeeper, still

Signing an *update* is not the same as signing the *application*. Without an
Apple Developer ID, a downloaded release is still refused by macOS the first
time with a misleading "is damaged" message. See `DISTRIBUTING.md`. The updater
is unaffected once Beacon is installed and running — it replaces itself in
place.
