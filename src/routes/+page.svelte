<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy } from "svelte";
  import { fade, slide } from "svelte/transition";

  // ── State ─────────────────────────────────────────────────────────────────

  let appState = $state("setup"); // "setup" | "converting" | "complete"

  let files = $state([]);
  let coverArt = $state(null);
  let coverArtPath = $state(null);
  let metadata = $state({ title: "", artist: "", album: "", narrator: "", year: "" });
  let outputDir = $state("");
  let outputFilename = $state("audiobook");
  let bitrate = $state(64);
  let mono = $state(true);
  let mergePlan = $state(null);
  let progress = $state({ stage: "", percent: 0, message: "" });
  let outputPath = $state(null);
  let error = $state(null);
  let ffmpegOk = $state(null);
  let dragOver = $state(false);
  let draggedIndex = $state(null);
  let dropTargetIndex = $state(null);
  let probing = $state(false);
  let liveAnnouncement = $state("");
  let focusedFileIndex = $state(-1);

  // Conversion timing
  let elapsedSeconds = $state(0);
  let elapsedTimer = null;

  // Completion data
  let completionData = $state(null);

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  let unlistenProgress;
  let unlistenComplete;
  let unlistenError;
  let unlistenCancelled;
  let unlistenDrop;
  let unlistenDragOver;
  let unlistenDragLeave;

  onMount(async () => {
    try {
      await invoke("check_ffmpeg");
      ffmpegOk = true;
    } catch (e) {
      ffmpegOk = false;
      error = "ffmpeg/ffprobe not found. Please install ffmpeg.";
    }

    unlistenProgress = await listen("merge-progress", (event) => {
      progress = event.payload;
    });

    unlistenComplete = await listen("merge-complete", (event) => {
      const path = event.payload;
      outputPath = path;
      stopTimer();
      completionData = {
        filename: path.split("/").pop(),
        elapsed: elapsedSeconds,
        fileCount: files.length,
        totalDuration: totalDuration(),
      };
      appState = "complete";
      announce("Audiobook created successfully");
    });

    unlistenError = await listen("merge-error", (event) => {
      stopTimer();
      error = String(event.payload);
      appState = "setup";
      announce("Conversion failed: " + String(event.payload));
    });

    unlistenCancelled = await listen("merge-cancelled", () => {
      stopTimer();
      error = "Conversion cancelled";
      appState = "setup";
    });

    // Tauri file drop events (OS file drag-and-drop)
    unlistenDrop = await listen("tauri://drag-drop", async (event) => {
      dragOver = false;
      if (appState !== "setup") return;
      const paths = event.payload.paths || [];
      if (paths.length > 0) {
        try {
          const result = await invoke("resolve_audio_paths", { paths });
          if (result.paths.length > 0) {
            await addFiles(result.paths, result.folder_name);
          }
        } catch (e) {
          error = String(e);
        }
      }
    });

    unlistenDragOver = await listen("tauri://drag-over", () => {
      dragOver = true;
    });

    unlistenDragLeave = await listen("tauri://drag-leave", () => {
      dragOver = false;
    });

    // Keyboard shortcuts
    window.addEventListener("keydown", handleKeydown);
  });

  onDestroy(() => {
    if (unlistenProgress) unlistenProgress();
    if (unlistenComplete) unlistenComplete();
    if (unlistenError) unlistenError();
    if (unlistenCancelled) unlistenCancelled();
    if (unlistenDrop) unlistenDrop();
    if (unlistenDragOver) unlistenDragOver();
    if (unlistenDragLeave) unlistenDragLeave();
    window.removeEventListener("keydown", handleKeydown);
    stopTimer();
  });

  function handleKeydown(e) {
    // Cmd+O / Ctrl+O — add files
    if ((e.metaKey || e.ctrlKey) && e.key === "o" && appState === "setup") {
      e.preventDefault();
      browseFiles();
    }
    // Cmd+Backspace / Ctrl+Backspace — clear all files
    if ((e.metaKey || e.ctrlKey) && e.key === "Backspace" && appState === "setup") {
      e.preventDefault();
      clearAll();
    }
    // Cmd+Enter / Ctrl+Enter — start conversion
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && appState === "setup") {
      e.preventDefault();
      if (files.length >= 1 && ffmpegOk) {
        startConvert();
      }
    }
    // Escape — dismiss errors or cancel conversion
    if (e.key === "Escape") {
      if (appState === "converting") {
        cancelConvert();
      } else if (error) {
        error = null;
      }
    }
  }

  // ── Timer ───────────────────────────────────────────────────────────────

  function startTimer() {
    elapsedSeconds = 0;
    elapsedTimer = setInterval(() => {
      elapsedSeconds += 1;
    }, 1000);
  }

  function stopTimer() {
    if (elapsedTimer) {
      clearInterval(elapsedTimer);
      elapsedTimer = null;
    }
  }

  // ── File handling ─────────────────────────────────────────────────────────

  async function addFiles(paths, folderName = null) {
    error = null;
    probing = true;
    try {
      const result = await invoke("probe_files", { paths });
      const probed = result.files;
      if (result.warnings && result.warnings.length > 0) {
        error = result.warnings.join("\n");
      }
      files = [...files, ...probed];
      if (probed.length > 0) announce(`${probed.length} file${probed.length !== 1 ? "s" : ""} added`);

      if (files.length > 0 && !outputDir) {
        const firstPath = files[0].path;
        const parts = firstPath.split("/");
        parts.pop();
        outputDir = parts.join("/");
      }

      if (files.length > 0 && metadata.title === "") {
        const first = files[0];
        metadata = {
          title: first.album || first.title || "",
          artist: first.artist || "",
          album: first.album || "",
          narrator: first.narrator || "",
          year: first.year || "",
        };
        if (!outputFilename || outputFilename === "audiobook") {
          // Prefer folder name, then metadata
          const name = folderName || first.album || first.title || "audiobook";
          outputFilename = name.replace(/[/\\:*?"<>|]/g, "");
        }
      }

      // Get cover art
      if (!coverArt) {
        const art = await invoke("get_cover_art", { paths: files.map(f => f.path) });
        if (art) {
          coverArt = art.data_uri;
          coverArtPath = art.file_path;
        }
      }

      // Get merge plan
      if (files.length >= 1) {
        mergePlan = await invoke("get_merge_plan", { paths: files.map(f => f.path) });
      } else {
        mergePlan = null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      probing = false;
    }
  }

  async function browseFiles() {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Audio", extensions: ["mp3", "m4a", "m4b", "aac"] }],
    });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      await addFiles(paths);
    }
  }

  async function browseFolders() {
    const selected = await open({ directory: true, multiple: true });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      try {
        const result = await invoke("resolve_audio_paths", { paths });
        if (result.paths.length > 0) {
          await addFiles(result.paths, result.folder_name);
        }
      } catch (e) {
        error = String(e);
      }
    }
  }

  async function browseOutputDir() {
    const selected = await open({ directory: true });
    if (selected) outputDir = selected;
  }

  async function chooseCoverArt() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png"] }],
    });
    if (selected) {
      try {
        const art = await invoke("set_custom_cover_art", { path: selected });
        coverArt = art.data_uri;
        coverArtPath = art.file_path;
      } catch (e) {
        error = String(e);
      }
    }
  }

  function handleDrop(e) {
    e.preventDefault();
    dragOver = false;
  }

  function clearAll() {
    files = [];
    coverArt = null;
    coverArtPath = null;
    metadata = { title: "", artist: "", album: "", narrator: "", year: "" };
    outputFilename = "audiobook";
    mergePlan = null;
    outputPath = null;
    error = null;
  }

  function removeFile(index) {
    files = files.filter((_, i) => i !== index);
    updateMergePlan();
  }

  function updateChapterName(index, name) {
    files = files.map((f, i) => i === index ? { ...f, chapter_name: name } : f);
  }

  async function updateMergePlan() {
    if (files.length >= 1) {
      try {
        mergePlan = await invoke("get_merge_plan", { paths: files.map(f => f.path) });
      } catch (e) {
        mergePlan = null;
      }
    } else {
      mergePlan = null;
    }
  }

  // ── Drag reorder ──────────────────────────────────────────────────────────

  function dragStart(index) {
    draggedIndex = index;
  }

  function dragOverItem(e, index) {
    e.preventDefault();
    if (draggedIndex === null || draggedIndex === index) return;
    dropTargetIndex = index;
    const newFiles = [...files];
    const [moved] = newFiles.splice(draggedIndex, 1);
    newFiles.splice(index, 0, moved);
    files = newFiles;
    draggedIndex = index;
  }

  function dragEnd() {
    draggedIndex = null;
    dropTargetIndex = null;
  }

  // ── Convert ───────────────────────────────────────────────────────────────

  async function startConvert() {
    if (files.length < 1 || !outputDir || !outputFilename) return;
    error = null;
    outputPath = null;

    // Run preflight checks before starting conversion
    try {
      const preflight = await invoke("preflight_check", {
        files: files.map(f => f.path),
        outputDir,
        outputFilename,
      });
      if (!preflight.ok) {
        error = preflight.errors.join("\n");
        return;
      }
      if (preflight.warnings.length > 0) {
        error = preflight.warnings.join("\n");
        // Show warnings but allow proceeding
      }
    } catch (e) {
      error = String(e);
      return;
    }

    progress = { stage: "preparing", percent: 0, message: "Starting\u2026" };

    appState = "converting";
    startTimer();
    announce("Conversion started");

    const config = {
      files: files.map(f => ({ path: f.path, chapter_name: f.chapter_name })),
      output_dir: outputDir,
      output_filename: outputFilename,
      title: metadata.title || null,
      artist: metadata.artist || null,
      album: metadata.album || null,
      narrator: metadata.narrator || null,
      year: metadata.year || null,
      cover_art_path: coverArtPath,
      bitrate,
      mono,
    };

    try {
      await invoke("merge_audiobook", { config });
    } catch (e) {
      // If invoke itself fails (not the async operation), handle immediately
      stopTimer();
      error = String(e);
      appState = "setup";
    }
  }

  async function cancelConvert() {
    try {
      await invoke("cancel_merge");
    } catch {
      // ignore
    }
  }

  async function revealInFinder() {
    if (outputPath) {
      try {
        const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
        await revealItemInDir(outputPath);
      } catch {
        // fallback: ignore
      }
    }
  }

  function convertAnother() {
    files = [];
    coverArt = null;
    coverArtPath = null;
    metadata = { title: "", artist: "", album: "", narrator: "", year: "" };
    outputFilename = "audiobook";
    outputDir = "";
    mergePlan = null;
    outputPath = null;
    error = null;
    completionData = null;
    progress = { stage: "", percent: 0, message: "" };
    elapsedSeconds = 0;
    appState = "setup";
  }

  // ── Helpers ───────────────────────────────────────────────────────────────

  function formatDuration(seconds) {
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  function formatDurationHuman(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  function formatElapsed(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  }

  function totalDuration() {
    return files.reduce((sum, f) => sum + f.duration, 0);
  }

  function estimateFileSize(durationSec, bitrateKbps) {
    let effectiveBps;
    if (mergePlan?.strategy === "remux") {
      // Use actual average bitrate from source files (bitrate is in bps)
      const totalBitrate = files.reduce((sum, f) => sum + (f.bitrate || 0), 0);
      effectiveBps = files.length > 0 ? totalBitrate / files.length : bitrateKbps * 1000;
    } else {
      effectiveBps = bitrateKbps * 1000;
    }
    const bytes = (effectiveBps / 8) * durationSec;
    if (bytes >= 1073741824) return `~${(bytes / 1073741824).toFixed(1)} GB`;
    if (bytes >= 1048576) return `~${(bytes / 1048576).toFixed(0)} MB`;
    return `~${(bytes / 1024).toFixed(0)} KB`;
  }

  function announce(msg) {
    liveAnnouncement = "";
    requestAnimationFrame(() => { liveAnnouncement = msg; });
  }

  function handleFileListKeydown(e) {
    if (files.length === 0) return;
    const focusEl = (sel) => /** @type {HTMLElement|null} */ (document.querySelector(sel))?.focus();
    if (e.key === "ArrowDown") {
      e.preventDefault();
      focusedFileIndex = Math.min(focusedFileIndex + 1, files.length - 1);
      focusEl(`.file-item[data-index="${focusedFileIndex}"]`);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      focusedFileIndex = Math.max(focusedFileIndex - 1, 0);
      focusEl(`.file-item[data-index="${focusedFileIndex}"]`);
    } else if (e.key === "Enter" && focusedFileIndex >= 0) {
      e.preventDefault();
      focusEl(`.file-item[data-index="${focusedFileIndex}"] .chapter-name`);
    } else if ((e.key === "Delete" || e.key === "Backspace") && focusedFileIndex >= 0) {
      if (/** @type {HTMLElement} */ (e.target).tagName === "INPUT") return;
      e.preventDefault();
      const idx = focusedFileIndex;
      removeFile(idx);
      announce(`Removed chapter ${idx + 1}`);
      focusedFileIndex = Math.min(idx, files.length - 2);
      requestAnimationFrame(() => {
        focusEl(`.file-item[data-index="${focusedFileIndex}"]`);
      });
    }
  }

  let needsTranscode = $derived(mergePlan && mergePlan.needs_transcode.length > 0);
  let strategyLabel = $derived(
    mergePlan?.strategy === "remux" ? "Lossless remux (no re-encoding)" :
    mergePlan?.strategy === "transcode_mp3" ? "Transcode all to AAC" :
    "Transcode non-AAC files to AAC"
  );
</script>

<div class="sr-only" aria-live="polite" aria-atomic="true">{liveAnnouncement}</div>

<main>
  {#if appState === "converting"}
    <!-- ── Converting screen ─────────────────────────────────────────── -->
    <div class="converting-screen" in:fade={{ duration: 300 }}>
      <header class="converting-header">
        <h1>Bindery</h1>
      </header>

      <div class="converting-content">
        <div class="converting-progress">
          <p class="converting-percent"><span class="percent-value">{Math.round(progress.percent)}</span>%</p>
          <div class="converting-bar-track" role="progressbar" aria-valuenow={Math.round(progress.percent)} aria-valuemin={0} aria-valuemax={100} aria-label="Conversion progress">
            <div class="converting-bar-fill" style="width: {progress.percent}%"></div>
          </div>
          <p class="converting-message">{progress.message}</p>
          <p class="converting-elapsed">{formatElapsed(elapsedSeconds)}</p>
        </div>

        <button class="btn-cancel" onclick={cancelConvert} aria-label="Cancel conversion">Cancel</button>
      </div>
    </div>

  {:else if appState === "complete"}
    <!-- ── Complete screen ───────────────────────────────────────────── -->
    <div class="complete-screen" in:fade={{ duration: 300 }}>
      <header class="complete-header">
        <h1>Bindery</h1>
      </header>

      <div class="complete-content">
        <div class="complete-icon">
          <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
            <circle class="check-circle" cx="24" cy="24" r="22" stroke="var(--success)" stroke-width="2" fill="color-mix(in srgb, var(--success) 10%, transparent)"/>
            <path class="check-path" d="M15 24l6 6 12-12" stroke="var(--success)" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
          </svg>
        </div>

        <h2 class="complete-title">Audiobook created</h2>

        {#if completionData}
          <p class="complete-filename">{completionData.filename}</p>
          <div class="complete-stats">
            <span>{completionData.fileCount} file{completionData.fileCount !== 1 ? "s" : ""}</span>
            <span class="complete-stat-sep">&middot;</span>
            <span>{formatDurationHuman(completionData.totalDuration)}</span>
            <span class="complete-stat-sep">&middot;</span>
            <span>{formatElapsed(completionData.elapsed)}</span>
          </div>
        {/if}

        <div class="complete-actions">
          <button class="btn-reveal-complete" onclick={revealInFinder}>Open folder</button>
          <button class="btn-another" onclick={convertAnother}>Convert another</button>
        </div>
      </div>
    </div>

  {:else}
    <!-- ── Setup screen ──────────────────────────────────────────────── -->
    <header>
      <h1>Bindery</h1>
      {#if files.length > 0}
        <span class="file-count">
          {files.length} file{files.length !== 1 ? "s" : ""} · {formatDurationHuman(totalDuration())} · {estimateFileSize(totalDuration(), bitrate)}
        </span>
      {/if}
    </header>

    {#if error}
      <div class="error-banner" transition:slide>
        <div class="error-content">
          {#each error.split("\n") as line}
            <span class="error-line">{line}</span>
          {/each}
          {#if error.includes("disk space") || error.includes("Permission denied") || error.includes("moved or deleted")}
            <span class="error-retry">Fix the issue above, then try again.</span>
          {/if}
        </div>
        <button class="error-dismiss" onclick={() => error = null} title="Dismiss (Esc)" aria-label="Dismiss error">×</button>
      </div>
    {/if}

    {#if probing && files.length === 0}
      <!-- Loading state while probing -->
      <div class="drop-zone probing-state">
        <div class="spinner"></div>
        <p class="drop-title">Reading files…</p>
        <p class="drop-subtitle">Probing audio metadata</p>
      </div>
    {:else if files.length === 0}
      <!-- Drop zone -->
      <div
        class="drop-zone"
        in:fade={{ duration: 200, delay: 100 }}
        class:drag-over={dragOver}
        role="region"
        aria-label="Drop audio files here or click to browse"
        tabindex="0"
        onclick={browseFiles}
        onkeydown={(e) => e.key === "Enter" && browseFiles()}
        ondragover={(e) => { e.preventDefault(); dragOver = true; }}
        ondragleave={() => dragOver = false}
        ondrop={handleDrop}
      >
        <div class="drop-icon">
          <svg width="56" height="56" viewBox="0 0 56 56" fill="none">
            <!-- Book spine -->
            <path d="M14 8c0-1.1.9-2 2-2h4v44h-4a2 2 0 01-2-2V8z" stroke="currentColor" stroke-width="1.5" fill="none"/>
            <!-- Book cover -->
            <path d="M20 6h20c1.1 0 2 .9 2 2v40c0 1.1-.9 2-2 2H20V6z" stroke="currentColor" stroke-width="1.5" fill="none"/>
            <!-- Audio wave lines -->
            <path d="M27 22v12M31 18v20M35 24v8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
        </div>
        <p class="drop-title">Drop audio files or folders here</p>
        <p class="drop-subtitle">MP3, M4A, M4B, AAC</p>
        <div class="drop-buttons">
          <button class="btn-drop-browse" onclick={(e) => { e.stopPropagation(); browseFiles(); }}>Browse files</button>
          <button class="btn-drop-browse" onclick={(e) => { e.stopPropagation(); browseFolders(); }}>Browse folders</button>
        </div>
        <p class="drop-hint">{navigator.platform?.includes("Mac") ? "⌘" : "Ctrl"}+O to open files</p>
      </div>
    {:else}
      <div class="content">
        <!-- File list -->
        <section class="panel file-list">
          <div class="panel-header">
            <h2>Chapters</h2>
            <div class="panel-actions">
              {#if probing}
                <span class="spinner-inline"></span>
              {/if}
              <button class="btn-text" onclick={browseFiles}>+ Add files</button>
              <button class="btn-text" onclick={browseFolders}>+ Add folder</button>
              <button class="btn-text btn-text-danger" onclick={clearAll}>Clear all</button>
            </div>
          </div>
          <div class="file-items" role="list" aria-label="Chapter list" onkeydown={handleFileListKeydown}>
            {#each files as file, i (file.path)}
              <div
                class="file-item"
                class:dragging={draggedIndex === i}
                class:drop-target={dropTargetIndex === i && draggedIndex !== i}
                draggable="true"
                role="listitem"
                tabindex={i === focusedFileIndex ? 0 : -1}
                data-index={i}
                aria-label="Chapter {i + 1}: {file.chapter_name}"
                ondragstart={() => dragStart(i)}
                ondragover={(e) => dragOverItem(e, i)}
                ondragend={dragEnd}
                onfocus={() => focusedFileIndex = i}
                in:fade={{ duration: 150 }}
                out:fade={{ duration: 100 }}
              >
                <span class="drag-handle" title="Drag to reorder" aria-hidden="true">⠿</span>
                <span class="file-number" aria-hidden="true">{i + 1}</span>
                <input
                  class="chapter-name"
                  type="text"
                  value={file.chapter_name}
                  oninput={(e) => updateChapterName(i, e.target.value)}
                  aria-label="Chapter {i + 1} name"
                />
                <span class="codec-badge" class:codec-aac={file.codec === "aac"} class:codec-mp3={file.codec === "mp3"} aria-label="{file.codec.toUpperCase()} format">
                  {file.codec.toUpperCase()}
                </span>
                <span class="file-duration">{formatDuration(file.duration)}</span>
                <button class="btn-remove" onclick={() => removeFile(i)} title="Remove" aria-label="Remove chapter {i + 1}">×</button>
              </div>
            {/each}
          </div>
        </section>

        <!-- Metadata + Settings two-column layout -->
        <div class="two-col">
          <!-- Metadata panel -->
          <section class="panel metadata-panel">
            <h2>Metadata</h2>
            <div class="metadata-content">
              <div class="cover-art-container">
                {#if coverArt}
                  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                  <img class="cover-art" src={coverArt} alt="Cover art — click to change" tabindex="0" role="button" aria-label="Change cover art" onclick={chooseCoverArt} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); chooseCoverArt(); } }} />
                  <button class="btn-remove-cover" onclick={(e) => { e.stopPropagation(); coverArt = null; coverArtPath = null; }} title="Remove cover art" aria-label="Remove cover art">×</button>
                {:else}
                  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                  <div class="cover-placeholder" tabindex="0" role="button" aria-label="Choose cover art" onclick={chooseCoverArt} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); chooseCoverArt(); } }}>
                    <svg width="32" height="32" viewBox="0 0 32 32" fill="none">
                      <rect x="4" y="4" width="24" height="24" rx="2" stroke="currentColor" stroke-width="1.5" fill="none"/>
                      <circle cx="12" cy="13" r="3" stroke="currentColor" stroke-width="1.5" fill="none"/>
                      <path d="M4 22l6-6 4 4 4-4 10 10" stroke="currentColor" stroke-width="1.5" fill="none"/>
                    </svg>
                  </div>
                {/if}
              </div>
              <div class="metadata-fields">
                <label>
                  <span>Title</span>
                  <input type="text" bind:value={metadata.title} placeholder="Audiobook title" />
                </label>
                <label>
                  <span>Author</span>
                  <input type="text" bind:value={metadata.artist} placeholder="Author name" />
                </label>
                <label>
                  <span>Album</span>
                  <input type="text" bind:value={metadata.album} placeholder="Album / Series" />
                </label>
                <label>
                  <span>Narrator</span>
                  <input type="text" bind:value={metadata.narrator} placeholder="Narrator name" />
                </label>
                <label>
                  <span>Year</span>
                  <input type="text" bind:value={metadata.year} placeholder="Year" />
                </label>
              </div>
            </div>
          </section>

          <!-- Output settings -->
          <section class="panel output-panel">
            <h2>Output</h2>
            <div class="output-fields">
              <label class="output-dir">
                <span>Folder</span>
                <div class="dir-input">
                  <input type="text" bind:value={outputDir} placeholder="Output folder" />
                  <button class="btn-browse" onclick={browseOutputDir}>Browse</button>
                </div>
              </label>
              <label>
                <span>Filename</span>
                <div class="filename-input">
                  <input type="text" bind:value={outputFilename} placeholder="output" />
                  <span class="ext">.m4b</span>
                </div>
              </label>
            </div>
          </section>
        </div>

        <!-- Encoding settings (shown when transcoding needed) -->
        {#if needsTranscode}
          <section class="panel encoding-panel">
            <h2>Encoding</h2>
            <p class="transcode-notice">{strategyLabel}</p>
            {#if mergePlan.strategy !== "remux"}
              <div class="encoding-fields">
                <label>
                  <span>Bitrate</span>
                  <select bind:value={bitrate}>
                    <option value={64}>64 kbps</option>
                    <option value={96}>96 kbps</option>
                    <option value={128}>128 kbps</option>
                    <option value={192}>192 kbps</option>
                    <option value={256}>256 kbps</option>
                    <option value={320}>320 kbps</option>
                  </select>
                </label>
                <label class="checkbox-label">
                  <input type="checkbox" bind:checked={mono} />
                  <span>Mono (recommended for spoken word)</span>
                </label>
              </div>
              <div class="transcode-files">
                <span class="transcode-label">Files to transcode:</span>
                {#each mergePlan.needs_transcode as path}
                  <span class="transcode-file">{path.split("/").pop()}</span>
                {/each}
              </div>
            {/if}
          </section>
        {/if}

        <!-- Convert button -->
        <div class="convert-section">
          <button
            class="btn-convert"
            onclick={startConvert}
            disabled={files.length < 1 || !ffmpegOk}
          >
            {needsTranscode ? "Bind audiobook (transcoding)" : "Bind audiobook"}
          </button>
          <p class="shortcut-hint">{navigator.platform?.includes("Mac") ? "⌘" : "Ctrl"}+↵</p>
        </div>
      </div>
    {/if}
  {/if}
</main>

<style>
  :root {
    --bg: #FAFAF8;
    --surface: #F2F0EC;
    --border: #E2DFD9;
    --text: #1A1918;
    --text-secondary: #706B64;
    --accent: #C67B30;
    --accent-hover: #A86520;
    --success: #4A7C59;
    --error: #C45D4E;
    --radius: 8px;
    --radius-lg: 12px;
    --font: "Inter", system-ui, -apple-system, sans-serif;
    --transition: 150ms ease-out;
  }

  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #1A1918;
      --surface: #242320;
      --border: #3A3835;
      --text: #EDECEA;
      --text-secondary: #918C85;
      --accent: #D4893A;
      --accent-hover: #E09A4A;
      --success: #5A9A6A;
      --error: #D46E5E;
    }
  }

  :global(body) {
    margin: 0;
    padding: 0;
    background: var(--bg);
    color: var(--text);
    font-family: var(--font);
    font-size: 13px;
    line-height: 1.5;
    -webkit-font-smoothing: antialiased;
    overflow: hidden;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 24px;
    box-sizing: border-box;
    gap: 24px;
    overflow-y: auto;
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 0 4px;
    flex-shrink: 0;
  }

  h1 {
    font-family: "Instrument Serif", Georgia, serif;
    font-size: 20px;
    font-weight: 400;
    margin: 0;
    letter-spacing: -0.02em;
  }

  .file-count {
    font-size: 12px;
    color: var(--text-secondary);
  }

  /* ── Error banner ──────────────────────────────────────────────────── */

  .error-banner {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    background: color-mix(in srgb, var(--error) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 30%, transparent);
    color: var(--error);
    padding: 8px 12px;
    border-radius: var(--radius);
    font-size: 12px;
    flex-shrink: 0;
    animation: fadeIn 150ms ease-out;
  }

  .error-content {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .error-line {
    display: block;
  }

  .error-retry {
    display: block;
    margin-top: 4px;
    opacity: 0.7;
    font-style: italic;
  }

  .error-dismiss {
    background: none;
    border: none;
    color: var(--error);
    cursor: pointer;
    font-size: 16px;
    padding: 0 4px;
    border-radius: 4px;
    transition: background var(--transition);
  }

  .error-dismiss:hover {
    background: color-mix(in srgb, var(--error) 15%, transparent);
  }

  /* ── Drop zone ─────────────────────────────────────────────────────── */

  .drop-zone {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: all var(--transition);
    gap: 8px;
    min-height: 240px;
  }

  .drop-zone:hover {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 4%, transparent);
  }

  .drop-zone.drag-over {
    border-color: var(--accent);
    border-style: solid;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    transform: scale(1.005);
  }

  .drop-zone.probing-state {
    cursor: default;
  }

  .drop-zone:hover .drop-icon, .drop-zone.drag-over .drop-icon {
    color: var(--accent);
    opacity: 0.7;
    transform: translateY(-2px);
  }

  .drop-icon {
    color: var(--text-secondary);
    opacity: 0.5;
    transition: all var(--transition);
  }

  .drop-title {
    font-size: 15px;
    font-weight: 500;
    margin: 0;
    color: var(--text);
  }

  .drop-subtitle {
    font-size: 12px;
    color: var(--text-secondary);
    margin: 0;
  }

  .drop-buttons {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .btn-drop-browse {
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--accent);
    font-family: var(--font);
    font-size: 12px;
    font-weight: 500;
    padding: 6px 14px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background var(--transition), border-color var(--transition);
  }

  .btn-drop-browse:hover {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, var(--surface));
  }

  .drop-hint {
    font-size: 11px;
    color: var(--text-secondary);
    margin: 8px 0 0 0;
    opacity: 0.6;
  }

  /* ── Content layout ────────────────────────────────────────────────── */

  .content {
    display: flex;
    flex-direction: column;
    gap: 24px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .two-col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
  }

  @media (max-width: 640px) {
    .two-col {
      grid-template-columns: 1fr;
    }
  }

  /* ── Panel ─────────────────────────────────────────────────────────── */

  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 12px 16px;
  }

  .panel h2 {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-secondary);
    margin: 0 0 8px 0;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
  }

  .panel-header h2 { margin: 0; }

  .panel-actions {
    display: flex;
    gap: 8px;
  }

  .btn-text {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 12px;
    font-weight: 500;
    padding: 6px 10px;
    min-height: 32px;
    border-radius: var(--radius);
    transition: background var(--transition), color var(--transition);
  }

  .btn-text:hover {
    color: var(--accent-hover);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  .btn-text-danger {
    color: var(--text-secondary);
  }

  .btn-text-danger:hover {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 10%, transparent);
  }

  /* ── File list ─────────────────────────────────────────────────────── */

  .file-items {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius);
    cursor: grab;
    transition: background var(--transition), opacity var(--transition);
    position: relative;
  }

  .file-item:hover {
    background: color-mix(in srgb, var(--border) 50%, transparent);
  }

  .file-item.dragging {
    opacity: 0.4;
  }

  .file-item.drop-target::before {
    content: "";
    position: absolute;
    top: -2px;
    left: 8px;
    right: 8px;
    height: 2px;
    background: var(--accent);
    border-radius: 1px;
  }

  .drag-handle {
    color: var(--text-secondary);
    font-size: 14px;
    cursor: grab;
    user-select: none;
    opacity: 0.4;
    min-width: 28px;
    min-height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    transition: opacity var(--transition), background var(--transition);
  }

  .file-item:hover .drag-handle { opacity: 0.7; }
  .file-item:focus-within .drag-handle { opacity: 0.6; }

  .drag-handle:hover {
    opacity: 1 !important;
    background: color-mix(in srgb, var(--border) 60%, transparent);
  }

  .file-number {
    color: var(--text-secondary);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    width: 18px;
    text-align: right;
    flex-shrink: 0;
  }

  .chapter-name {
    flex: 1;
    background: none;
    border: 1px solid transparent;
    color: var(--text);
    font-family: var(--font);
    font-size: 13px;
    padding: 4px 6px;
    border-radius: 4px;
    outline: none;
    min-width: 0;
    max-width: 100%;
    text-overflow: ellipsis;
    overflow: hidden;
    white-space: nowrap;
    transition: border-color var(--transition), background var(--transition);
  }

  .chapter-name:hover { border-color: var(--border); }

  .chapter-name:focus {
    border-color: var(--accent);
    background: var(--bg);
    text-overflow: clip;
    overflow: visible;
  }

  .codec-badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    text-transform: uppercase;
    flex-shrink: 0;
  }

  .codec-aac {
    background: color-mix(in srgb, var(--success) 15%, transparent);
    color: var(--success);
  }

  .codec-mp3 {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--accent);
  }

  .file-duration {
    color: var(--text-secondary);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
    width: 40px;
    text-align: right;
  }

  .btn-remove {
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 16px;
    padding: 0;
    min-width: 32px;
    min-height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    opacity: 0;
    transition: opacity var(--transition), background var(--transition), color var(--transition);
  }

  .file-item:hover .btn-remove { opacity: 1; }
  .btn-remove:hover {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 12%, transparent);
  }

  /* ── Metadata ──────────────────────────────────────────────────────── */

  .metadata-content {
    display: flex;
    gap: 16px;
  }

  .cover-art-container {
    position: relative;
    flex-shrink: 0;
    width: 80px;
    height: 80px;
  }

  .cover-art-container:hover .btn-remove-cover { opacity: 1; }

  .btn-remove-cover {
    position: absolute;
    top: -6px;
    right: -6px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity var(--transition), background var(--transition), color var(--transition);
    z-index: 1;
  }

  .btn-remove-cover:hover {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 12%, transparent);
    border-color: var(--error);
  }

  .cover-art {
    width: 80px;
    height: 80px;
    border-radius: var(--radius);
    object-fit: cover;
    flex-shrink: 0;
    border: 1px solid var(--border);
    cursor: pointer;
    transition: border-color var(--transition), box-shadow var(--transition);
  }

  .cover-art:hover {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
  }

  .cover-placeholder {
    width: 80px;
    height: 80px;
    border-radius: var(--radius);
    background: var(--bg);
    border: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    opacity: 0.5;
    flex-shrink: 0;
    cursor: pointer;
    transition: border-color var(--transition), opacity var(--transition);
  }

  .cover-placeholder:hover {
    border-color: var(--accent);
    opacity: 0.8;
  }

  .metadata-fields {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .metadata-fields label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .metadata-fields label span {
    font-size: 11px;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .metadata-fields input {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 13px;
    padding: 6px 8px;
    border-radius: 4px;
    outline: none;
    min-width: 0;
    height: 36px;
    box-sizing: border-box;
    transition: border-color var(--transition);
  }

  .metadata-fields input:hover { border-color: color-mix(in srgb, var(--accent) 40%, var(--border)); }
  .metadata-fields input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 15%, transparent);
  }

  /* ── Output fields ─────────────────────────────────────────────────── */

  .output-fields {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .output-fields label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .output-fields label > span {
    font-size: 11px;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .dir-input { display: flex; gap: 6px; }

  .dir-input input {
    flex: 1;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 13px;
    padding: 6px 8px;
    border-radius: 4px;
    outline: none;
    min-width: 0;
    height: 36px;
    box-sizing: border-box;
    transition: border-color var(--transition);
  }

  .dir-input input:hover { border-color: color-mix(in srgb, var(--accent) 40%, var(--border)); }
  .dir-input input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 15%, transparent);
  }

  .btn-browse {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 12px;
    font-weight: 500;
    padding: 6px 12px;
    border-radius: 4px;
    cursor: pointer;
    flex-shrink: 0;
    height: 36px;
    box-sizing: border-box;
    transition: border-color var(--transition), background var(--transition);
  }

  .btn-browse:hover {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 6%, var(--bg));
  }

  .btn-browse:active {
    background: color-mix(in srgb, var(--accent) 12%, var(--bg));
  }

  .filename-input {
    display: flex;
    align-items: center;
  }

  .filename-input input {
    flex: 1;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 13px;
    padding: 6px 8px;
    border-radius: 4px 0 0 4px;
    border-right: none;
    outline: none;
    min-width: 0;
    height: 36px;
    box-sizing: border-box;
    transition: border-color var(--transition);
  }

  .filename-input input:hover { border-color: color-mix(in srgb, var(--accent) 40%, var(--border)); }
  .filename-input input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 15%, transparent);
  }

  .filename-input .ext {
    background: var(--border);
    color: var(--text-secondary);
    padding: 6px 8px;
    font-size: 12px;
    border-radius: 0 4px 4px 0;
    border: 1px solid var(--border);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    height: 36px;
    box-sizing: border-box;
  }

  /* ── Encoding ──────────────────────────────────────────────────────── */

  .transcode-notice {
    font-size: 12px;
    color: var(--text-secondary);
    margin: 0 0 8px 0;
  }

  .encoding-fields {
    display: flex;
    gap: 16px;
    align-items: end;
    margin-bottom: 8px;
  }

  .encoding-fields label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .encoding-fields label > span {
    font-size: 11px;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .encoding-fields select {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 13px;
    padding: 6px 8px;
    border-radius: 4px;
    outline: none;
    height: 36px;
    box-sizing: border-box;
    transition: border-color var(--transition);
  }

  .encoding-fields select:hover { border-color: color-mix(in srgb, var(--accent) 40%, var(--border)); }
  .encoding-fields select:focus { border-color: var(--accent); }

  .checkbox-label {
    flex-direction: row !important;
    align-items: center !important;
    gap: 6px !important;
  }

  .checkbox-label input[type="checkbox"] { accent-color: var(--accent); }

  .transcode-files {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }

  .transcode-label {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .transcode-file {
    font-size: 11px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
    padding: 1px 6px;
    border-radius: 3px;
  }

  /* ── Convert button ─────────────────────────────────────────────────── */

  .convert-section {
    padding: 4px 0;
    flex-shrink: 0;
  }

  .btn-convert {
    width: 100%;
    background: var(--accent);
    color: white;
    border: none;
    font-family: var(--font);
    font-size: 13px;
    font-weight: 500;
    padding: 0 24px;
    height: 36px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background var(--transition), transform var(--transition);
  }

  .btn-convert:hover:not(:disabled) {
    background: var(--accent-hover);
    box-shadow: 0 2px 8px color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .btn-convert:active:not(:disabled) { transform: scale(0.99); }
  .btn-convert:disabled { opacity: 0.5; cursor: not-allowed; }

  .shortcut-hint {
    font-size: 11px;
    color: var(--text-secondary);
    opacity: 0.5;
    text-align: center;
    margin: 6px 0 0;
  }

  /* ── Converting screen ─────────────────────────────────────────────── */

  .converting-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 0;
  }

  .converting-header {
    padding: 0 4px;
    flex-shrink: 0;
  }

  .converting-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 32px;
  }

  .converting-progress {
    width: 100%;
    max-width: 480px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .converting-percent {
    font-size: 32px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--text);
    margin: 0;
    letter-spacing: -0.02em;
  }

  .percent-value {
    display: inline-block;
    transition: transform 0.3s ease-out;
  }

  .converting-bar-track {
    width: 100%;
    height: 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow: hidden;
  }

  .converting-bar-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 4px;
    transition: width 0.5s ease-out;
    animation: barPulse 2s ease-in-out infinite;
  }

  @keyframes barPulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.75; }
  }

  .converting-message {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 0;
    text-align: center;
  }

  .converting-elapsed {
    font-size: 12px;
    color: var(--text-secondary);
    opacity: 0.6;
    margin: 0;
    font-variant-numeric: tabular-nums;
  }

  .btn-cancel {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    font-family: var(--font);
    font-size: 13px;
    font-weight: 500;
    padding: 8px 32px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background var(--transition), border-color var(--transition), color var(--transition);
  }

  .btn-cancel:hover {
    border-color: var(--error);
    color: var(--error);
    background: color-mix(in srgb, var(--error) 8%, transparent);
  }

  /* ── Complete screen ───────────────────────────────────────────────── */

  .complete-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 0;
  }

  .complete-header {
    padding: 0 4px;
    flex-shrink: 0;
  }

  .complete-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
  }

  .complete-icon {
    animation: fadeIn 300ms ease-out;
  }

  .complete-icon .check-circle {
    stroke-dasharray: 138;
    stroke-dashoffset: 138;
    animation: drawCircle 0.6s ease-out 0.1s forwards;
  }

  .complete-icon .check-path {
    stroke-dasharray: 30;
    stroke-dashoffset: 30;
    animation: drawCheck 0.4s ease-out 0.5s forwards;
  }

  @keyframes drawCircle {
    to { stroke-dashoffset: 0; }
  }

  @keyframes drawCheck {
    to { stroke-dashoffset: 0; }
  }

  .complete-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--text);
    margin: 0;
  }

  .complete-filename {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 0;
    background: var(--surface);
    padding: 6px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
  }

  .complete-stats {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12px;
    color: var(--text-secondary);
  }

  .complete-stat-sep {
    opacity: 0.4;
  }

  .complete-actions {
    display: flex;
    gap: 12px;
    margin-top: 8px;
  }

  .btn-reveal-complete {
    background: var(--success);
    color: white;
    border: none;
    font-family: var(--font);
    font-size: 13px;
    font-weight: 500;
    padding: 8px 24px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: filter var(--transition), transform var(--transition);
  }

  .btn-reveal-complete:hover { filter: brightness(1.1); }
  .btn-reveal-complete:active { transform: scale(0.98); }

  .btn-another {
    background: none;
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 13px;
    font-weight: 500;
    padding: 8px 24px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background var(--transition), border-color var(--transition);
  }

  .btn-another:hover {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }

  /* ── Spinner ──────────────────────────────────────────────────────── */

  .spinner {
    width: 32px;
    height: 32px;
    border: 2.5px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .spinner-inline {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  /* ── Animations ────────────────────────────────────────────────────── */

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── Scrollbar ─────────────────────────────────────────────────────── */

  :global(::-webkit-scrollbar) { width: 6px; }
  :global(::-webkit-scrollbar-track) { background: transparent; }
  :global(::-webkit-scrollbar-thumb) { background: var(--border); border-radius: 3px; }
  :global(::-webkit-scrollbar-thumb:hover) { background: var(--text-secondary); }

  /* ── Screen reader only ──────────────────────────────────────────── */

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  /* ── Focus-visible ───────────────────────────────────────────────── */

  .drop-zone:focus-visible {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
  }

  .btn-drop-browse:focus-visible,
  .btn-text:focus-visible,
  .btn-browse:focus-visible,
  .btn-convert:focus-visible,
  .btn-cancel:focus-visible,
  .btn-reveal-complete:focus-visible,
  .btn-another:focus-visible,
  .error-dismiss:focus-visible,
  .btn-remove:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .btn-remove:focus-visible,
  .btn-remove-cover:focus-visible {
    opacity: 1;
  }

  .cover-art:focus-visible,
  .cover-placeholder:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .btn-remove-cover:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .file-item:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }

  .chapter-name:focus-visible {
    border-color: var(--accent);
    background: var(--bg);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 15%, transparent);
  }

  .metadata-fields input:focus-visible,
  .dir-input input:focus-visible,
  .filename-input input:focus-visible {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
  }

  .encoding-fields select:focus-visible {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
  }

  .checkbox-label input[type="checkbox"]:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* ── Reduced motion ──────────────────────────────────────────────── */

  @media (prefers-reduced-motion: reduce) {
    *, *::before, *::after {
      animation-duration: 0.01ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.01ms !important;
    }

    .converting-bar-fill {
      animation: none;
    }

    .complete-icon .check-circle,
    .complete-icon .check-path {
      animation: none;
      stroke-dashoffset: 0;
    }
  }
</style>
