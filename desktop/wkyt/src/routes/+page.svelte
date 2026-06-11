<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";

  type VaultStatus =
    | { state: "first_run" }
    | { state: "ready"; live_items: number }
    | { state: "keychain_lost" }
    | { state: "inconsistent"; reason: string };

  interface ItemView {
    id: string;
    connector_id: string;
    source_id: string;
    kind: unknown;
    timestamp: string;
    ingested_at: string;
    properties: Record<string, unknown>;
  }

  interface VaultStats {
    live_items: number;
    import_dir: string;
  }

  type Phase =
    | "loading"
    | "first_run"
    | "ceremony_show"
    | "ceremony_verify"
    | "keychain_lost"
    | "inconsistent"
    | "ready"
    | "fatal";

  let phase = $state<Phase>("loading");
  let fatalMessage = $state("");
  let inconsistentReason = $state("");

  // Ceremony state. recoveryKey exists in the UI only between
  // begin_first_run and verification, then is overwritten.
  let recoveryKey = $state("");
  let keyInput = $state("");
  let keyError = $state("");
  let copied = $state(false);
  let busy = $state(false);

  // Dashboard state.
  let items = $state<ItemView[]>([]);
  let stats = $state<VaultStats | null>(null);
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  onMount(refreshStatus);
  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });

  async function refreshStatus() {
    try {
      const status = await invoke<VaultStatus>("vault_status");
      switch (status.state) {
        case "first_run":
          phase = "first_run";
          break;
        case "ready":
          enterDashboard();
          break;
        case "keychain_lost":
          phase = "keychain_lost";
          break;
        case "inconsistent":
          inconsistentReason = status.reason;
          phase = "inconsistent";
          break;
      }
    } catch (e) {
      fatalMessage = String(e);
      phase = "fatal";
    }
  }

  async function beginFirstRun() {
    busy = true;
    keyError = "";
    try {
      recoveryKey = await invoke<string>("begin_first_run");
      copied = false;
      phase = "ceremony_show";
    } catch (e) {
      keyError = String(e);
    } finally {
      busy = false;
    }
  }

  async function copyKey() {
    await navigator.clipboard.writeText(recoveryKey);
    copied = true;
  }

  function proceedToVerify() {
    phase = "ceremony_verify";
    keyInput = "";
    keyError = "";
  }

  async function verifyKey() {
    busy = true;
    keyError = "";
    try {
      await invoke("verify_recovery_key", { input: keyInput });
      recoveryKey = ""; // gone from the UI for good
      keyInput = "";
      enterDashboard();
    } catch (e) {
      keyError = String(e);
    } finally {
      busy = false;
    }
  }

  async function recoverWithKey() {
    busy = true;
    keyError = "";
    try {
      await invoke("recover_with_key", { input: keyInput });
      keyInput = "";
      enterDashboard();
    } catch (e) {
      keyError = String(e);
    } finally {
      busy = false;
    }
  }

  function enterDashboard() {
    phase = "ready";
    loadData();
    if (!refreshTimer) refreshTimer = setInterval(loadData, 5000);
  }

  async function loadData() {
    try {
      stats = await invoke<VaultStats>("get_stats");
      items = await invoke<ItemView[]>("get_items", { limit: 200 });
    } catch (e) {
      console.error("dashboard refresh failed:", e);
    }
  }

  function kindLabel(kind: unknown): string {
    if (typeof kind === "string") return kind;
    if (kind && typeof kind === "object") {
      const k = kind as Record<string, string>;
      if ("other" in k) return k.other;
    }
    return String(kind);
  }

  function propsPreview(p: Record<string, unknown>): string {
    const json = JSON.stringify(p);
    return json.length > 120 ? json.slice(0, 117) + "…" : json;
  }

  function fmtTime(iso: string): string {
    return new Date(iso).toLocaleString();
  }
</script>

