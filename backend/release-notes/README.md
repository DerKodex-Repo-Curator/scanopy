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

## Terms updates

A change to Scanopy's terms is one `terms/YYYY-MM-DD.md`, named for the day it takes effect. Scanopy
Cloud emails notice of it to every verified user who signed up before that day, once each. The
release that ships the file sends it, and a daily sweep retries anyone the relay refused.
Self-hosted servers never send it.

Run `make new-terms-update` to create the file. It asks for the effective date and the documents,
prefilling each, and checks the date against the rules below. Set `EFFECTIVE` or `DOCUMENTS` to
skip a question. It also drafts each changed page in the website checkout
(`src/lib/legal/<page>/<effective>.svelte`, copied from the version in force) and opens an issue in
`scanopy/website`, assigned to you, to publish them. It needs the website checkout at `../website`
beside this repo (or `WEBSITE_DIR`) and a logged-in `gh`, and stops before writing anything without
them. Replace each `<describe the change>` it leaves in the terms file; a file that still holds one
fails CI.

```markdown
---
effective: 2026-12-01
date: 2026-10-20
documents: terms, privacy
---

- Terms of Service, section 4: we now email you before a significant change takes effect.
```

- `effective`: the day the update takes effect. It must match the file name.
- `date`: the day you wrote it. It can't be in the future.
- `documents`: any of `terms`, `privacy`, `refund`, `security`. The email links each one.
- The body says what changed, in plain terms. It goes into the email as written.

The notice goes out when the release carrying the file deploys, so cut the release the day you
create the file. The prefilled effective date is 30 days out; a notice sent with less than 30 days
to go logs an error. One the server hasn't sent by `effective` still goes out for 30 more days.

CI fails when an update takes effect less than 30 days or more than 180 days after `date`, or is
dated before an update that takes effect sooner.

The email links each changed document to `scanopy.net/<page>/<effective>`, which the website builds
from that draft. Edit the drafts and push them to website `main` before cutting the release:
Prepare Release fails while any terms file's pages are missing there. Until `effective`, the live
page links to the new version; on that day the website's `legal-effective-rebuild.yml` rebuilds the
site, and the live page switches to it.
