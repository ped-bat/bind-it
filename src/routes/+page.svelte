<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy } from "svelte";

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
  });

  onDestroy(() => {
    if (unlistenProgress) unlistenProgress();
    if (unlistenDrop) unlistenDrop();
    if (unlistenDragOver) unlistenDragOver();
    if (unlistenDragLeave) unlistenDragLeave();
  });

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
    const newFiles = [...files];
    const [moved] = newFiles.splice(draggedIndex, 1);
    newFiles.splice(index, 0, moved);
    files = newFiles;
    draggedIndex = index;
  }

  function dragEnd() {
    draggedIndex = null;
  }

  // ── Convert ───────────────────────────────────────────────────────────────

  async function startConvert() {
    if (files.length < 1 || !outputDir || !outputFilename) return;
    error = null;
    converting = true;
    outputPath = null;
    progress = { stage: "preparing", percent: 0, message: "Starting…" };

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

  function totalDuration() {
    return files.reduce((sum, f) => sum + f.duration, 0);
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
      <span class="file-count">{files.length} file{files.length !== 1 ? "s" : ""} · {formatDuration(totalDuration())}</span>
    {/if}
  </header>

  {#if error}
    <div class="error-banner">
      <span>{error}</span>
      <button class="error-dismiss" onclick={() => error = null}>×</button>
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
        <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
          <path d="M8 6h20l12 12v24a2 2 0 01-2 2H8a2 2 0 01-2-2V8a2 2 0 012-2z" stroke="currentColor" stroke-width="2" fill="none"/>
          <path d="M28 6v12h12" stroke="currentColor" stroke-width="2" fill="none"/>
          <path d="M16 28h16M24 20v16" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
      </div>
      <p class="drop-title">Drop audio files here</p>
      <p class="drop-subtitle">or click to browse · MP3, M4A, M4B</p>
    </div>
  {:else}
    <div class="content">
      <!-- File list -->
      <section class="panel file-list">
        <div class="panel-header">
          <h2>Chapters</h2>
          <button class="btn-text" onclick={browseFiles}>+ Add files</button>
        </div>
        <div class="file-items">
          {#each files as file, i}
            <div
              class="file-item"
              class:dragging={draggedIndex === i}
              draggable="true"
              role="listitem"
              ondragstart={() => dragStart(i)}
              ondragover={(e) => dragOverItem(e, i)}
              ondragend={dragEnd}
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
  }

  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #1A1918;
      --surface: #242320;
      --border: #3A3835;
      --text: #EDECEA;
      --text-secondary: #9A958E;
      --accent: #D4893A;
      --accent-hover: #E09A4E;
      --success: #5A9C69;
      --error: #D46D5E;
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
    padding: 16px;
    box-sizing: border-box;
    gap: 12px;
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
    font-size: 22px;
    font-weight: 400;
    margin: 0;
    letter-spacing: -0.02em;
  }

  .file-count {
    font-size: 12px;
    color: var(--text-secondary);
  }

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
  }

  .error-dismiss {
    background: none;
    border: none;
    color: var(--error);
    cursor: pointer;
    font-size: 16px;
    padding: 0 4px;
  }

  .drop-zone {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: all 0.15s ease;
    gap: 8px;
  }

  .drop-zone:hover, .drop-zone.drag-over {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 5%, transparent);
  }

  .drop-icon {
    color: var(--text-secondary);
    opacity: 0.6;
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

  .content {
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .panel {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 12px 16px;
  }

  .panel h2 {
    font-size: 12px;
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

  .btn-text {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 12px;
    font-weight: 500;
    padding: 2px 4px;
  }

  .btn-text:hover { color: var(--accent-hover); }

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
    transition: background 0.1s ease;
  }

  .file-item:hover {
    background: color-mix(in srgb, var(--border) 50%, transparent);
  }

  .file-item.dragging { opacity: 0.5; }

  .drag-handle {
    color: var(--text-secondary);
    font-size: 14px;
    cursor: grab;
    user-select: none;
    opacity: 0.4;
    width: 14px;
    text-align: center;
  }

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
    transition: opacity 0.1s;
  }

  .file-item:hover .btn-remove { opacity: 1; }
  .btn-remove:hover { color: var(--error); }

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
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 12px;
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
    padding: 4px 8px;
    border-radius: 4px;
    outline: none;
    min-width: 0;
  }

  .metadata-fields input:focus { border-color: var(--accent); }

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
    padding: 4px 8px;
    border-radius: 4px;
    outline: none;
    min-width: 0;
  }

  .dir-input input:focus { border-color: var(--accent); }

  .btn-browse {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 12px;
    padding: 4px 10px;
    border-radius: 4px;
    cursor: pointer;
    flex-shrink: 0;
  }

  .btn-browse:hover { border-color: var(--accent); }

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
    padding: 4px 8px;
    border-radius: 4px 0 0 4px;
    border-right: none;
    outline: none;
    min-width: 0;
  }

  .filename-input input:focus { border-color: var(--accent); }

  .filename-input .ext {
    background: var(--border);
    color: var(--text-secondary);
    padding: 4px 8px;
    font-size: 12px;
    border-radius: 0 4px 4px 0;
    border: 1px solid var(--border);
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }

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
    padding: 4px 8px;
    border-radius: 4px;
    outline: none;
  }

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
    font-size: 14px;
    font-weight: 600;
    padding: 10px 24px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .btn-convert:hover:not(:disabled) { background: var(--accent-hover); }
  .btn-convert:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-reveal {
    width: 100%;
    background: var(--success);
    color: white;
    border: none;
    font-family: var(--font);
    font-size: 14px;
    font-weight: 600;
    padding: 10px 24px;
    border-radius: var(--radius);
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .btn-reveal:hover { filter: brightness(1.1); }

  .progress-container {
    width: 100%;
    height: 36px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .progress-bar {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s ease;
    border-radius: var(--radius);
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

  :global(::-webkit-scrollbar) { width: 6px; }
  :global(::-webkit-scrollbar-track) { background: transparent; }
  :global(::-webkit-scrollbar-thumb) { background: var(--border); border-radius: 3px; }
  :global(::-webkit-scrollbar-thumb:hover) { background: var(--text-secondary); }
</style>
