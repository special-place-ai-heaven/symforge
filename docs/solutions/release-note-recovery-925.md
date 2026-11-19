# Release-note recovery for PR #925 (2026-11-19)

PR #925 was merged with a plain merge commit whose subject was
`Merge pull request #925 from …` and whose body was empty. release-please
parses neither, so the PR's two fixes reached `main` after the 11.22.4 release
PR had been cut and never appeared in any release:

- `fix(update)` repair every harness on each update run (5373768a)
- `fix(update)` leave disabled and remote Grok and OMP entries as the user set them (323d14d2)

The Release workflow's `prove_release_owed.py` step caught it on the next
push to `main` and went red.

## Recovery

The commit that adds this file carries both fixes as conventional
paragraphs (subject plus one body paragraph), so release-please lists them in
the next release. This is the recovery the repo's `CLAUDE.md` describes for a
parsed-away commit.

## Prevention

Merge PRs with the squash command in `CLAUDE.md` ("Merging PRs"), with an
explicit conventional subject and a short body. A plain `--merge` with the
default subject hides every commit in the PR from the changelog.
