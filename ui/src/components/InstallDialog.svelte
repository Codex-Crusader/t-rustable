<script lang="ts">
  // The permission screen of an install or an update. It is one question in the queue.
  import type { Ask } from "../lib/store.svelte";
  import { S } from "../lib/strings";
  import type { Offer } from "../lib/types";

  let { ask, offer }: { ask: Ask; offer: Offer } = $props();

  const isNew = (p: string) => offer.previousVersion !== null && offer.newPermissions.includes(p as never);
</script>

<div class="backdrop">
  <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="install-title" aria-describedby="install-text">
    <h2 id="install-title">{ask.title}</h2>
    <div id="install-text">
      <p class="meta">
        {S.install.version(offer.version)}. {S.install.by(offer.author)}. <span class="id">{offer.id}</span>
      </p>
      {#if offer.description}<p>{offer.description}</p>{/if}
      {#if offer.previousVersion}
        <p>{S.install.update(offer.previousVersion, offer.version)}</p>
      {/if}
      {#if offer.previousVersion && offer.newPermissions.length}
        <p class="warn">{S.install.newPermissions}</p>
      {/if}
      {#if offer.permissions.length}
        <p>{S.install.asks}</p>
        <ul>
          {#each offer.permissions as p (p)}
            <li>
              {#if isNew(p)}<strong class="new">{S.install.newMark}</strong>{/if}
              {S.permissions[p]} <code>{p}</code>
            </li>
          {/each}
        </ul>
      {:else}
        <p>{S.install.none}</p>
      {/if}
      <p class="sha">{S.install.fingerprint(offer.sha256.slice(0, 12))}</p>
    </div>
    <div class="buttons">
      <!-- svelte-ignore a11y_autofocus -->
      <button type="button" autofocus onclick={() => ask.resolve(false)}>{S.dialog.cancel}</button>
      <button type="button" class="primary" onclick={() => ask.resolve(true)}
        >{offer.previousVersion ? S.install.updateButton : S.install.install}</button
      >
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.4);
  }
  .dialog {
    max-width: 520px;
    padding: 16px 20px;
    border-radius: var(--tr-radius);
    background: var(--tr-surface);
    border: 1px solid var(--tr-border);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 1.15em;
  }
  .meta,
  .sha {
    color: var(--tr-text-muted);
  }
  .id,
  code {
    font-family: ui-monospace, monospace;
    font-size: 0.9em;
  }
  ul {
    padding-left: 20px;
  }
  li {
    margin: 4px 0;
  }
  .new {
    margin-right: 6px;
    padding: 0 5px;
    border-radius: 3px;
    border: 2px solid var(--tr-warning);
    font-weight: 700;
  }
  .warn {
    border-left: 4px solid var(--tr-warning);
    padding-left: 8px;
  }
  .buttons {
    display: flex;
    justify-content: end;
    gap: 8px;
  }
  .buttons button {
    padding: 4px 10px;
    border: 1px solid var(--tr-border);
    border-radius: var(--tr-radius);
    background: var(--tr-surface-2);
  }
  .buttons .primary {
    background: var(--tr-accent);
    color: var(--tr-accent-text);
    border-color: var(--tr-accent);
  }
</style>
