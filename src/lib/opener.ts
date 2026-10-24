import { isTauri } from "@tauri-apps/api/core";
import { writable } from "svelte/store";
import { addMessage } from "$lib/stores";

export const sourcePage = writable<{ url: string } | null>(null);

/** Source pages belong to the app's isolated native browser, never the OS browser. */
export async function openExternal(value: string): Promise<void> {
  if (!value?.trim()) return;

  let url: URL;
  try {
    url = new URL(value.trim());
    if (
      !["http:", "https:"].includes(url.protocol) ||
      !url.hostname ||
      url.username ||
      url.password
    ) {
      throw new Error("Unsupported URL");
    }
  } catch {
    addMessage("Only valid HTTP or HTTPS source links can be opened.", "error");
    return;
  }

  if (!isTauri()) {
    addMessage(
      "Open the desktop app to browse source pages inside the app.",
      "error",
    );
    return;
  }

  sourcePage.set({ url: url.href });
}
