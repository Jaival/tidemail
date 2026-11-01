import type { View } from "./bindings";
import { ThreadList } from "./features/list/ThreadList";
import { Shell } from "./features/shell/Shell";
import { useSystemTheme } from "./features/theme/useSystemTheme";
import { reportFirstPaint } from "./lib/firstPaint";

const INBOX: View = { labelKey: "INBOX", accountId: null };

export function App() {
  useSystemTheme();
  return (
    <Shell>
      <ThreadList view={INBOX} onFirstPage={reportFirstPaint} />
    </Shell>
  );
}
