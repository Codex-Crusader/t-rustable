<script lang="ts">
  // One mod panel in a sandboxed iframe (mod system rule 6). The panel has no Tauri
  // access. Messages go only between this iframe and its own mod.
  import { api, on } from "../lib/api";
  import { app } from "../lib/store.svelte";
  import { schemeBase } from "../lib/tiles";

  let { modId, panelId, title, entry }: { modId: string; panelId: string; title: string; entry: string } = $props();

  let frame: HTMLIFrameElement | undefined = $state();
  const src = $derived(`${schemeBase("trpanel", app.info.platform)}/${modId}/${entry}`);

  $effect(() => {
    const [m, p] = [modId, panelId];
    void api.panelOpen(m, p, true);
    return () => void api.panelOpen(m, p, false);
  });

  // Panel to mod. The source check makes sure that the message comes from this iframe.
  $effect(() => {
    const onMessage = (e: MessageEvent) => {
      if (!frame || e.source !== frame.contentWindow || typeof e.data !== "string") return;
      api.panelMessage(modId, panelId, e.data).catch((err) => app.fail(err));
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  });

  // Mod to panel.
  $effect(() => {
    const unlisten = on("panel-message", (e) => {
      if (e.modId !== modId || e.panelId !== panelId) return;
      frame?.contentWindow?.postMessage(e.message, "*");
    });
    return () => void unlisten.then((f) => f());
  });
</script>

{#key app.panelTheme}
  <iframe {title} {src} sandbox="allow-scripts" bind:this={frame}></iframe>
{/key}

<style>
  iframe {
    width: 100%;
    height: 100%;
    border: 0;
    background: var(--tr-surface);
  }
</style>
