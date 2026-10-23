<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import LaunchAlertBox from "./LaunchAlertBox.svelte";
  import { addMessage } from "../lib/stores";
  import { lovelyPopupStore } from "../stores/modStore";
  import { isLinuxPlatform } from "../lib/platform";
  import { onMount } from "svelte";
  import { Check } from "lucide-svelte";

  let showAlert = false;
  let isLinux = false;
  let isLaunching = false;
  let launchViaSteam = false;
  let launchCheckTimer: ReturnType<typeof setInterval> | null = null;
  let launchTimeoutTimer: ReturnType<typeof setTimeout> | null = null;

  // Launch mode state
  let launchMode: "modded" | "vanilla" = "modded";
  let dropdownOpen = false;

  onMount(async () => {
    try {
      isLinux = await isLinuxPlatform();
    } catch (_) {
      isLinux = false;
    }
    // Load saved launch mode
    try {
      const savedMode = await invoke<string>("get_launch_mode");
      if (savedMode === "modded" || savedMode === "vanilla") {
        launchMode = savedMode;
      }
    } catch (_) {
      // Default to modded if we can't load
      launchMode = "modded";
    }
  });

  async function setMode(mode: "modded" | "vanilla") {
    const previousMode = launchMode;
    // Optimistic update - instant UI feedback
    launchMode = mode;
    dropdownOpen = false;

    // Save in background, revert on error
    try {
      await invoke("set_launch_mode", { mode });
    } catch (err) {
      launchMode = previousMode;
      addMessage(`Failed to set launch mode: ${err}`, "error");
    }
  }

  function handleDropdownToggle(event: MouseEvent) {
    event.stopPropagation();
    dropdownOpen = !dropdownOpen;
  }

  function handleClickOutside(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (!target.closest(".launch-wrapper")) {
      dropdownOpen = false;
    }
  }

  async function doLaunch() {
    let usesSteam = false;
    if (isLinux) {
      const is_balatro_running: boolean = await invoke("check_balatro_running");
      if (is_balatro_running) {
        addMessage("Balatro is already running", "error");
        return { launched: false, usesSteam };
      }
    }

    const path = await invoke("get_balatro_path");
    if (path && path.toString().includes("Steam")) {
      usesSteam = true;
      const is_balatro_running: boolean = await invoke("check_balatro_running");
      if (is_balatro_running) {
        addMessage("Balatro is already running", "error");
        return { launched: false, usesSteam };
      }
      if (!isLinux) {
        const is_steam_running: boolean = await invoke("check_steam_running");
        if (!is_steam_running) {
          showAlert = true;
          return { launched: false, usesSteam };
        }
      }
    }

    await invoke("launch_balatro");
    return { launched: true, usesSteam };
  }

  const handleLaunch = async () => {
    if (isLaunching) return;
    isLaunching = true;
    try {
      isLinux = await isLinuxPlatform();
    } catch (_) {
      isLinux = false;
    }
    // Only warn about Lovely if launching in modded mode
    if (launchMode === "modded" && !isLinux) {
      try {
        const present = await invoke<boolean>("is_lovely_installed");
        if (!present) {
          lovelyPopupStore.set({
            visible: true,
            source: "launch",
            onLaunchAnyway: async () => {
              await doLaunch();
            },
          });
          isLaunching = false;
          return;
        }
      } catch (_) {
        // ignore detection errors, proceed
      }
    }
    let launched = false;
    let usesSteam = false;
    try {
      const res = await doLaunch();
      launched = res.launched;
      usesSteam = res.usesSteam;
    } catch (_) {
      launched = false;
    }
    if (!launched) {
      isLaunching = false;
      return;
    }
    launchViaSteam = usesSteam;

    if (launchCheckTimer) clearInterval(launchCheckTimer);
    if (launchTimeoutTimer) clearTimeout(launchTimeoutTimer);

    if (isLinux) {
      launchCheckTimer = setInterval(async () => {
        try {
          const running: boolean = await invoke("check_balatro_running");
          if (running) {
            if (launchCheckTimer) clearInterval(launchCheckTimer);
            launchCheckTimer = null;
            if (launchTimeoutTimer) clearTimeout(launchTimeoutTimer);
            launchTimeoutTimer = null;
            isLaunching = false;
            return;
          }
        } catch (_) {
          // ignore polling errors
        }
      }, 500);

      launchTimeoutTimer = setTimeout(async () => {
        if (launchCheckTimer) {
          clearInterval(launchCheckTimer);
          launchCheckTimer = null;
        }
        try {
          const running: boolean = await invoke("check_balatro_running");
          if (running) {
            isLaunching = false;
            return;
          }
        } catch (_) {
          // ignore polling errors
        }
        isLaunching = false;
        addMessage("Launch timed out. Try again.", "warning");
      }, 12000);
      return;
    }

    launchCheckTimer = setInterval(async () => {
      try {
        const running: boolean = await invoke("check_balatro_running");
        if (running) {
          if (launchCheckTimer) clearInterval(launchCheckTimer);
          launchCheckTimer = null;
          if (launchTimeoutTimer) clearTimeout(launchTimeoutTimer);
          launchTimeoutTimer = null;
          isLaunching = false;
          return;
        }
      } catch (_) {
        // ignore polling errors
      }
    }, 500);

    launchTimeoutTimer = setTimeout(async () => {
      if (launchCheckTimer) {
        clearInterval(launchCheckTimer);
        launchCheckTimer = null;
      }
      try {
        const running: boolean = await invoke("check_balatro_running");
        if (running) {
          isLaunching = false;
          return;
        }
      } catch (_) {
        // ignore polling errors
      }
      if (launchViaSteam) {
        try {
          const steamRunning: boolean = await invoke("check_steam_running");
          if (steamRunning) {
            isLaunching = false;
            return;
          }
        } catch (_) {
          // ignore polling errors
        }
      }
      isLaunching = false;
      addMessage("Launch timed out. Try again.", "warning");
    }, 12000);
  };

  onMount(() => {
    // Add click outside listener
    document.addEventListener("click", handleClickOutside);
    return () => {
      if (launchCheckTimer) clearInterval(launchCheckTimer);
      if (launchTimeoutTimer) clearTimeout(launchTimeoutTimer);
      document.removeEventListener("click", handleClickOutside);
    };
  });

  const handleAlertClose = () => {
    showAlert = false;
  };
