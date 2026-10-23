import { writable } from "svelte/store";

function createPersistentNumber(
  key: string,
  fallback: number,
  opts?: { min?: number; max?: number },
) {
  const isBrowser = typeof window !== "undefined";
  let initial = fallback;
  if (isBrowser) {
    try {
      const raw = localStorage.getItem(key);
      if (raw != null) {
        const n = Number(raw);
        if (!Number.isNaN(n)) initial = n;
      }
    } catch (err) {
      console.warn("Failed to read ui setting:", err);
    }
  }
  // Clamp to optional bounds
  if (typeof opts?.min === "number") initial = Math.max(opts.min, initial);
  if (typeof opts?.max === "number") initial = Math.min(opts.max, initial);

  const store = writable<number>(initial);
  if (isBrowser) {
    store.subscribe((val) => {
      try {
        let v = val;
        if (typeof opts?.min === "number") v = Math.max(opts.min, v);
        if (typeof opts?.max === "number") v = Math.min(opts.max, v);
        localStorage.setItem(key, String(v));
      } catch (err) {
        console.warn("Failed to persist ui setting:", err);
      }
    });
  }
  return store;
}

function createPersistentBoolean(key: string, fallback: boolean) {
  const isBrowser = typeof window !== "undefined";
  let initial = fallback;
  if (isBrowser) {
    try {
      const raw = localStorage.getItem(key);
      if (raw != null) {
        initial = raw === "true";
      }
    } catch (err) {
      console.warn("Failed to read ui setting:", err);
    }
  }

  const store = writable<boolean>(initial);
  if (isBrowser) {
    store.subscribe((val) => {
      try {
        localStorage.setItem(key, String(val));
      } catch (err) {
        console.warn("Failed to persist ui setting:", err);
      }
    });
  }
  return store;
}

// Controls how large mod cards render in the grid/search views
// Range: 0.75x – 1.4x
export const cardScale = createPersistentNumber("ui.cardScale", 1, {
  min: 0.75,
  max: 1.4,
});

// The interface now uses one consistent dark palette, including upgraded installs.
export const darkMode = writable(true);

export const showModPreviews = createPersistentBoolean("ui.showModPreviews", false);

export const MODS_PER_PAGE_OPTIONS = [10, 25, 50, 100, "all"] as const;
export type ModsPerPage = (typeof MODS_PER_PAGE_OPTIONS)[number];

function createModsPerPage() {
  let initial: ModsPerPage = 25;
  if (typeof window !== "undefined") {
    try {
      const raw = localStorage.getItem("ui.modsPerPage");
      const value = raw === "all" ? "all" : Number(raw);
      if (MODS_PER_PAGE_OPTIONS.includes(value as ModsPerPage)) {
        initial = value as ModsPerPage;
      }
    } catch {
      // Storage is optional; browsing still works with the default page size.
    }
  }
  const store = writable<ModsPerPage>(initial);
  if (typeof window !== "undefined") {
    store.subscribe((value) => {
      try {
        localStorage.setItem("ui.modsPerPage", String(value));
      } catch {
        // Keep the current session preference if storage is unavailable.
      }
    });
  }
  return store;
}

export const modsPerPage = createModsPerPage();
