<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";

  type VaultStatus =
    | { state: "first_run" }
    | { state: "ready"; live_items: number }
    | { state: "keychain_lost" }
    | { state: "inconsistent"; reason: string }
    | { state: "needs_passphrase"; is_new: boolean };

  interface ItemView {
    id: string;
    connector_id: string;
    source_id: string;
    kind: unknown;
    timestamp: string;
    ingested_at: string;
    properties: Record<string, unknown>;
  }

  interface Evidence {
    source_id: string;
    content: string;
  }

  interface Claim {
    id: string;
    topic: string;
    claim: string;
    time_range: [string, string];
    confidence: "High" | "Medium" | "Low";
    epistemic_state: string;
    evidence: Evidence[];
    revisions?: RevisionView[];
    revisionsLoading?: boolean;
    agent_id?: string;
    target_claim_id?: string;
  }

  interface RevisionView {
    revision_id: number;
    replaced_at: string;
    properties: Record<string, unknown>;
  }

  interface VaultStats {
    live_items: number;
    import_dir: string;
  }

  interface CapabilityManifest {
    id: string;
    name: string;
    description: string;
    inputs_schema: unknown;
    outputs_schema: unknown;
    side_effects: boolean;
  }

  interface CapabilityResult {
    data: unknown;
  }

  type GoogleAuthStatus =
    | { status: "not_configured" }
    | { status: "needs_auth" }
    | { status: "authenticated"; email?: string | null };

  type Phase =
    | "loading"
    | "first_run"
    | "ceremony_show"
    | "ceremony_verify"
    | "keychain_lost"
    | "inconsistent"
    | "needs_passphrase"
    | "ready"
    | "fatal";

  let phase = $state<Phase>("loading");
  let fatalMessage = $state("");
  let inconsistentReason = $state("");
  let isNewPassphrase = $state(false);
  let passphraseInput = $state("");

  // Ceremony state. recoveryKey exists in the UI only between
  // begin_first_run and verification, then is overwritten.
  let recoveryKey = $state("");
  let keyInput = $state("");
  let keyError = $state("");
  let copied = $state(false);
  let busy = $state(false);

  // Google OAuth state.
  let googleStatus = $state<GoogleAuthStatus>({ status: "not_configured" });
  let googleBusy = $state(false);
  let googleSyncing = $state(false);
  let googleError = $state("");

  // Dashboard state.
  let items = $state<ItemView[]>([]);
  let claims = $state<Claim[]>([]);
  let capabilities = $state<CapabilityManifest[]>([]);
  let capResultJSON = $state<string>("");
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
        case "needs_passphrase":
          isNewPassphrase = status.is_new;
          phase = "needs_passphrase";
          break;
      }
      await refreshGoogleStatus();
    } catch (e) {
      fatalMessage = String(e);
      phase = "fatal";
    }
  }

  async function refreshGoogleStatus() {
    try {
      googleStatus = await invoke<GoogleAuthStatus>("google_auth_status");
    } catch (e) {
      console.error("failed to get Google auth status:", e);
    }
  }

  async function connectGoogle() {
    googleBusy = true;
    googleError = "";
    try {
      googleStatus = await invoke<GoogleAuthStatus>("start_oauth");
      await loadData();
    } catch (e) {
      googleError = String(e);
    } finally {
      googleBusy = false;
    }
  }

  async function disconnectGoogle() {
    googleBusy = true;
    googleError = "";
    try {
      await invoke("google_logout");
      googleStatus = { status: "needs_auth" };
      await loadData();
    } catch (e) {
      googleError = String(e);
    } finally {
      googleBusy = false;
    }
  }

  async function syncGoogle() {
    googleSyncing = true;
    googleError = "";
    try {
      await invoke("trigger_google_sync");
      await loadData();
    } catch (e) {
      googleError = String(e);
    } finally {
      googleSyncing = false;
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

  function downloadKey() {
    const blob = new Blob([recoveryKey], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "wkyt-recovery-key.txt";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
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

  async function submitPassphrase() {
    busy = true;
    keyError = "";
    try {
      await invoke("set_passphrase", { passphrase: passphraseInput });
      const pass = passphraseInput;
      passphraseInput = "";
      if (isNewPassphrase) {
        await beginFirstRun();
      } else {
        await refreshStatus();
      }
    } catch (e) {
      keyError = String(e);
    } finally {
      busy = false;
    }
  }

  function enterDashboard() {
    phase = "ready";
    loadData();
    refreshGoogleStatus();
    if (!refreshTimer) {
      refreshTimer = setInterval(() => {
        loadData();
        refreshGoogleStatus();
      }, 5000);
    }
  }

  async function loadData() {
    try {
      stats = await invoke<VaultStats>("get_stats");
      items = await invoke<ItemView[]>("get_items", { limit: 200 });
      let newClaims = await invoke<Claim[]>("query_claims");
      claims = newClaims.map(nc => {
        const existing = claims.find(c => c.id === nc.id);
        if (existing) {
          return { ...nc, revisions: existing.revisions, revisionsLoading: existing.revisionsLoading };
        }
        return nc;
      });
      capabilities = await invoke<CapabilityManifest[]>("list_capabilities");
    } catch (e) {
      console.error("dashboard refresh failed:", e);
    }
  }

  async function runCapability(cap: CapabilityManifest) {
    try {
      const result = await invoke<CapabilityResult>("invoke_capability", {
        invocation: { capability_id: cap.id, arguments: {} }
      });
      capResultJSON = JSON.stringify(result.data, null, 2);
    } catch (e) {
      capResultJSON = `Error: ${String(e)}`;
    }
  }

  async function toggleRevisions(claim: Claim) {
    if (claim.revisions !== undefined) {
      claims = claims.map(c => c.id === claim.id ? { ...c, revisions: undefined } : c);
      return;
    }
    claims = claims.map(c => c.id === claim.id ? { ...c, revisionsLoading: true } : c);
    try {
      const revs = await invoke<RevisionView[]>("query_claim_revisions", { itemId: claim.id });
      claims = claims.map(c => c.id === claim.id ? { ...c, revisions: revs, revisionsLoading: false } : c);
    } catch (e) {
      console.error("failed to load revisions", e);
      claims = claims.map(c => c.id === claim.id ? { ...c, revisionsLoading: false } : c);
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

  function propsPreview(item: ItemView): string {
    const p = item.properties;
    if (item.connector_id === "google-calendar") {
      let parts = [];
      if (p.summary) parts.push(p.summary);
      if (p.location) parts.push(`📍 ${p.location}`);
      if (p.description) {
        let desc = String(p.description);
        desc = desc.replace(/<[^>]*>/g, ""); // Strip any HTML styling tags
        if (desc.length > 60) desc = desc.slice(0, 57) + "…";
        parts.push(`📝 ${desc}`);
      }
      return parts.join(" | ") || "(No details)";
    }
    
    if (p.content) {
      const contentStr = typeof p.content === "object" ? JSON.stringify(p.content) : String(p.content);
      return contentStr.length > 120 ? contentStr.slice(0, 117) + "…" : contentStr;
    }
    
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
        <button onclick={downloadKey}>Download .txt</button>
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
  {:else if phase === "needs_passphrase"}
    <section class="card">
      {#if isNewPassphrase}
        <h1>Create Passphrase</h1>
        <p>
          OS keychain is unavailable on this system. Please choose a passphrase
          to secure your local data vault.
        </p>
      {:else}
        <h1>Unlock Vault</h1>
        <p>
          Enter your passphrase to unlock your data vault.
        </p>
      {/if}
      <input
        type="password"
        class="key-input"
        placeholder="Enter passphrase"
        bind:value={passphraseInput}
        onkeydown={(e) => e.key === "Enter" && passphraseInput.trim() !== "" && submitPassphrase()}
        autocomplete="off"
        spellcheck="false"
      />
      <div class="row">
        <button onclick={submitPassphrase} disabled={busy || passphraseInput.trim() === ""}>
          {isNewPassphrase ? "Create and continue" : "Unlock"}
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
    <section class="google-panel card-inline">
      <div class="google-info">
        <div class="google-brand">
          <svg class="google-icon" viewBox="0 0 24 24" width="20" height="20">
            <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
            <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
            <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.06H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.94l2.85-2.22c-.62-.62-1.03-1.37-1.2-2.63z"/>
            <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.06l3.66 2.84c.87-2.6 3.3-4.52 6.16-4.52z"/>
          </svg>
          <strong>Google Calendar</strong>
        </div>
        
        {#if googleStatus.status === "not_configured"}
          <span class="badge badge-warning">Unconfigured</span>
        {:else if googleStatus.status === "needs_auth"}
          <span class="badge badge-info">Sign-in Required</span>
        {:else if googleStatus.status === "authenticated"}
          <span class="badge badge-success">Connected</span>
        {/if}
      </div>

      <div class="google-actions">
        {#if googleStatus.status === "not_configured"}
          <p class="muted small explanation">
            To enable Google Calendar sync, launch the app with the <code>WKYT_GOOGLE_CLIENT_ID</code> environment variable set.
          </p>
        {:else if googleStatus.status === "needs_auth"}
          <button class="primary" onclick={connectGoogle} disabled={googleBusy}>
            {#if googleBusy}
              Connecting…
            {:else}
              Connect Google Account
            {/if}
          </button>
        {:else if googleStatus.status === "authenticated"}
          <div class="btn-group">
            <button class="primary" onclick={syncGoogle} disabled={googleBusy || googleSyncing}>
              {#if googleSyncing}
                Syncing…
              {:else}
                Sync Calendar
              {/if}
            </button>
            <button class="outline" onclick={disconnectGoogle} disabled={googleBusy || googleSyncing}>
              Disconnect
            </button>
          </div>
        {/if}
      </div>
      {#if googleError}
        <p class="error small">{googleError}</p>
      {/if}
    </section>

    <p class="muted import-hint">
      Drop <code>.json</code> / <code>.ics</code> files into
      <code>{stats?.import_dir ?? "…"}</code> — they are picked up within ~10s.
    </p>

    <div class="claims-container">
      <h2>Temporal Claims</h2>
      {#if claims.length === 0}
        <p class="muted empty">No claims generated yet.</p>
      {:else}
        <div class="claims-list">
          {#each claims as claim (claim.id)}
            <div class="claim-card {claim.epistemic_state === 'disagreement' ? 'disagreement-card' : ''}">
              <div class="claim-header">
                <span class="topic">{claim.topic}</span>
                <span class={`badge confidence-${claim.confidence.toLowerCase()}`}>{claim.confidence}</span>
                <span class="badge epistemic-badge">{claim.epistemic_state.replace('_', ' ')}</span>
                {#if claim.agent_id}
                  <span class="badge badge-agent">🤖 {claim.agent_id}</span>
                {/if}
              </div>
              <h3 class="claim-text">{claim.claim}</h3>
              {#if claim.target_claim_id}
                <div class="target-claim-ref muted text-small">
                  ↳ Challenges claim: <code>{claim.target_claim_id.substring(0, 8)}...</code>
                </div>
              {/if}
              <div class="time-range text-small muted">
                {fmtTime(claim.time_range[0])}
                <button class="small link-button" onclick={() => toggleRevisions(claim)}>
                  {claim.revisions !== undefined ? "Hide History" : "View History"}
                </button>
              </div>
              
              {#if claim.revisionsLoading}
                <div class="revisions-section text-small muted">Loading history...</div>
              {:else if claim.revisions !== undefined}
                <div class="revisions-section">
                  <h4 class="text-small">Revision History</h4>
                  {#if claim.revisions.length === 0}
                    <p class="text-small muted">No previous revisions.</p>
                  {:else}
                    <ul class="revisions-list">
                      {#each claim.revisions as rev}
                        <li class="revision-item text-small">
                          <span class="rev-time">{fmtTime(rev.replaced_at)}</span>
                          <span class="rev-props">{JSON.stringify(rev.properties).slice(0, 100)}...</span>
                        </li>
                      {/each}
                    </ul>
                  {/if}
                </div>
              {/if}
              
              <div class="evidence-section">
                <h4 class="text-small">Evidence</h4>
                <ul>
                  {#each claim.evidence as ev}
                    <li class="evidence-item text-small">
                      <span class="source-id">{ev.source_id}</span>
                      <span class="content">{ev.content}</span>
                    </li>
                  {/each}
                </ul>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="capabilities-container">
      <h2 class="section-title">Capabilities (Phase 2 Preview)</h2>
      {#if capabilities.length === 0}
        <p class="muted empty">No capabilities found.</p>
      {:else}
        <div class="capabilities-list">
          {#each capabilities as cap}
            <div class="capability-card card-inline">
              <div class="cap-header">
                <strong>{cap.name}</strong> <code>{cap.id}</code>
                {#if cap.side_effects}<span class="badge badge-warning">Side Effects</span>{/if}
              </div>
              <p class="muted text-small">{cap.description}</p>
              <button class="small" onclick={() => runCapability(cap)}>Invoke</button>
            </div>
          {/each}
        </div>
        {#if capResultJSON}
          <div class="cap-result">
            <h4>Inspectable Result</h4>
            <pre><code>{capResultJSON}</code></pre>
            <button class="small outline" onclick={() => capResultJSON = ""}>Clear</button>
          </div>
        {/if}
      {/if}
    </div>

    <h2 class="section-title">Raw Ingestion Stream</h2>
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
              <td class="props">{propsPreview(item)}</td>
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

  .text-small {
    font-size: 0.85rem;
  }

  .section-title {
    margin-top: 2rem;
    font-size: 1.2rem;
    border-bottom: 1px solid #eaeaea;
    padding-bottom: 0.5rem;
  }

  .claims-container {
    margin: 2rem 0;
  }

  .claims-container h2 {
    font-size: 1.2rem;
    border-bottom: 1px solid #eaeaea;
    padding-bottom: 0.5rem;
  }

  .claims-list {
    display: grid;
    gap: 1.5rem;
    margin-top: 1rem;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
  }

  .capabilities-container {
    margin: 2rem 0;
  }

  .capabilities-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    margin-top: 1rem;
  }

  .capability-card {
    padding: 1rem;
    background: #fff;
    border-radius: 8px;
    border: 1px solid #ddd;
  }

  .cap-header {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    margin-bottom: 0.3rem;
  }

  .cap-header code {
    font-size: 0.8rem;
    color: #555;
    background: #f0f0f0;
    padding: 0.1rem 0.3rem;
    border-radius: 4px;
  }

  .cap-result {
    margin-top: 1rem;
    background: #1e1e1e;
    color: #d4d4d4;
    padding: 1rem;
    border-radius: 8px;
    overflow-x: auto;
  }

  .cap-result pre {
    margin: 0.5rem 0;
    font-size: 0.85rem;
  }

  .claim-card {
    background: #ffffff;
    border-radius: 12px;
    padding: 1.2rem;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.05);
    border: 1px solid #eaeaea;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .claim-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .topic {
    font-weight: 600;
    color: #4285F4;
    text-transform: uppercase;
    font-size: 0.8rem;
    letter-spacing: 0.05em;
  }

  .confidence-high {
    background: #effff2;
    color: #1c8833;
    border: 1px solid #d1fad8;
  }

  .confidence-medium {
    background: #fff8e6;
    color: #b07c00;
    border: 1px solid #ffe8cc;
  }

  .confidence-low {
    background: #fff0f0;
    color: #c43c3c;
    border: 1px solid #fadcdc;
  }

  .claim-text {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 500;
  }

  .time-range {
    margin-bottom: 0.5rem;
  }

  .evidence-section {
    margin-top: auto;
    padding-top: 0.8rem;
    border-top: 1px dashed #eaeaea;
  }

  .evidence-section h4 {
    margin: 0 0 0.5rem 0;
    font-weight: 600;
    color: #555;
  }

  .evidence-section ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .evidence-item {
    display: flex;
    flex-direction: column;
    background: #f9f9f9;
    padding: 0.5rem 0.6rem;
    border-radius: 6px;
    border: 1px solid #f0f0f0;
  }

  .source-id {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.75rem;
    color: #888;
    margin-bottom: 0.2rem;
    word-break: break-all;
  }

  .content {
    color: #333;
    line-height: 1.3;
  }

  .epistemic-badge {
    background: #f0f4ff;
    color: #4285F4;
    border: 1px solid #d2e3fc;
    text-transform: capitalize;
  }

  .link-button {
    background: none;
    border: none;
    color: #4285F4;
    padding: 0;
    margin-left: 10px;
    box-shadow: none;
    cursor: pointer;
  }

  .link-button:hover {
    text-decoration: underline;
  }

  .revisions-section {
    background: #fafafa;
    border: 1px solid #eee;
    border-radius: 6px;
    padding: 0.8rem;
    margin-top: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .revisions-section h4 {
    margin: 0 0 0.5rem 0;
    color: #555;
  }

  .revisions-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .revision-item {
    display: flex;
    flex-direction: column;
    border-bottom: 1px solid #eee;
    padding-bottom: 0.4rem;
  }

  .revision-item:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }

  .rev-time {
    color: #666;
    font-weight: 500;
  }

  .rev-props {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.75rem;
    color: #888;
  }

  .evidence-section h4 {
    margin: 0 0 0.5rem 0;
    color: #555;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .evidence-section ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .evidence-item {
    display: flex;
    gap: 0.5rem;
    background: #f9f9f9;
    padding: 0.4rem 0.6rem;
    border-radius: 6px;
    border: 1px solid #eee;
  }

  .source-id {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    color: #888;
    background: #eee;
    padding: 0.1rem 0.3rem;
    border-radius: 4px;
    font-size: 0.75rem;
  }

  .empty {
    text-align: center;
    margin-top: 3rem;
  }

  .error {
    color: #c43c3c;
  }

  /* Inline card for panel settings */
  .card-inline {
    background: #ffffff;
    border-radius: 12px;
    padding: 1.2rem;
    margin-bottom: 1.5rem;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.05);
    border: 1px solid #eaeaea;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1.5rem;
    flex-wrap: wrap;
    transition: transform 0.2s, box-shadow 0.2s;
  }

  .card-inline:hover {
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
  }

  .google-panel {
    border-left: 4px solid #4285F4;
  }

  .google-info {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .google-brand {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    font-size: 1.05rem;
  }

  /* Badges */
  .badge {
    padding: 0.25em 0.7em;
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .badge-warning {
    background: #fff8e6;
    color: #b07c00;
    border: 1px solid #ffe8cc;
  }

  .badge-info {
    background: #eef7ff;
    color: #006cd8;
    border: 1px solid #d0ebff;
  }

  .badge-success {
    background: #effff2;
    color: #1c8833;
    border: 1px solid #d1fad8;
  }

  .google-actions {
    display: flex;
    align-items: center;
  }

  .explanation {
    margin: 0;
    max-width: 320px;
    line-height: 1.4;
  }

  .explanation code {
    background: #f0f0f0;
    padding: 0.1em 0.3em;
    border-radius: 4px;
    font-size: 0.85em;
  }

  .btn-group {
    display: flex;
    gap: 0.5rem;
  }

  button.outline {
    background: transparent;
    border: 1px solid #ccc;
    box-shadow: none;
  }

  button.outline:hover {
    border-color: #999;
    background: #fcfcfc;
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
    .card-inline {
      background: #2a2a2a;
      border-color: #3d3d3d;
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
    }
    .card-inline:hover {
      box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
    }
    .explanation code {
      background: #1a1a1a;
    }
    button.outline {
      border-color: #555;
    }
    button.outline:hover {
      border-color: #777;
      background: #333;
    }
    .section-title, .claims-container h2 {
      border-bottom-color: #3d3d3d;
    }
    .claim-card {
      background: #2a2a2a;
      border-color: #3d3d3d;
    }
    .evidence-section {
      border-top-color: #3d3d3d;
    }
    .evidence-section h4 {
      color: #999;
    }
    .evidence-item {
      background: #222;
      border-color: #333;
    }
    .source-id {
      background: #333;
      color: #aaa;
    }
    .confidence-high {
      background: #10321a;
      color: #98eeb1;
      border-color: #194a28;
    }
    .confidence-medium {
      background: #3e2f12;
      color: #ffd073;
      border-color: #594215;
    }
    .confidence-low {
      background: #421818;
      color: #ff9c9c;
      border-color: #5c2020;
    }
    .badge-warning {
      background: #3e2f12;
      color: #ffd073;
      border-color: #594215;
    }
    .badge-info {
      background: #132742;
      color: #9cd4ff;
      border-color: #1b385e;
    }
    .badge-success {
      background: #10321a;
      color: #98eeb1;
      border-color: #194a28;
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
  
  .disagreement-card {
    border-left: 4px solid #f44336;
    background-color: #fff9f9;
  }
  
  .badge-agent {
    background: #e8dbff;
    color: #551a8b;
    border-color: #d1b8ff;
  }
  
  .target-claim-ref {
    margin-top: 0.5rem;
    margin-bottom: 0.5rem;
  }
  
  @media (prefers-color-scheme: dark) {
    .disagreement-card {
      background-color: #2a1f1f;
      border-left-color: #d32f2f;
    }
    .badge-agent {
      background: #2d1f42;
      color: #cdadff;
      border-color: #4b326e;
    }
  }
</style>
