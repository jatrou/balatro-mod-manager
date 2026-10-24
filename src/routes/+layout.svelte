<script lang="ts">
  import MessageStack from "../components/MessageStack.svelte";
  import { updatePopupStore } from "../stores/modStore";
  import { darkMode } from "../stores/ui";
  import { onMount } from "svelte";
  import DragDropOverlay from "../components/DragDropOverlay.svelte";
  import DepPrompt from "../components/DepPrompt.svelte";
  import { Window } from "@tauri-apps/api/window";
  // Initialize popup manager subscriptions
  import { isPopupTransitioning } from "../stores/popupManager";

  import "../app.css";
  import UpdateAvailablePopup from "../components/UpdateAvailablePopup.svelte";
  import RecoveryScreen from "../components/RecoveryScreen.svelte";
  import SourceBrowser from "../components/SourceBrowser.svelte";
  import { updatePromptDisabled } from "../stores/update";
  import { invoke } from "@tauri-apps/api/core";
  import { fetchLatestAppVersion } from "$lib/releases";

  const { data, children } = $props();

  let detectedPlatform: string | null = null;

  function normalize(v: string): string {
    // strip leading 'v' and any pre-release metadata
    const t = v.trim().replace(/^v/i, "");
    // keep only digits and dots prefix
    const m = t.match(/^[0-9]+(?:\.[0-9]+)*/);
    return m ? m[0] : t;
  }

  function cmp(a: string, b: string): number {
    const as = normalize(a)
      .split(".")
      .map((n) => parseInt(n, 10));
    const bs = normalize(b)
      .split(".")
      .map((n) => parseInt(n, 10));
    const len = Math.max(as.length, bs.length);
    for (let i = 0; i < len; i++) {
      const ai = as[i] ?? 0;
      const bi = bs[i] ?? 0;
      if (ai < bi) return -1;
      if (ai > bi) return 1;
    }
    return 0;
  }

  async function checkForUpdate() {
    try {
      if ($updatePromptDisabled) return;
      const cur = await invoke<string>("get_app_version");
      const latest = await fetchLatestAppVersion();
      if (!latest) return;
      if (cmp(cur, latest) < 0) {
        updatePopupStore.set({
          visible: true,
          currentVersion: cur,
          latestVersion: latest,
          onClose: () => {},
          onDontShow: () => {
            updatePromptDisabled.set(true);
          },
        });
      }
    } catch (e) {
      console.warn("Update check failed:", e);
    }
  }

  async function detectPlatform() {
    if (typeof navigator === "undefined") return;

    const ua = navigator.userAgent.toLowerCase();

    try {
      const { platform } = await import("@tauri-apps/plugin-os");
      detectedPlatform = await platform();
    } catch (_) {
      if (ua.includes("linux")) detectedPlatform = "linux";
      else if (ua.includes("mac")) detectedPlatform = "macos";
      else if (ua.includes("windows")) detectedPlatform = "windows";
    }

    if (detectedPlatform) {
      document.documentElement.dataset.platform = detectedPlatform;
    }
  }

  /**
   * Make absolutely sure the window becomes visible. If anything else in
   * startup throws (deserialisation crash, plugin import error, network
   * hang etc.) the user must still get a window. Falls back to a single
   * deadline-based show.
   */
  let windowShown = false;
  async function forceShowWindow(reason: string) {
    if (windowShown) return;
    windowShown = true;
    try {
      const appWindow = Window.getCurrent();
      await appWindow.show();
      await appWindow.setFocus();
    } catch (e) {
      console.warn(`forceShowWindow (${reason}) failed:`, e);
    }
  }

  async function setupAppWindow() {
    try {
      // Wait for the next frame to ensure the page is painted, but never
      // longer than 1500ms. If raf never fires (rare WebView2 hang), the
      // deadline below shows the window anyway.
      await new Promise<void>((resolve) => {
        let resolved = false;
        const done = () => {
          if (resolved) return;
          resolved = true;
          resolve();
        };
        requestAnimationFrame(() => requestAnimationFrame(done));
        setTimeout(done, 1500);
      });
    } finally {
      await forceShowWindow("setupAppWindow");
    }
  }

  onMount(() => {
    const unsubscribeTheme = darkMode.subscribe((enabled) => {
      document.documentElement.dataset.theme = enabled ? "dark" : "light";
      document.documentElement.style.colorScheme = enabled ? "dark" : "light";
    });

    // Last-resort safety net: if nothing showed the window within 3 s of
    // mount, show it ourselves. Prevents the "process alive, window
    // hidden" failure mode reported on Windows.
    const safetyNet = window.setTimeout(() => {
      forceShowWindow("safety-net-timeout");
    }, 3000);

    const onUnhandledError = () => forceShowWindow("unhandled-error");
    window.addEventListener("error", onUnhandledError);
    window.addEventListener("unhandledrejection", onUnhandledError);

    detectPlatform();
    setupAppWindow();
    checkForUpdate();

    return () => {
      unsubscribeTheme();
      window.clearTimeout(safetyNet);
      window.removeEventListener("error", onUnhandledError);
      window.removeEventListener("unhandledrejection", onUnhandledError);
    };
  });
</script>

<MessageStack />
<DragDropOverlay />
<DepPrompt />
{#if $isPopupTransitioning}
  <div class="popup-transition-blocker"></div>
{/if}
<div class="layout-container app-body">
  {#key data.url}
    <div class="page-content">
      <svelte:boundary onerror={(e) => console.error("Page boundary:", e)}>
        {@render children()}
        {#snippet failed(error)}
          <RecoveryScreen {error} />
        {/snippet}
      </svelte:boundary>
    </div>
  {/key}
</div>

<UpdateAvailablePopup />
<SourceBrowser />

<style>
  .layout-container {
    position: fixed;
    inset: 0;
    overflow: hidden;
    background: var(--ui-bg);
  }

  .page-content {
    width: 100%;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .popup-transition-blocker {
    position: fixed;
    inset: 0;
    z-index: 2500;
    pointer-events: auto;
  }
</style>
