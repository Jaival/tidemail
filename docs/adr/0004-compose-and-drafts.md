# 0004: Compose, reply, forward and drafts

Date: 2026-11-24. Status: accepted. Decided in the Phase 3 grilling session.

## Context

SPEC feature 8 left the editor, Gmail drafts sync and the offline send queue
open. The OAuth scope `gmail.modify` already allows `drafts.*` and
`messages.send`, so no new sign-in is needed. `gpui-component` 0.9 has two
multi-line inputs: `Textarea` (no text decorations) and `Editor` (code editor
mode with diagnostics squiggles, code actions, completion provider,
`soft_wrap`, `line_number(false)`).

## Decision

### Editor and format

- Compose uses `gpui-component`'s **`Editor` set up for prose**: soft wrap on,
  line numbers off, no gutter. No WebView for compose.
- You write plain text with a **Markdown subset**: bold, italic, links, lists,
  quotes, inline code. Markers are shown dimmed.
- On send, `pulldown-cmark` renders the subset to HTML; raw HTML in the source
  is escaped. The message is `multipart/alternative` (text + HTML), sent as raw
  RFC 822.

### Where and how

- Reply / reply all / forward open **inline at the bottom of the reader**. A new
  message takes the reader pane (full width in Command). Esc saves the draft and
  closes the editor.
- Keys: `c` compose, `r` reply, `a` reply all, `f` forward, Ctrl+Enter send,
  Esc save + close, Ctrl+Shift+D discard (with undo), F7 next misspelling. All
  listed in the palette.
- Header: To; Cc / Bcc behind a "Cc Bcc" toggle (open when they hold values).
  Subject gets `Re:` / `Fwd:` (never doubled), editable.
- Reply all = From, To and Cc of the last message, minus the account's own
  addresses. Reply uses `Reply-To` when present.
