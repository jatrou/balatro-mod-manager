<script lang="ts">
  import BalatroPicker from "../components/BalatroPicker.svelte";
  import ReportIssue from "../components/ReportIssue.svelte";
  import { Menu, MenuItem } from "@tauri-apps/api/menu";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";
  import { goto } from "$app/navigation";
  import { fade } from "svelte/transition";

  interface AppInitData {
    version: string;
    existing_installation: string | null;
    security_acknowledged: boolean;
    lovely_installed: boolean;
    lovely_update_available: string | null;
    launch_mode: string;
  }

  let appVersion = $state("");
  let isLoading = $state(true);

  onMount(() => {
    const init = async () => {
      // Use batched init data call to reduce IPC overhead
      try {
        const initData = await invoke<AppInitData>("get_app_init_data");
        appVersion = initData.version;

        if (initData.existing_installation) {
          try {
            const nav = goto("/main/", { replaceState: true });
            await Promise.race([
              nav,
              new Promise((_, reject) =>
                setTimeout(() => reject(new Error("nav-timeout")), 4000),
              ),
            ]);
          } catch (_) {
            // Fallback: hard navigate if SPA router stalls
            window.location.replace("/main/");
          }
          return; // Don't show picker if navigating away
        }
      } catch (error) {
        console.error("Error loading init data:", error);
      }

      // Only show picker if no existing installation found
      isLoading = false;
    };

    init();
  });

  let contextHandler: ((e: MouseEvent) => void) | null = null;
  onMount(() => {
    contextHandler = async (e: MouseEvent) => {
      e.preventDefault();
      const menuItems = [
        await MenuItem.new({ text: "Copy", action: () => {} }),
        await MenuItem.new({ text: "Paste", action: () => {} }),
      ];
      const menu = await Menu.new({ items: menuItems });
      menu.popup();
    };
    window.addEventListener("contextmenu", contextHandler);
  });

  onDestroy(() => {
    if (contextHandler) {
      window.removeEventListener("contextmenu", contextHandler);
      contextHandler = null;
    }
  });
</script>

{#if isLoading}
  <div class="loading-screen" transition:fade={{ duration: 400 }}>
    <div class="spinner" role="status" aria-label="Loading"></div>
  </div>
{:else}
  <div class="app" transition:fade={{ duration: 300 }}>
    <h1>Welcome to Balatro Mod Manager</h1>
    <BalatroPicker />
    <ReportIssue />
    {#if appVersion}<div class="version-text">v{appVersion}</div>{/if}
  </div>
{/if}

<style>
  .app {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 40px 24px;
    box-sizing: border-box;
    overflow: auto;
  }

  h1 {
    color: var(--ui-text);
    font-size: 24px;
    font-weight: 600;
    margin: 0 0 32px;
  }

  .version-text {
    position: fixed;
    bottom: 14px;
    right: 20px;
    color: var(--ui-text-muted);
    font-size: 12px;
  }

  .loading-screen {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
