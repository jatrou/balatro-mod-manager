<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { sourcePage } from "$lib/opener";

  type BrowserStatus = {
    url: string;
    loading: boolean;
    download?: string | null;
  };
  let visible = $state(false);
  let url = $state("");
  let loading = $state(false);
  let download = $state("");
  let error = $state("");
  let body: HTMLDivElement | null = $state(null);
  let closing = $state(false);
  let generation = 0;
  let operations: Promise<unknown> = Promise.resolve();

  // Finish creation, navigation, resize and close in order, including a close
  // clicked while WebView2 is still creating its native child view.
  function enqueue<T>(operation: () => Promise<T>): Promise<T> {
    const next = operations.then(operation, operation);
    operations = next.catch(() => {});
    return next;
  }

  function bounds() {
    const rect = body?.getBoundingClientRect();
    if (!rect) throw new Error("The source browser has no content area.");
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  }

  async function open(url: string) {
    return invoke<BrowserStatus>("open_source_browser", {
      url,
      bounds: bounds(),
    });
  }

  async function action(action: "back" | "forward" | "reload") {
    try {
      await enqueue(async () => {
        if (action === "reload") {
          const status = await invoke<BrowserStatus | null>(
            "source_browser_status",
          );
          if (!status) {
            await open(url);
            return;
          }
        }
        await invoke("source_browser_action", { action });
      });
      error = "";
    } catch (cause) {
      error = String(cause);
    }
  }

  async function close() {
    if (closing) return;
    closing = true;
    ++generation;
    try {
      await enqueue(() => invoke("close_source_browser"));
      sourcePage.set(null);
      visible = false;
      error = "";
    } catch (cause) {
      error = `Could not close the source page: ${String(cause)}`;
    } finally {
      closing = false;
    }
  }

  onMount(() => {
    let disposed = false;
    let polling = false;
    let resizeFrame = 0;
    const resize = () => {
      cancelAnimationFrame(resizeFrame);
      resizeFrame = requestAnimationFrame(() => {
        if (!visible || disposed) return;
        void enqueue(() =>
          invoke("resize_source_browser", { bounds: bounds() }),
        ).catch((cause) => {
          error = String(cause);
        });
      });
    };
    const observer = new ResizeObserver(resize);
    // A full main-webview reload can leave its native child alive after all
    // Svelte state is lost. Retire that orphan before opening any new request.
    if (isTauri()) {
      void enqueue(async () => {
        try {
          await invoke("close_source_browser");
        } catch (cause) {
          if (disposed) return;
          visible = true;
          loading = false;
          error = `Could not restore the source browser: ${String(cause)}`;
          await tick();
          if (body) observer.observe(body);
        }
      });
    }
    const unsubscribe = sourcePage.subscribe((request) => {
      if (!request) return;
      const version = ++generation;
      visible = true;
      url = request.url;
      loading = true;
      download = "";
      error = "";
      void enqueue(async () => {
        await tick();
        if (disposed || version !== generation) return;
        if (body) observer.observe(body);
        try {
          const status = await open(request.url);
          if (!disposed && version === generation) {
            url = status.url;
            loading = status.loading;
            error = "";
          }
        } catch (cause) {
          if (!disposed && version === generation) {
            loading = false;
            error = `Could not open this page inside the app: ${String(cause)}`;
          }
        }
      });
    });

    window.addEventListener("resize", resize);
    // Native URL queries also cover pushState/replaceState navigation on sites
    // such as GitHub, without granting remote pages access to our event bridge.
    const interval = window.setInterval(async () => {
      if (!visible || disposed || polling || closing) return;
      const version = generation;
      polling = true;
      try {
        const status = await invoke<BrowserStatus | null>(
          "source_browser_status",
        );
        if (status && !disposed && version === generation) {
          url = status.url;
          loading = status.loading;
          download = status.download || "";
        }
      } catch {
        // Startup/teardown can race status queries; actions display errors.
      } finally {
        polling = false;
      }
    }, 500);

    return () => {
      disposed = true;
      ++generation;
      unsubscribe();
      observer.disconnect();
      window.removeEventListener("resize", resize);
      cancelAnimationFrame(resizeFrame);
      window.clearInterval(interval);
      void enqueue(() => invoke("close_source_browser")).catch(() => {});
    };
  });
</script>

{#if visible}
  <div
    class="source-browser"
    role="dialog"
    aria-modal="true"
    aria-label="Source page browser"
    tabindex="-1"
  >
    <header class="source-toolbar">
      <button onclick={() => action("back")} title="Back" aria-label="Back"
        >←</button
      >
      <button
        onclick={() => action("forward")}
        title="Forward"
        aria-label="Forward">→</button
      >
      <button
        onclick={() => action("reload")}
        title="Reload"
        aria-label="Reload">↻</button
      >
      <div class="source-location" title={url}>
        <span class="source-label"
          >{download || (loading ? "Loading source…" : "Source page")}</span
        >
        <input
          aria-label="Source page URL"
          value={url}
          readonly
          spellcheck="false"
        />
      </div>
      <button class="close-source" onclick={close} disabled={closing}>
        {closing ? "Closing…" : "Close source"}
      </button>
    </header>
    {#if error}<p class="source-error" role="alert">{error}</p>{/if}
    <div class="source-content" bind:this={body}>
      <p>
        {error
          ? "Use Reload to retry, or close this page to return to your mods."
          : "Opening source page…"}
      </p>
    </div>
  </div>
{/if}

<style>
  .source-browser {
    position: fixed;
    inset: 0;
    z-index: 10000;
    display: flex;
    flex-direction: column;
    background: var(--ui-bg, #14171d);
    color: var(--ui-text, #f4f4f4);
  }
  .source-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 72px;
    flex-shrink: 0;
    padding: 10px 16px;
    box-sizing: border-box;
    border-bottom: 1px solid var(--ui-border, #444);
  }
  button {
    min-width: 36px;
    height: 36px;
    padding: 0 10px;
    border: 1px solid var(--ui-border, #555);
    border-radius: 6px;
    background: var(--ui-surface, #292d36);
    color: inherit;
    cursor: pointer;
  }
  button:hover {
    filter: brightness(1.2);
  }
  button:focus-visible {
    outline: 2px solid #76b7ff;
    outline-offset: 2px;
  }
  .close-source {
    white-space: nowrap;
  }
  .source-location {
    flex: 1;
    min-width: 0;
  }
  .source-label {
    display: block;
    font-size: 11px;
    opacity: 0.75;
    margin-bottom: 2px;
  }
  input {
    width: 100%;
    color: inherit;
    background: transparent;
    border: 0;
    font-size: 13px;
  }
  .source-content {
    flex: 1;
    min-height: 1px;
    background: #fff;
    color: #222;
  }
  .source-content p {
    padding: 24px;
  }
  .source-error {
    margin: 0;
    padding: 10px 16px;
    background: #6b2323;
    color: #fff;
  }
</style>