- Replies quote the original Gmail-style (`gmail_quote` blockquote, "On <date>,
  <name> wrote:"), text on top; the quote is collapsed behind "…" in the editor.
  Forward includes the full original with its headers and attachments.
- Threading headers on reply/forward: `In-Reply-To`, `References`, and the Gmail
  `threadId`.
- From: the account's main address only (send-as aliases later).
- Signature: one per account, set in Tidemail, same Markdown subset. Importing
  the Gmail signature is later.

### Drafts

- Own tables: `drafts` (account, Gmail draft id, Gmail message id last saved,
  reply-to message, to/cc/bcc, subject, Markdown source, updated_at, sync state)
  and `draft_attachments`. Attachment files are **copied** to
  `%LOCALAPPDATA%\Tidemail\outbox\<draft>\`.
- Saved locally at once; saved to Gmail (`drafts.create` / `drafts.update`)
  after 3 s without typing. **Two-way:** Gmail drafts come in as editable
  drafts. Last edit wins, no merge.
- A Gmail draft whose message id still matches the one Tidemail saved uses the
  local Markdown source. Otherwise its HTML is converted to Markdown/plain text
  (lossy) with the note "Edited outside Tidemail; formatting simplified."
- Drafts view lists the `drafts` table. A draft reply shows as a "Draft" block
  at the end of its thread in the reader; the list row gets a "Draft" marker.
- Discard deletes the local draft, its Gmail draft (`drafts.delete`) and its
  copied files, after the 5 s undo.

### Sending

- Send goes through a durable queue with a **10 s "Undo send"** toast (actions
  keep 5 s). Offline mail waits in Outbox and retries with backoff like the
  action queue. Drafts synced to Gmail are sent with `drafts.send`.
- The sent message is inserted locally at once with `SENT`, in its thread; the
  reader tags it "Sending…" / "Queued (offline)" until Gmail confirms, then the
  Gmail ids replace the local ones.
- Failure: the status area shows it with Retry; the mail goes back to Drafts.
- Quit inside the undo window: the send goes out on the next launch; the Quit
  menu says "1 message waiting to send". Hide to tray is not quit.
- Sign-out warns "N drafts and M unsent messages will be deleted".

### Attachments

- File picker and drag-and-drop, 25 MB total (Gmail limit).

### Spell check

- Engine: Windows **`ISpellChecker`** (`windows` crate, COM). Uses installed
  Windows languages and the Windows user dictionary; nothing bundled.
- Language: Windows display language by default; Ctrl+K "Spell check language…"
  lists installed languages, per draft; the last choice becomes the default. No
  auto-detect.
- Checks only your own text: skips the quoted reply, URLs, email addresses,
  inline code / code blocks, ALL-CAPS words and words with digits.
- Runs on a background thread 300 ms after typing stops, only on changed
  paragraphs; the word under the caret stays unmarked.
- Squiggles are wavy underlines from Tidemail's own `InputHighlighter` (slice 0
  result), not `Editor` diagnostics. Menu (right-click via
  `Editor::context_menu`, Ctrl+.): up to 5 suggestions, "Add to dictionary",
  "Ignore in this draft". No autocorrect.

### Autocomplete

- To / Cc / Bcc complete from local mail only (from / to / cc), ranked by how
  often and how recently you wrote to them. No People API (new scope).

### Budgets

- `bench-all` adds: open compose to caret paint p95 <= 50 ms; keystroke to
  paint in a 5,000-word draft p95 <= 16 ms.

### Build slices (one commit each, each with a test)

0. **Editor spike** (throwaway example, before slice 1): prose-mode `Editor`
   (auto close, smart indent, line numbers, folding off; soft wrap on), Inter,
   5,000 words, fake diagnostics. Pass = keystroke to paint p95 <= 16 ms,
   squiggles right on wrapped lines, IME / emoji input and Ctrl+arrow word moves
   work (checked by screenshot). Fail = fall back to `Textarea` without spell
   check, or patch the vendored `gpui-component`; decide before slice 1.
1. New message, plain text, send queue, undo send.
2. Reply, reply all, forward, threading headers.
3. Local drafts and autosave.
4. Gmail drafts two-way sync.
5. Markdown to HTML and signatures.
6. Spell check.
7. Attachments.
8. Autocomplete.

## Consequences

- `Editor` mode is a code editor; slice 0 showed it works for prose (below).

## Slice 0 result (2026-11-24): pass

Spike: `crates/app/examples/editor_spike.rs` (`--bench` prints timings),
screenshots: `scripts/ui-shots/spike.ps1`.

- **Setup that works:** `EditorState::new(..).soft_wrap(true).line_number(false)
  .folding(false).indent_guides(false).auto_close(false).smart_indent(false)`,
  `Editor::new(..).font_family("Inter")`, plus our own highlighter through
  `set_highlighter_factory`.
- **Speed:** 5,000 words, caret mid-text in a wrapped paragraph, 300 keys
  (letters, space, backspace), squiggles on: keystroke to paint p50 ~1.0 ms,
  p95 1.2-1.4 ms, max ~5 ms (3 runs, release). Budget 16 ms.
- **Inter** draws well; soft wrap breaks between words.
- **Squiggles: not diagnostics.** Without the `tree-sitter` / `syntect`
  features `gpui-component` installs no highlighter, and diagnostics only draw
  when one exists. With one installed, diagnostics draw in the theme's
  `highlight_theme` status colour, which our theme leaves unset (invisible),
  and every edit clears them (LSP republish model). A custom `InputHighlighter`
  returning wavy `UnderlineStyle` runs draws them correctly, also on wrapped
  lines and after edits above them. The same highlighter will dim Markdown
  markers (slice 5), so no tree-sitter dependency.
- **Input:** emoji (incl. skin tone), combining accent, CJK through
  `SendInput` Unicode (the path the emoji panel commits through) all render.
  Ctrl+Left / Ctrl+Shift+Left select Latin words; CJK moves per character.
  `( " [ {` get no closer.
- **Enter keeps the previous line's indent** even with smart indent off.
  Accepted: it continues Markdown list items.
- **Not checked:** IME composition (no IME on this PC; en-IN only). Check by
  hand with a Japanese or Chinese IME before release.
- New dependency: `windows` (COM) next to `windows-sys`; `pulldown-cmark`.
- Later: send-as aliases, Gmail signature import, spell-check auto-detect.
