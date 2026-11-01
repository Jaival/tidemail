# Domain Docs

How the engineering skills should consume this repo's domain documentation.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root, or
- **`CONTEXT-MAP.md`** at the repo root if it exists. It points to one `CONTEXT.md` per context; read the files relevant to the work.
- **`docs/adr/`** for decisions that touch the area. In multi-context repos, also check `src/<context>/docs/adr/`.

If these files do not exist, proceed without flagging their absence. The domain-modeling workflow creates them when terms or decisions are resolved.

## File structure

This repo uses a single-context layout:

```text
/
├── CONTEXT.md
├── docs/
│   └── adr/
└── src/
```

## Use the glossary's vocabulary

When naming a domain concept in issues, proposals, or tests, use the term defined in `CONTEXT.md`. If a needed concept is missing, note that gap for domain modeling rather than inventing a competing term.

## Flag ADR conflicts

If a proposal contradicts an existing ADR, name the ADR and explain why it may be worth revisiting.
