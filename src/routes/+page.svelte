<script>
  import { onMount, onDestroy } from "svelte";
  import { appStore } from "$lib/stores/app.svelte.js";
  import { fileStore } from "$lib/stores/files.svelte.js";
  import { conversionStore } from "$lib/stores/conversion.svelte.js";
  import { checkFfmpeg, setupListeners } from "$lib/services/tauri.js";
  import { addFiles, addFilesFromBrowse } from "$lib/services/actions.js";
  import SetupScreen from "$lib/components/screens/SetupScreen.svelte";
  import ConvertingScreen from "$lib/components/screens/ConvertingScreen.svelte";
  import CompleteScreen from "$lib/components/screens/CompleteScreen.svelte";

  /** @type {(() => void) | undefined} */
  let cleanup;

  onMount(async () => {
    try {
      await checkFfmpeg();
      appStore.ffmpegOk = true;
    } catch {
      appStore.ffmpegOk = false;
      appStore.error = "ffmpeg/ffprobe not found. Please install ffmpeg.";
    }

    cleanup = await setupListeners({
      onProgress: (p) => conversionStore.handleProgress(p),
      onComplete: (p) => conversionStore.handleComplete(p),
      onError: (p) => conversionStore.handleError(p),
      onCancelled: () => conversionStore.handleCancelled(),
      onFileDrop: (paths, folderName) => {
        if (appStore.screen === "setup") addFiles(paths, folderName);
      },
    });

    window.addEventListener("keydown", handleKeydown);
  });

  onDestroy(() => {
    cleanup?.();
    conversionStore.destroy();
    window.removeEventListener("keydown", handleKeydown);
  });

  /** @param {KeyboardEvent} e */
  function handleKeydown(e) {
    const mod = e.metaKey || e.ctrlKey;
    if (!mod && e.key !== "Escape") return;

    if (mod && e.key === "o" && appStore.screen === "setup") {
      e.preventDefault();
      addFilesFromBrowse();
    } else if (mod && e.key === "Backspace" && appStore.screen === "setup") {
      e.preventDefault();
      appStore.clearAll();
    } else if (mod && e.key === "Enter" && appStore.screen === "setup") {
      e.preventDefault();
      if (fileStore.count >= 1 && appStore.ffmpegOk) conversionStore.start();
    } else if (e.key === "Escape") {
      if (appStore.screen === "converting") conversionStore.cancel();
      else if (appStore.error && !appStore.dismissingError) appStore.dismissError();
    }
  }
</script>

<div class="sr-only" aria-live="polite" aria-atomic="true">{appStore.liveAnnouncement}</div>
<main>
  {#if appStore.screen === "converting"}
    <ConvertingScreen />
  {:else if appStore.screen === "complete"}
    <CompleteScreen />
  {:else}
    <SetupScreen />
  {/if}
</main>
