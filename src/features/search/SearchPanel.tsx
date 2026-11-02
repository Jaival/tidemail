/**
 * Search slot. Contract with the UI shell: the shell renders `<SearchPanel … />` while search
 * is active (`/` focuses it), and `onOpenThread` selects a thread in the reader.
 * Phase 1 (search) owns this folder.
 */
export function SearchPanel({
  query,
  onOpenThread,
}: {
  query: string;
  onOpenThread: (threadId: number) => void;
}) {
  void onOpenThread;
  return <section className="search-results">{query}</section>;
}