<main class="container">
  {#if phase === "loading"}
    <p class="muted">Unlocking vault…</p>
  {:else if phase === "first_run"}
    <section class="card">
      <h1>Welcome to wkyt</h1>
      <p>
        Your data vault is encrypted on this device. Before anything is
        ingested, you'll get a <strong>recovery key</strong> — the only way
        back in if this machine's keychain is ever lost.
      </p>
      <button onclick={beginFirstRun} disabled={busy}>Create my vault</button>
      {#if keyError}<p class="error">{keyError}</p>{/if}
    </section>
  {:else if phase === "ceremony_show"}
    <section class="card">
      <h1>Your recovery key</h1>
      <p>
        Write this down or store it in a password manager.
        <strong>It will never be shown again.</strong> Without it, losing
        this machine's keychain means losing your data.
      </p>
      <code class="recovery-key">{recoveryKey}</code>
      <div class="row">
        <button onclick={copyKey}>{copied ? "Copied ✓" : "Copy"}</button>
        <button class="primary" onclick={proceedToVerify}>
          I saved it — verify me
        </button>
      </div>
    </section>
  {:else if phase === "ceremony_verify"}
    <section class="card">
      <h1>Verify your recovery key</h1>
      <p>
        Enter the key you just saved. This proves the copy you kept actually
        works — the screen showing it is gone.
      </p>
      <input
        class="key-input"
        placeholder="XXXX-XXXX-…"
        bind:value={keyInput}
        autocomplete="off"
        spellcheck="false"
      />
      <div class="row">
        <button onclick={verifyKey} disabled={busy || keyInput.trim() === ""}>
          Verify and open vault
        </button>
      </div>
      {#if keyError}<p class="error">{keyError}</p>{/if}
    </section>
  {:else if phase === "keychain_lost"}
    <section class="card">
      <h1>Keychain lost</h1>
      <p>
        This vault exists, but the OS keychain no longer holds its key
        (reinstalled OS? new keyring?). Enter your recovery key to restore
        access — your data is intact.
      </p>
      <input
        class="key-input"
        placeholder="XXXX-XXXX-…"
        bind:value={keyInput}
        autocomplete="off"
        spellcheck="false"
      />
      <div class="row">
        <button onclick={recoverWithKey} disabled={busy || keyInput.trim() === ""}>
          Recover vault
        </button>
      </div>
      {#if keyError}<p class="error">{keyError}</p>{/if}
    </section>
  {:else if phase === "inconsistent"}
    <section class="card">
      <h1>Vault needs attention</h1>
      <p class="error">{inconsistentReason}</p>
      <p class="muted">
        The vault file and its key material disagree. This needs manual
        intervention — nothing has been changed.
      </p>
    </section>
  {:else if phase === "fatal"}
    <section class="card">
      <h1>Something went wrong</h1>
      <p class="error">{fatalMessage}</p>
      <button onclick={refreshStatus}>Retry</button>
    </section>
  {:else if phase === "ready"}
    <header class="topbar">
      <h1>wkyt vault</h1>
      <div class="stats">
        <span><strong>{stats?.live_items ?? "…"}</strong> items</span>
        <button class="small" onclick={loadData}>Refresh</button>
      </div>
    </header>
    <p class="muted import-hint">
      Drop <code>.json</code> / <code>.ics</code> files into
      <code>{stats?.import_dir ?? "…"}</code> — they are picked up within ~10s.
    </p>

    {#if items.length === 0}
      <p class="muted empty">The vault is empty so far.</p>
    {:else}
      <table>
        <thead>
          <tr>
            <th>When</th>
            <th>Connector</th>
            <th>Source</th>
            <th>Kind</th>
            <th>Properties</th>
          </tr>
        </thead>
        <tbody>
          {#each items as item (item.id)}
            <tr>
              <td class="nowrap">{fmtTime(item.timestamp)}</td>
              <td>{item.connector_id}</td>
              <td>{item.source_id}</td>
              <td><span class="kind">{kindLabel(item.kind)}</span></td>
              <td class="props">{propsPreview(item.properties)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  {/if}
</main>

<style>
  :root {
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    font-size: 15px;
    line-height: 1.5;
    color: #0f0f0f;
    background-color: #f6f6f6;
  }

  .container {
    max-width: 920px;
    margin: 0 auto;
    padding: 2rem 1.5rem;
  }

  .card {
    max-width: 560px;
    margin: 12vh auto 0;
    padding: 2rem;
    background: #ffffff;
    border-radius: 12px;
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.08);
  }

  .card h1 {
    margin-top: 0;
    font-size: 1.4rem;
  }

  .recovery-key {
    display: block;
    margin: 1.2rem 0;
    padding: 1rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.95rem;
    word-break: break-all;
    background: #f0f0f0;
    border-radius: 8px;
    user-select: all;
  }

  .key-input {
    width: 100%;
    box-sizing: border-box;
    margin: 0.8rem 0;
    padding: 0.7em 0.9em;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    border-radius: 8px;
    border: 1px solid #ccc;
  }

  .row {
    display: flex;
    gap: 0.6rem;
    margin-top: 0.6rem;
  }

  button {
    border-radius: 8px;
    border: 1px solid transparent;
    padding: 0.55em 1.1em;
    font-size: 0.95em;
    font-weight: 500;
    font-family: inherit;
    color: #0f0f0f;
    background-color: #ffffff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
    cursor: pointer;
    transition: border-color 0.2s;
  }

  button:hover {
    border-color: #396cd8;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  button.primary {
    background: #396cd8;
    color: #fff;
  }

  button.small {
    padding: 0.3em 0.8em;
    font-size: 0.85em;
  }

  .topbar {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .topbar h1 {
    font-size: 1.3rem;
    margin: 0;
  }

  .stats {
    display: flex;
    align-items: center;
    gap: 0.8rem;
  }

  .import-hint {
    margin-top: 0.2rem;
  }

  .import-hint code {
    background: #ececec;
    padding: 0.1em 0.4em;
    border-radius: 4px;
  }

  table {
    width: 100%;
    margin-top: 1rem;
    border-collapse: collapse;
    font-size: 0.88rem;
  }

  th {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 2px solid #ddd;
    font-weight: 600;
  }

  td {
    padding: 0.45rem 0.6rem;
    border-bottom: 1px solid #e8e8e8;
    vertical-align: top;
  }

  .nowrap {
    white-space: nowrap;
  }

  .kind {
    background: #e6efff;
    color: #2a55b0;
    padding: 0.1em 0.5em;
    border-radius: 999px;
    font-size: 0.8rem;
  }

  .props {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.78rem;
    color: #555;
    word-break: break-all;
  }

  .muted {
    color: #777;
  }

  .empty {
    text-align: center;
    margin-top: 3rem;
  }

  .error {
    color: #c43c3c;
  }

  @media (prefers-color-scheme: dark) {
    :root {
      color: #f6f6f6;
      background-color: #1f1f1f;
    }
    .card {
      background: #2a2a2a;
      box-shadow: 0 2px 12px rgba(0, 0, 0, 0.4);
    }
    .recovery-key {
      background: #1a1a1a;
    }
    .key-input {
      background: #1a1a1a;
      color: #f6f6f6;
      border-color: #444;
    }
    button {
      color: #f6f6f6;
      background-color: #0f0f0f98;
    }
    .import-hint code {
      background: #333;
    }
    th {
      border-bottom-color: #444;
    }
    td {
      border-bottom-color: #333;
    }
    .kind {
      background: #25355c;
      color: #9db9f5;
    }
    .props {
      color: #aaa;
    }
  }
</style>
