/**
 * Reader pane. Contract with the UI shell: the shell renders `<Reader threadId={…} />`
 * in its reader slot; `null` means nothing is selected. Phase 1 (renderer) owns this folder.
 */
export function Reader({ threadId }: { threadId: number | null }) {
  return <section className="reader">{threadId === null ? null : `Thread ${threadId}`}</section>;
}
