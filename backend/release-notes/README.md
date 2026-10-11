# Release notes

One `vX.Y.Z.md` per release, written by `prepare_release.yml` and reviewed in the release PR. The
server embeds these files: they fill the in-app release notes modal and the release email.

```markdown
---
version: 0.18.0
date: 2026-10-11
title: 'Short headline for the release'
email: true
---

Markdown body. Raw HTML is dropped when rendered.
```

`email: true` sends the release email to every user when a server starts on this version. That
email covers every release since the last one sent. `email: false` shows the notes in the app
only.

Edit a file here before the release is tagged. A server shows the copy it was built with, so a
change after the tag reaches servers only with the next release.
