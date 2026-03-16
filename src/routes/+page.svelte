<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy } from "svelte";
  import { fade, slide } from "svelte/transition";

  // ── State ─────────────────────────────────────────────────────────────────

  let files = $state([]);
  let coverArt = $state(null);
  let metadata = $state({ title: "", artist: "", album: "", narrator: "", year: "" });
  let outputDir = $state("");
  let outputFilename = $state("audiobook");
  let bitrate = $state(64);
  let mono = $state(true);
  let mergePlan = $state(null);
  let converting = $state(false);
  let progress = $state({ stage: "", percent: 0, message: "" });
  let outputPath = $state(null);
  let error = $state(null);
  let ffmpegOk = $state(null);
  let dragOver = $state(false);
  let draggedIndex = $state(null);
  let dropTargetIndex = $state(null);

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  let unlistenProgress;
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

    // Tauri file drop events (OS file drag-and-drop)
    unlistenDrop = await listen("tauri://drag-drop", async (event) => {
      dragOver = false;
      const paths = event.payload.paths || [];
      const audioPaths = paths.filter(p => /\.(mp3|m4a|m4b|aac)$/i.test(p));
      if (audioPaths.length > 0) {
        await addFiles(audioPaths);
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
    if (unlistenDrop) unlistenDrop();
    if (unlistenDragOver) unlistenDragOver();
    if (unlistenDragLeave) unlistenDragLeave();
    window.removeEventListener("keydown", handleKeydown);
  });

  function handleKeydown(e) {
    // Cmd+O / Ctrl+O — add files
    if ((e.metaKey || e.ctrlKey) && e.key === "o") {
      e.preventDefault();
      browseFiles();
    }
    // Cmd+Backspace / Ctrl+Backspace — clear all files
    if ((e.metaKey || e.ctrlKey) && e.key === "Backspace") {
      e.preventDefault();
      clearAll();
    }
    // Escape — dismiss errors
    if (e.key === "Escape" && error) {
      error = null;
    }
  }

  // ── File handling ─────────────────────────────────────────────────────────

  async function addFiles(paths) {
    error = null;
    try {
      const probed = await invoke("probe_files", { paths });
      files = [...files, ...probed];

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
          outputFilename = (first.album || first.title || "audiobook").replace(/[/\\:*?"<>|]/g, "");
        }
      }

      // Get cover art
      if (!coverArt) {
        const art = await invoke("get_cover_art", { paths: files.map(f => f.path) });
        if (art) coverArt = art;
      }

      // Get merge plan
      if (files.length >= 1) {
        mergePlan = await invoke("get_merge_plan", { paths: files.map(f => f.path) });
      } else {
        mergePlan = null;
      }
    } catch (e) {
      error = String(e);
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

  async function browseOutputDir() {
    const selected = await open({ directory: true });
    if (selected) outputDir = selected;
  }

  function handleDrop(e) {
    e.preventDefault();
    dragOver = false;
  }

  function clearAll() {
    files = [];
    coverArt = null;
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
    converting = true;
    outputPath = null;
    progress = { stage: "preparing", percent: 0, message: "Starting\u2026" };

    try {
      const config = {
        files: files.map(f => ({ path: f.path, chapter_name: f.chapter_name })),
        output_dir: outputDir,
        output_filename: outputFilename,
        title: metadata.title || null,
        artist: metadata.artist || null,
        album: metadata.album || null,
        narrator: metadata.narrator || null,
        year: metadata.year || null,
        cover_art_path: null,
        bitrate,
        mono,
      };
      const result = await invoke("merge_audiobook", { config });
      outputPath = result;
    } catch (e) {
      error = String(e);
    } finally {
      converting = false;
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

  function totalDuration() {
    return files.reduce((sum, f) => sum + f.duration, 0);
  }

  function estimateFileSize(durationSec, bitrateKbps) {
    const bytes = (bitrateKbps * 1000 / 8) * durationSec;
    if (bytes >= 1073741824) return `~${(bytes / 1073741824).toFixed(1)} GB`;
    if (bytes >= 1048576) return `~${(bytes / 1048576).toFixed(0)} MB`;
    return `~${(bytes / 1024).toFixed(0)} KB`;
  }

  let needsTranscode = $derived(mergePlan && mergePlan.needs_transcode.length > 0);
  let strategyLabel = $derived(
    mergePlan?.strategy === "remux" ? "Lossless remux (no re-encoding)" :
    mergePlan?.strategy === "transcode_mp3" ? "Transcode all to AAC" :
    "Transcode non-AAC files to AAC"
  );
</script>

<main>
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
      <span>{error}</span>
      <button class="error-dismiss" onclick={() => error = null} title="Dismiss (Esc)">×</button>
    </div>
  {/if}

  {#if files.length === 0}
    <!-- Drop zone -->
    <div
      class="drop-zone"
      class:drag-over={dragOver}
      role="button"
      tabindex="0"
      onclick={browseFiles}
      onkeydown={(e) => e.key === "Enter" && browseFiles()}
      ondragover={(e) => { e.preventDefault(); dragOver = true; }}
      ondragleave={() => dragOver = false}
      ondrop={handleDrop}
    >
      <div class="drop-icon">
        <svg width="56" height="56" viewBox="0 0 56 56" fill="none">
          <path d="M10 8h22l14 14v26a3 3 0 01-3 3H10a3 3 0 01-3-3V11a3 3 0 013-3z" stroke="currentColor" stroke-width="1.5" fill="none"/>
          <path d="M32 8v14h14" stroke="currentColor" stroke-width="1.5" fill="none"/>
          <path d="M20 33h16M28 25v16" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="drop-title">Drop audio files here</p>
      <p class="drop-subtitle">or click to browse · MP3, M4A, M4B, AAC</p>
      <p class="drop-hint">{navigator.platform?.includes("Mac") ? "⌘" : "Ctrl"}+O to open files</p>
    </div>
  {:else}
    <div class="content">
      <!-- File list -->
      <section class="panel file-list">
        <div class="panel-header">
          <h2>Chapters</h2>
          <div class="panel-actions">
            <button class="btn-text" onclick={browseFiles}>+ Add files</button>
            <button class="btn-text btn-text-danger" onclick={clearAll}>Clear all</button>
          </div>
        </div>
        <div class="file-items">
          {#each files as file, i (file.path)}
            <div
              class="file-item"
              class:dragging={draggedIndex === i}
              class:drop-target={dropTargetIndex === i && draggedIndex !== i}
              draggable="true"
              role="listitem"
              ondragstart={() => dragStart(i)}
              ondragover={(e) => dragOverItem(e, i)}
              ondragend={dragEnd}
              in:fade={{ duration: 150 }}
              out:fade={{ duration: 100 }}
            >
              <span class="drag-handle" title="Drag to reorder">⠿</span>
              <span class="file-number">{i + 1}</span>
              <input
                class="chapter-name"
                type="text"
                value={file.chapter_name}
                oninput={(e) => updateChapterName(i, e.target.value)}
              />
              <span class="codec-badge" class:codec-aac={file.codec === "aac"} class:codec-mp3={file.codec === "mp3"}>
                {file.codec.toUpperCase()}
              </span>
              <span class="file-duration">{formatDuration(file.duration)}</span>
              <button class="btn-remove" onclick={() => removeFile(i)} title="Remove">×</button>
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
            {#if coverArt}
              <img class="cover-art" src={coverArt} alt="Cover art" />
            {:else}
              <div class="cover-placeholder">
                <svg width="32" height="32" viewBox="0 0 32 32" fill="none">
                  <rect x="4" y="4" width="24" height="24" rx="2" stroke="currentColor" stroke-width="1.5" fill="none"/>
                  <circle cx="12" cy="13" r="3" stroke="currentColor" stroke-width="1.5" fill="none"/>
                  <path d="M4 22l6-6 4 4 4-4 10 10" stroke="currentColor" stroke-width="1.5" fill="none"/>
                </svg>
              </div>
            {/if}
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

      <!-- Convert button / progress -->
      <div class="convert-section">
        {#if converting}
          <div class="progress-container">
            <div class="progress-bar" style="width: {progress.percent}%"></div>
            <span class="progress-label">{Math.round(progress.percent)}%</span>
          </div>
          <p class="progress-message">{progress.message}</p>
        {:else if outputPath}
          <button class="btn-reveal" onclick={revealInFinder}>
            Reveal in Finder
          </button>
          <p class="success-message">Created: {outputPath.split("/").pop()}</p>
        {:else}
          <button
            class="btn-convert"
            onclick={startConvert}
            disabled={files.length < 1 || !ffmpegOk}
          >
            Bind audiobook
          </button>
        {/if}
      </div>
    </div>
  {/if}
</main>

<style>
  :root {
    --bg: #FAFAF8;
    --surface: #F2F0EC;
    --border: #E2DFD9;
    --text: #1A1918;
    --text-secondary: #7A756E;
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
      --text-secondary: #8A857E;
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
    align-items: center;
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

  .drop-zone:hover, .drop-zone.drag-over {
    border-color: var(--accent);
    border-style: solid;
    background: color-mix(in srgb, var(--accent) 5%, transparent);
  }

  .drop-zone:hover .drop-icon, .drop-zone.drag-over .drop-icon {
    color: var(--accent);
    opacity: 0.8;
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
    font-size: 14px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
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
    padding: 2px 6px;
    border-radius: 4px;
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
    opacity: 0.3;
    width: 14px;
    text-align: center;
    transition: opacity var(--transition);
  }

  .file-item:hover .drag-handle { opacity: 0.7; }

  .file-number {
    color: var(--text-secondary);
    font-size: 11px;
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
    transition: border-color var(--transition), background var(--transition);
  }

  .chapter-name:hover { border-color: var(--border); }

  .chapter-name:focus {
    border-color: var(--accent);
    background: var(--bg);
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
    padding: 0 4px;
    opacity: 0;
    transition: opacity var(--transition), color var(--transition);
    border-radius: 4px;
  }

  .file-item:hover .btn-remove { opacity: 1; }
  .btn-remove:hover { color: var(--error); }

  /* ── Metadata ──────────────────────────────────────────────────────── */

  .metadata-content {
    display: flex;
    gap: 16px;
  }

  .cover-art {
    width: 80px;
    height: 80px;
    border-radius: var(--radius);
    object-fit: cover;
    flex-shrink: 0;
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
  .metadata-fields input:focus { border-color: var(--accent); }

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
  .dir-input input:focus { border-color: var(--accent); }

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
    background: color-mix(in srgb, var(--accent) 5%, var(--bg));
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
  .filename-input input:focus { border-color: var(--accent); }

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

  /* ── Convert / Progress ────────────────────────────────────────────── */

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

  .btn-convert:hover:not(:disabled) { background: var(--accent-hover); }
  .btn-convert:active:not(:disabled) { transform: scale(0.99); }
  .btn-convert:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-reveal {
    width: 100%;
    background: var(--success);
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

  .btn-reveal:hover { filter: brightness(1.1); }
  .btn-reveal:active { transform: scale(0.99); }

  .progress-container {
    width: 100%;
    height: 36px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    position: relative;
  }

  .progress-bar {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s linear;
    border-radius: var(--radius);
  }

  .progress-label {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
    mix-blend-mode: difference;
    pointer-events: none;
  }

  .progress-message {
    font-size: 12px;
    color: var(--text-secondary);
    text-align: center;
    margin: 6px 0 0;
  }

  .success-message {
    font-size: 12px;
    color: var(--success);
    text-align: center;
    margin: 6px 0 0;
  }

  /* ── Animations ────────────────────────────────────────────────────── */

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── Scrollbar ─────────────────────────────────────────────────────── */

  :global(::-webkit-scrollbar) { width: 6px; }
  :global(::-webkit-scrollbar-track) { background: transparent; }
  :global(::-webkit-scrollbar-thumb) { background: var(--border); border-radius: 3px; }
  :global(::-webkit-scrollbar-thumb:hover) { background: var(--text-secondary); }
</style>
