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
skip a question. Replace each `<describe the change>` it leaves in the body; a file that still
holds one fails CI.

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

Before the release deploys, publish each changed document's new text at
`scanopy.net/<page>/<effective>` (for example `scanopy.net/terms/2026-12-01`). The email links
there, and the page stays as the archived copy. Add a line to the live page linking to it until
`effective`, then put the new text on the live page.
