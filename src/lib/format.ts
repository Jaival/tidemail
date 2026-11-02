const time = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const day = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });
const dayYear = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" });

/** List date: time today, "Mar 4" this year, "Mar 4, 2024" before. */
export function formatDate(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  if (d.getFullYear() !== now.getFullYear()) return dayYear.format(d);
  if (d.getMonth() === now.getMonth() && d.getDate() === now.getDate()) return time.format(d);
  return day.format(d);
}

/** "12 s", "5 min", "3 h". */
export function formatAge(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  return `${Math.floor(s / 3600)} h`;
}