</script>

<div class="launch-container">
  <div class="launch-wrapper">
    <button
      class="launch-button launch-main"
      onclick={handleLaunch}
      disabled={isLaunching}
    >
      {launchMode === "modded" ? "Launch" : "Launch Vanilla"}
    </button><button
      class="launch-button launch-dropdown-toggle"
      onclick={handleDropdownToggle}
      disabled={isLaunching}
      aria-label="Select launch mode"
    >
      <span class="dropdown-arrow"></span>
    </button>

    {#if dropdownOpen}
      <div class="dropdown-menu">
        <button
          class="dropdown-item"
          class:active={launchMode === "modded"}
          onclick={() => setMode("modded")}
        >
          <span class="check"
            >{#if launchMode === "modded"}<Check
                size={16}
                strokeWidth={3}
              />{/if}</span
          >
          Modded
        </button>
        <button
          class="dropdown-item"
          class:active={launchMode === "vanilla"}
          onclick={() => setMode("vanilla")}
        >
          <span class="check"
            >{#if launchMode === "vanilla"}<Check
                size={16}
                strokeWidth={3}
              />{/if}</span
          >
          Vanilla
        </button>
      </div>
    {/if}
  </div>
</div>

<LaunchAlertBox show={showAlert} onClose={handleAlertClose} />

<style>
  .launch-container {
    position: relative;
  }
  .launch-wrapper {
    position: relative;
    display: inline-flex;
  }
  .launch-button {
    background: var(--ui-launch-bg);
    color: var(--ui-launch-text);
    border: 1px solid var(--ui-launch-outline);
    min-height: 33px;
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    transition: background 0.15s ease;
  }
  .launch-main {
    padding: 7px 16px;
    border-radius: 5px 0 0 5px;
  }
  .launch-dropdown-toggle {
    padding: 7px 10px;
    border-radius: 0 5px 5px 0;
    border-left: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dropdown-arrow {
    display: block;
    width: 0;
    height: 0;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
    border-top: 4px solid currentColor;
  }
  .launch-button:hover:not(:disabled) {
    background: var(--ui-launch-hover);
  }
  .launch-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .dropdown-menu {
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 6px;
    padding: 4px;
    background: var(--ui-surface);
    border: 1px solid var(--ui-border);
    border-radius: 6px;
    box-shadow: 0 5px 18px rgba(0, 0, 0, 0.25);
    z-index: 1500;
    min-width: 145px;
  }
  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 7px 9px;
    background: transparent;
    border: 0;
    border-radius: 3px;
    color: var(--ui-text);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
  }
  .dropdown-item:hover {
    background: var(--ui-glass);
  }
  .dropdown-item.active {
    background: var(--ui-mod-chip-active-bg);
  }
  .dropdown-item .check {
    width: 16px;
    height: 16px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--ui-accent);
  }
</style>
