<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  interface Query {
    text: string; connector_ids: string[]; kind: string | { other: string } | null;
    time_axis: "event" | "recorded"; from: string | null; to: string | null;
    as_of: number | null; include_deleted: boolean; limit: number;
  }
  interface Saved { id: string; name: string; query: Query }
  interface Boundary { sequence: number; recorded_at: string }
  interface Result {
    boundary: Boundary; coverage_start: Boundary; truncated: boolean;
    items: { revision: number; recorded_at: string; deleted_at: string | null;
      item: { id: string; source_id: string; connector_id: string; kind: unknown;
        timestamp: string; properties: Record<string, unknown>; raw_payload: unknown } }[];
  }
  const initial = (): Query => ({ text: "", connector_ids: [], kind: null, time_axis: "event",
    from: null, to: null, as_of: null, include_deleted: false, limit: 50 });
  let active = initial();
  let text = "", connectors = "", kind = "", from = "", to = "";
  let axis: "event" | "recorded" = "event";
  let includeDeleted = false;
  let saved: Saved[] = [];
  let selected = "", name = "", error = "";
  let result: Result | null = null;
  let busy = false;
  let mounted = true;

  async function call<T>(id: string, args: unknown): Promise<T> {
    const response = await invoke<{ data: T }>("invoke_capability", {
      invocation: { capability_id: id, arguments: args }
    });
    return response.data;
  }
  async function refresh() {
    if (busy) return;
    busy = true;
    try {
      const [next, views] = await Promise.all([
        call<Result>("core.query_stream", active), call<Saved[]>("core.list_substreams", {})
      ]);
      if (mounted) { result = next; saved = views; error = ""; }
    } catch (e) { if (mounted) error = String(e); }
    finally { if (mounted) busy = false; }
  }
  function localTime(value: string | null) {
    if (!value) return "";
    const d = new Date(value);
    return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
  }
  function edit(query: Query) {
    text = query.text; connectors = query.connector_ids.join(", "); kind = query.kind === null ? "" : JSON.stringify(query.kind);
    axis = query.time_axis; from = localTime(query.from); to = localTime(query.to);
    includeDeleted = query.include_deleted;
  }
  function search() {
    active = { ...active, text, connector_ids: connectors.split(",").map(s => s.trim()).filter(Boolean),
      kind: kind ? JSON.parse(kind) : null, time_axis: axis, from: from ? new Date(from).toISOString() : null,
      to: to ? new Date(to).toISOString() : null, include_deleted: includeDeleted };
    result = null;
    void refresh();
  }
  function open(view: Saved) {
    selected = view.id; name = view.name; active = { ...view.query };
    edit(active); result = null; void refresh();
  }
  async function save(asNew: boolean) {
    if (busy || !name.trim() || !result) return;
    busy = true;
    try {
      const id = asNew ? crypto.randomUUID() : selected;
      saved = await call<Saved[]>("core.save_substream", { id, name: name.trim(), query: active });
      selected = id; error = "";
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  async function remove(id: string) {
    if (busy) return;
    busy = true;
    try {
      saved = await call<Saved[]>("core.delete_substream", { id });
      if (selected === id) { selected = ""; name = ""; }
      error = "";
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
  function setPinned(pin: boolean) {
    if (!result) return;
    active = { ...active, as_of: pin ? result.boundary.sequence : null };
    void refresh();
  }
  onMount(() => {
    void refresh();
    const timer = setInterval(() => { if (active.as_of === null) void refresh(); }, 5000);
    return () => { mounted = false; clearInterval(timer); };
  });
</script>

<section aria-labelledby="stream-title" class="stream-panel">
  <h2 id="stream-title">History and saved views</h2>
  <p>Find source records and claims across your vault. Saved views share the same evidence.</p>
  <form onsubmit={(event) => { event.preventDefault(); search(); }}>
    <div class="filters">
      <label>Find text <input bind:value={text} maxlength="1024" placeholder="Project Alpha" /></label>
      <label>Sources <input bind:value={connectors} placeholder="All, or file-import, google-calendar" /></label>
      <label>Record type <select bind:value={kind}>
        <option value="">All types</option><option value={'"file"'}>Files</option>
        <option value={'"event"'}>Events</option><option value={'"claim"'}>Claims</option>
        <option value={'"relationship"'}>Evidence relationships</option>
      {#if kind && !['"file"', '"event"', '"claim"', '"relationship"'].includes(kind)}
          <option value={kind}>Saved type: {kind}</option>
        {/if}
      </select></label>
      <label>Time filter <select bind:value={axis}>
        <option value="event">Source event time</option><option value="recorded">Recorded change time</option>
      </select></label>
      <label>From (inclusive, local time) <input type="datetime-local" bind:value={from} /></label>
      <label>Until (exclusive, local time) <input type="datetime-local" bind:value={to} /></label>
    </div>
    <label class="deleted"><input type="checkbox" bind:checked={includeDeleted} /> Include deleted records</label>
    <button disabled={busy} type="submit">Apply filters</button>
  </form>
  <p class="hint">Text matches source identifiers and structured properties, without semantic inference. Times describe the source record; a scheduled event is not proof that it occurred.</p>
  {#if error}<p role="alert">{error}</p>{/if}
  <div class="actions">
    <button disabled={busy} onclick={() => refresh()}>Refresh</button>
    <button disabled={busy || !result} onclick={() => setPinned(active.as_of === null)}>
      {active.as_of === null ? "Pin this moment" : "Return to live view"}
    </button>
    <span aria-live="polite">{busy ? "Loading…" : active.as_of === null ? "Live · refreshes every 5 seconds" : "Pinned historical view"}</span>
  </div>
  {#if result}
    <p class="hint">Knowledge at {new Date(result.boundary.recorded_at).toLocaleString()}.
      Item history is available from {new Date(result.coverage_start.recorded_at).toLocaleString()}; earlier states are unavailable here.</p>
    <div class="actions">
      <label>View name <input bind:value={name} maxlength="256" placeholder="Alpha this week" /></label>
      <button disabled={busy || !name.trim()} onclick={() => save(true)}>Save as new view</button>
      {#if selected}<button disabled={busy || !name.trim()} onclick={() => save(false)}>Update selected view</button>{/if}
    </div>
    <p class="hint">Saves the applied filters and live/pinned setting. Apply edited filters before saving.</p>
    {#if result.truncated}<p>Showing the first {active.limit} matches. Narrow the filters to see more.</p>{/if}
    {#if result.items.length === 0}<p>No matching records at this boundary.</p>{/if}
    <ul class="results">
      {#each result.items as version (version.item.id)}
        <li>
          <strong>{String(version.item.properties.assertion ?? version.item.properties.summary ?? version.item.properties.filename ?? version.item.source_id)}</strong>
          {#if version.deleted_at}<span> · Deleted</span>{/if}
          <p class="hint">{version.item.connector_id} · {version.item.source_id}<br />
            Event: {new Date(version.item.timestamp).toLocaleString()} · Recorded: {new Date(version.recorded_at).toLocaleString()}</p>
          <details><summary>Inspect evidence and metadata</summary>
            <pre>{JSON.stringify(version.item, null, 2)}</pre>
          </details>
        </li>
      {/each}
    </ul>
  {/if}
  <h3>Saved views</h3>
  {#if saved.length === 0}<p>No saved views yet.</p>{/if}
  <ul class="saved">
    {#each saved as view (view.id)}
      <li>
        <button disabled={busy} aria-pressed={selected === view.id} onclick={() => open(view)}>{view.name}</button>
        <span>{view.query.as_of === null ? "Live" : "Pinned"}</span>
        <button disabled={busy} onclick={() => remove(view.id)} aria-label={`Remove view ${view.name}`}>Remove view</button>
      </li>
    {/each}
  </ul>
  <p class="hint">Removing a view keeps its sources and definition history. It does not erase personal data.</p>
</section>

<style>
  .stream-panel { margin: 2rem 0; padding: 1.25rem; border: 1px solid #8885; border-radius: 12px; }
  .filters { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: .8rem; }
  label { display: flex; flex-direction: column; gap: .3rem; }
  input, select, button { font: inherit; padding: .5rem; border-radius: 5px; border: 1px solid #8888; }
  input, select { background: transparent; color: inherit; }
  button { cursor: pointer; } button:disabled { cursor: default; opacity: .5; }
  .deleted, .actions, .saved li { display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; gap: .6rem; margin: .8rem 0; }
  .hint { font-size: .85rem; opacity: .8; }
  .results, .saved { list-style: none; padding: 0; }
  .results li { border-top: 1px solid #8885; padding: .8rem 0; overflow-wrap: anywhere; }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 320px; overflow: auto; }
</style>
