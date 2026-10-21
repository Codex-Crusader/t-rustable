<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { S } from "../lib/strings";
  import ModPanel from "./ModPanel.svelte";

  const panels = $derived(
    app.mods.flatMap((m) =>
      m.disabled
        ? []
        : m.panels.map((p) => ({
            key: `${m.id}/${p.id}`,
            modId: m.id,
            panel: p,
            title: m.developer ? `${p.title} (${S.manager.source.developer})` : p.title,
          })),
    ),
  );
  const current = $derived(panels.find((p) => p.key === app.activePanel) ?? panels[0]);
</script>

{#if current}
  <aside class="host" aria-label={S.panels.label}>
    {#if panels.length > 1}
      <label class="choose">
        {S.panels.choose}
        <select bind:value={() => current.key, (v) => (app.activePanel = v)}>
          {#each panels as p (p.key)}
            <option value={p.key}>{p.title}</option>
          {/each}
        </select>
      </label>
    {:else}
      <h2>{current.title}</h2>
    {/if}
    {#key `${current.key}#${app.panelReloads[current.modId] ?? 0}`}
      <ModPanel modId={current.modId} panelId={current.panel.id} title={current.panel.title} entry={current.panel.entry} />
    {/key}
  </aside>
{/if}

<style>
  .host {
    display: grid;
    grid-template-rows: auto 1fr;
    width: 300px;
    min-height: 0;
    background: var(--tr-surface);
    border-left: 1px solid var(--tr-border);
  }
  h2,
  .choose {
    margin: 0;
    padding: 6px 10px;
    font-size: 1em;
    border-bottom: 1px solid var(--tr-border);
  }
  .choose {
    display: flex;
    gap: 8px;
    align-items: center;
  }
</style>
