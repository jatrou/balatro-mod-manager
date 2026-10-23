<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as choosePath } from "@tauri-apps/plugin-dialog";
  import type { AwesomeEntry } from "$lib/awesomeCatalog";
  import type { AwesomeInstallSource } from "$lib/awesomeInstall";
  import { openExternal } from "$lib/opener";

  interface PreparedInstall {
    token: string;
    destination: string;
    warnings: { code: string; message: string; blocking: boolean }[];
    mods: { id: string; name: string; version?: string | null }[];
  }

  let {
    entry,
    source,
    folderName,
    onClose,
    onInstalled,
  }: {
    entry: AwesomeEntry;
    source: AwesomeInstallSource;
    folderName: string;
    onClose: () => void;
    onInstalled: (path: string) => void;
  } = $props();

  let dialog: HTMLDialogElement;
  let localPath = $state("");
  let prepared = $state<PreparedInstall | null>(null);
  let busy = $state(false);
  let error = $state("");
  let acknowledged = $state(false);
  let importing = $state(false);
  let preparedFromLocal = $state(false);
  let alive = true;
  let blocked = $derived(
    prepared?.warnings.some((warning) => warning.blocking) ?? false,
  );
  let upstreamWarnings = $derived(entry.badges.map((badge) => badge.label));
  let hasWarnings = $derived(
    !!prepared?.warnings.length || !!upstreamWarnings.length,
  );

  async function discard(): Promise<void> {
    const previous = prepared;
    prepared = null;
    if (previous)
      await invoke("cancel_awesome_install", { token: previous.token });
  }

  async function prepare(fromLocal = false): Promise<void> {
    if (busy || source.kind === "unsupported") return;
    busy = true;
    error = "";
    acknowledged = false;
    try {
      await discard();
      const result = await invoke<PreparedInstall>("prepare_awesome_install", {
        title: entry.title,
        folderName,
        loader: entry.loader,
        url: !fromLocal && source.kind === "download" ? source.url : null,
        localPath: fromLocal
          ? localPath.trim().replace(/^"(.*)"$/, "$1")
          : null,
      });
      if (alive) {
        prepared = result;
        preparedFromLocal = fromLocal;
      } else await invoke("cancel_awesome_install", { token: result.token });
    } catch (cause) {
      error = String(cause);
      importing = true;
    } finally {
      busy = false;
    }
  }

  async function browse(directory: boolean): Promise<void> {
    try {
      const selected = await choosePath({
        directory,
        multiple: false,
        title: directory
          ? "Select the downloaded mod folder"
          : "Select the downloaded mod",
        ...(!directory
          ? {
              filters: [
                { name: "Mod files", extensions: ["lua", "zip", "tar", "gz"] },
              ],
            }
          : {}),
      });
      if (typeof selected === "string") localPath = selected;
    } catch (cause) {
      error = String(cause);
    }
  }

  async function finish(): Promise<void> {
    if (!prepared || busy || blocked || (hasWarnings && !acknowledged)) return;
    busy = true;
    error = "";
    try {
      const path = await invoke<string>("finish_awesome_install", {
        token: prepared.token,
        acceptWarnings: acknowledged,
      });
      prepared = null;
      onInstalled(path);
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function close(): Promise<void> {
    if (busy) return;
    try {
      await discard();
      onClose();
    } catch (cause) {
      error = String(cause);
    }
  }

  async function openSource(): Promise<void> {
    try {
      await openExternal(source.discordUrl || entry.sourceUrl);
    } catch (cause) {
      error = String(cause);
    }
  }

  onMount(() => {
    dialog.showModal();
    if (source.kind === "download") void prepare();
    else {
      importing = true;
      if (source.discordUrl) void openSource();
    }
    return () => {
      alive = false;
      if (prepared)
        void invoke("cancel_awesome_install", { token: prepared.token }).catch(
          () => {},
        );
    };
  });
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="awesome-install-title"
  oncancel={(event) => {
    event.preventDefault();
    void close();
  }}
>
  <h2 id="awesome-install-title">Install {entry.title}</h2>
  {#if source.kind === "unsupported"}
    <p>{source.reason}</p>
  {:else}
    {#if !prepared && importing}
      <p>
        {source.kind === "local"
          ? source.reason
          : "You can also import a copy downloaded from the mod's source."}
        Download the mod, then enter its file or folder location below.
      </p>
      <button type="button" onclick={openSource}
        >{source.discordUrl
          ? "Open Discord download"
          : "Open mod source"}</button
      >
      <p class="source-url">{source.discordUrl || entry.sourceUrl}</p>
      <label for="awesome-download-path"
        >Downloaded file or folder location</label
      >
      <input
        id="awesome-download-path"
        bind:value={localPath}
        disabled={busy}
        placeholder="Paste a path, or browse…"
      />
      <p class="hint">
        Choose a .lua file, ZIP, TAR, TAR.GZ, or an extracted mod folder. Your
        downloaded files stay in place.
      </p>
      <div class="row">
        <button type="button" disabled={busy} onclick={() => browse(false)}
          >Browse files</button
        >
        <button type="button" disabled={busy} onclick={() => browse(true)}
          >Browse folders</button
        >
        <button
          type="button"
          class="primary"
          disabled={busy || !localPath.trim()}
          onclick={() => prepare(true)}>Check download</button
        >
      </div>
    {/if}
    {#if busy}
      <p role="status">
        {prepared
          ? "Installing…"
          : "Preparing download and checking conflicts…"}
      </p>
    {/if}
    {#if prepared}
      <p>Ready to install into:</p>
      <p class="destination">{prepared.destination}</p>
      {#if prepared.mods.length}
        <p>
          Detected: {prepared.mods
            .map(
              (mod) => `${mod.name}${mod.version ? ` (${mod.version})` : ""}`,
            )
            .join(", ")}
        </p>
      {/if}
      {#if hasWarnings}
        <div class="warnings" role="alert">
          <h3>
            {blocked ? "Installation blocked" : "Review compatibility warnings"}
          </h3>
          <ul>
            {#each prepared.warnings as warning}
              <li class:blocking={warning.blocking}>{warning.message}</li>
            {/each}
            {#each upstreamWarnings as warning}
              <li>Awesome Balatro lists: {warning}</li>
            {/each}
          </ul>
        </div>
      {/if}
      <p class="hint">
        Checks cover declared conflicts, dependencies, and duplicate mods.
        Undeclared gameplay conflicts may still occur.
      </p>
      {#if hasWarnings && !blocked}
        <label class="acknowledge"
          ><input type="checkbox" bind:checked={acknowledged} disabled={busy} /> I
          have reviewed these warnings and want to install.</label
        >
      {/if}
      <div class="row">
        <button
          type="button"
          class="primary"
          disabled={busy || blocked || (hasWarnings && !acknowledged)}
          onclick={finish}>Install mod</button
        >
        <button
          type="button"
          disabled={busy}
          onclick={() => prepare(preparedFromLocal)}>Recheck</button
        >
      </div>
    {:else if !busy && source.kind === "download"}
      <button type="button" onclick={() => prepare()}>Retry download</button>
    {/if}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="footer">
    <button type="button" disabled={busy} onclick={close}>Cancel</button>
  </div>
</dialog>

<style>
  dialog {
    color: var(--text-primary, #eee);
    background: var(--ui-surface);
    border: 1px solid var(--ui-border);
    border-radius: 14px;
    padding: 20px;
    width: min(640px, calc(100vw - 48px));
    max-height: calc(100vh - 64px);
    overflow-y: auto;
    box-sizing: border-box;
  }
  dialog::backdrop {
    background: #000b;
  }
  h2 {
    margin: 0 0 18px;
    font-size: 16px;
  }
  h3 {
    font-size: 1rem;
    margin: 0 0 10px;
  }
  p,
  li {
    line-height: 1.5;
  }
  label {
    display: block;
    margin-top: 16px;
  }
  input:not([type="checkbox"]) {
    box-sizing: border-box;
    width: 100%;
    padding: 12px;
    margin-top: 8px;
    background: var(--ui-input-bg);
    color: #eee;
    border: 1px solid var(--ui-border);
    border-radius: 6px;
  }
  button {
    cursor: pointer;
    padding: 10px 14px;
    color: #eee;
    background: var(--ui-surface);
    border: 1px solid var(--ui-border);
    border-radius: 6px;
  }
  button:hover:not(:disabled) {
    background: var(--ui-glass-strong);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button.primary {
    background: var(--ui-success);
    border-color: var(--ui-border);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin: 16px 0;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    margin-top: 22px;
  }
  .hint {
    color: #b6b6b6;
    font-size: 0.85rem;
  }
  .source-url,
  .destination {
    overflow-wrap: anywhere;
    font-size: 0.88rem;
  }
  .destination {
    padding: 12px;
    border-radius: 6px;
    background: var(--ui-input-bg);
  }
  .warnings {
    border: 1px solid var(--ui-border);
    background: #71511c30;
    border-radius: 8px;
    padding: 16px;
  }
  ul {
    padding-left: 20px;
    margin-bottom: 0;
  }
  li + li {
    margin-top: 9px;
  }
  .blocking,
  .error {
    color: #ffaaaa;
  }
  .error {
    overflow-wrap: anywhere;
  }
  .acknowledge {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    line-height: 1.5;
  }
  .acknowledge input {
    margin-top: 5px;
  }
  button:focus-visible {
    outline: 2px solid var(--ui-accent);
    outline-offset: 2px;
  }
</style>
