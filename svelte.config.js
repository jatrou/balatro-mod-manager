// Tauri doesn't have a Node.js server to do proper SSR
// so we will use adapter-static to prerender the app (SSG)
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter(),
    // Tauri serves prerendered routes with or without a trailing slash.
    // Keep their assets rooted so a hard reload of /main/ also works.
    paths: { relative: false },
    // Ensure SSG builds include non-linked routes like "/main".
    // In release, missing prerendered entries can cause client-side
    // navigation to hang or appear to do nothing inside Tauri.
    prerender: {
      entries: ["*", "/main"],
    },
    alias: {
      $lib: "./src/lib",
    },
  },
};

export default config;
