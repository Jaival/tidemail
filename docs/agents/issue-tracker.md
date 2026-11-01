# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues in `Jaival/tidemail`. Use the `gh` CLI for all operations.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`.
- **Read an issue**: `gh issue view <number> --comments`.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments`.
- **Comment on an issue**: `gh issue comment <number> --body "..."`.
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`.
- **Close**: `gh issue close <number> --comment "..."`.

The `origin` remote identifies this repo. Run `gh` from the repo checkout.

## Pull requests as a triage surface

**PRs as a request surface: no.**

When set to yes, PRs run through the same labels and states as issues. Use `gh pr` equivalents to read, comment on, label, or close them. GitHub shares a number space across issues and PRs, so resolve a bare `#42` by checking `gh pr view 42` and falling back to `gh issue view 42`.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.

## Wayfinding operations

- **Map**: one GitHub issue labelled `wayfinder:map`, holding Notes / Decisions-so-far / Fog.
- **Child ticket**: a GitHub sub-issue linked to the map where available. Otherwise add it to the map's task list and include `Part of #<map>` in the child body. Use `wayfinder:<type>` labels (`research`, `prototype`, `grilling`, `task`).
- **Blocking**: use GitHub's native issue dependencies where available. Otherwise include `Blocked by: #<n>, #<n>` in the child body.
- **Frontier**: choose the first open child in map order that has no open blockers and no assignee.
- **Claim**: assign the issue to the current user.
- **Resolve**: comment with the answer, close the issue, then add a context pointer to the map's Decisions-so-far.
