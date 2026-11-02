/** Turns a tauri-specta result into its data, or throws the command's error string. */
export function unwrap<T>(result: { status: "ok"; data: T } | { status: "error"; error: string }): T {
  if (result.status === "error") throw new Error(result.error);
  return result.data;
}
