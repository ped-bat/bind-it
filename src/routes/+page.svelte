<script>
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount, onDestroy } from "svelte";
  import { fade } from "svelte/transition";
  import LogoMark from "$lib/LogoMark.svelte";

  // ── State ─────────────────────────────────────────────────────────────────

  let appState = $state("setup"); // "setup" | "converting" | "complete"

  /** @type {any[]} */
  let files = $state([]);
  /** @type {string | null} */
  let coverArt = $state(null);
  /** @type {string | null} */
  let coverArtPath = $state(null);
  let metadata = $state({ title: "", artist: "", album: "", narrator: "", year: "" });
  let outputDir = $state("");
  let outputFilename = $state("audiobook");
  let bitrate = $state(64);
  let mono = $state(true);
  let lossless = $state(true);
  /** @type {{ strategy: string, needs_transcode: string[], total_duration: number } | null} */
  let mergePlan = /** @type {{ strategy: string, needs_transcode: string[], total_duration: number } | null} */ ($state(null));
  let progress = $state({ stage: "", percent: 0, message: "" });
  let displayPercent = $state(0);
  let displayMessage = $state("");
  /** @type {string | null} */
  let outputPath = $state(null);
  /** @type {string | null} */
  let error = $state(null);
  let dismissingError = $state(false);
  /** @type {boolean | null} */
  let ffmpegOk = $state(null);
  let dragOver = $state(false);
  /** @type {number | null} */
  let draggedIndex = $state(null);
  /** @type {number | null} */
  let dropTargetIndex = $state(null);
  let probing = $state(false);
  let liveAnnouncement = $state("");
  /** @type {string | null} */
  let warning = $state(null);
  let focusedFileIndex = $state(-1);

  // Conversion timing
  let elapsedSeconds = $state(0);
  /** @type {ReturnType<typeof setInterval> | null} */
  let elapsedTimer = null;

  // Completion data
  /** @type {{ filename: string, elapsed: number, fileCount: number, totalDuration: number, sizeBytes: number } | null} */
  let completionData = $state(null);

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  /** @type {Function | undefined} */
  let unlistenProgress;
  /** @type {Function | undefined} */
  let unlistenComplete;
  /** @type {Function | undefined} */
  let unlistenError;
  /** @type {Function | undefined} */
  let unlistenCancelled;
  /** @type {Function | undefined} */
  let unlistenDrop;

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
      // Only update display if percent increased (never go backwards) and round to prevent flicker
      const rounded = Math.round(progress.percent);
      if (rounded > displayPercent) {
        displayPercent = rounded;
      }
      if (progress.message !== displayMessage) {
        displayMessage = progress.message;
      }
    });

    unlistenComplete = await listen("merge-complete", (event) => {
      const { path, size_bytes } = event.payload;
      outputPath = path;
      stopTimer();
      completionData = {
        filename: path.split(/[\\/]/).pop(),
        elapsed: elapsedSeconds,
        fileCount: files.length,
        totalDuration: totalDuration(),
        sizeBytes: size_bytes,
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

    // Tauri 2 file drop events via webview API
    unlistenDrop = await getCurrentWebview().onDragDropEvent(async (event) => {
      const { type } = event.payload;
      if (type === "over" || type === "enter") {
        dragOver = true;
      } else if (type === "leave") {
        dragOver = false;
      } else if (type === "drop") {
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
      }
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
    window.removeEventListener("keydown", handleKeydown);
    stopTimer();
  });

  /** @param {KeyboardEvent} e */
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
      } else if (error && !dismissingError) {
        dismissError();
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

  // ── Dialog helpers ───────────────────────────────────────────────────────

  /**
   * Extract string path from a dialog result (may be string or {path:string} object)
   * @param {any} item
   */
  function toPath(item) {
    return typeof item === "object" && item !== null && "path" in item ? item.path : item;
  }

  /** @param {any} selected */
  function toPaths(selected) {
    const items = Array.isArray(selected) ? selected : [selected];
    return items.map(toPath);
  }

  // ── File handling ─────────────────────────────────────────────────────────

  /** @param {string[]} paths @param {string | null} [folderName] */
  async function addFiles(paths, folderName = null) {
    error = null;
    probing = true;
    try {
      const result = await invoke("probe_files", { paths });
      const probed = result.files;
      if (result.warnings && result.warnings.length > 0) {
        warning = result.warnings.join("\n");
      }
      // Deduplicate by path
      const existingPaths = new Set(files.map((/** @type {any} */ f) => f.path));
      const newFiles = probed.filter((/** @type {any} */ f) => !existingPaths.has(f.path));
      files = [...files, ...newFiles];
      if (newFiles.length > 0) announce(`${newFiles.length} file${newFiles.length !== 1 ? "s" : ""} added`);

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
        mergePlan = await invoke("get_merge_plan", { files: filePlanInfo() });
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
      await addFiles(toPaths(selected));
    }
  }

  async function browseFolder() {
    const selected = await open({ directory: true });
    if (selected) {
      try {
        const result = await invoke("resolve_audio_paths", { paths: [toPath(selected)] });
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
    if (selected) outputDir = toPath(selected);
  }

  async function chooseCoverArt() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png"] }],
    });
    if (selected) {
      try {
        const art = await invoke("set_custom_cover_art", { path: toPath(selected) });
        coverArt = art.data_uri;
        coverArtPath = art.file_path;
      } catch (e) {
        error = String(e);
      }
    }
  }

  /** @param {DragEvent} e */
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
    warning = null;
  }

  /** @param {number} index */
  function removeFile(index) {
    files = files.filter((_, i) => i !== index);
    updateMergePlan();
  }

  /** @param {number} index @param {string} name */
  function updateChapterName(index, name) {
    files = files.map((f, i) => i === index ? { ...f, chapter_name: name } : f);
  }

  /** Build the file info payload for get_merge_plan (avoids re-probing in Rust) */
  function filePlanInfo() {
    return files.map(f => ({
      path: f.path,
      codec: f.codec,
      sample_rate: f.sample_rate,
      channels: f.channels,
      duration: f.duration,
    }));
  }

  async function updateMergePlan() {
    if (files.length >= 1) {
      try {
        mergePlan = await invoke("get_merge_plan", { files: filePlanInfo() });
      } catch (e) {
        mergePlan = null;
      }
    } else {
      mergePlan = null;
    }
  }

  // ── Drag reorder (pointer-based to avoid Tauri OS drop conflict) ────────

  let pointerDragActive = $state(false);
  let dragOffsetY = $state(0);
  let dragStartY = 0;

  /**
   * Custom animate function: spring-animates displaced items,
   * but returns duration:0 for the actively dragged item so
   * FLIP doesn't fight the manual translateY.
   * @param {Element} node
   * @param {{ from: DOMRect, to: DOMRect }} rects
   */
  function springFlip(node, { from, to }) {
    if (node.classList.contains('dragging')) {
      return { duration: 0 };
    }
    const dy = from.top - to.top;
    const dx = from.left - to.left;
    if (dy === 0 && dx === 0) return { duration: 0 };
    return {
      duration: 350,
      css: (/** @type {number} */ t) => {
        // Spring with gentle overshoot: peaks ~1.08 then settles to 1
        const s = t === 0 ? 0 : t === 1 ? 1
          : 1 - Math.pow(2, -6 * t) * Math.cos(t * Math.PI * 2);
        return `transform: translate(${dx * (1 - s)}px, ${dy * (1 - s)}px)`;
      }
    };
  }

  /** @param {number} index @param {PointerEvent} e */
  function dragStart(index, e) {
    if (!/** @type {HTMLElement} */ (e.target)?.closest('.drag-handle')) return;
    e.preventDefault();

    const el = /** @type {HTMLElement} */ (e.target).closest('.file-item');
    if (!el) return;

    const listEl = /** @type {HTMLElement | null} */ (el.closest('.file-items'));
    const listRect = listEl ? listEl.getBoundingClientRect() : null;
    const itemH = el.getBoundingClientRect().height;

    draggedIndex = index;
    pointerDragActive = true;
    dragStartY = e.clientY;
    dragOffsetY = 0;

    /** @param {PointerEvent} me */
    const onMove = (me) => {
      if (draggedIndex === null) return;
      let raw = me.clientY - dragStartY;

      // Clamp to list boundaries
      if (listRect) {
        const curEl = document.querySelector(`.file-item[data-index="${draggedIndex}"]`);
        if (curEl) {
          // Natural top = where the element sits in DOM without our transform
          const curRect = curEl.getBoundingClientRect();
          const naturalTop = curRect.top - dragOffsetY;
          const clampedTop = Math.max(listRect.top, Math.min(naturalTop + raw, listRect.bottom - itemH));
          raw = clampedTop - naturalTop;
        }
      }
      dragOffsetY = raw;

      // Find the visual center of the dragged item
      const curEl = document.querySelector(`.file-item[data-index="${draggedIndex}"]`);
      if (!curEl) return;
      const curRect = curEl.getBoundingClientRect();
      // curRect already includes our translateY, so this IS the visual position
      const dragMidY = curRect.top + curRect.height / 2;

      // Check neighbors for swap at 50% midpoint crossing
      const els = document.querySelectorAll('.file-item');
      for (const other of els) {
        const idx = parseInt(/** @type {HTMLElement} */ (other).dataset.index ?? "");
        if (isNaN(idx) || idx === draggedIndex) continue;
        const rect = other.getBoundingClientRect();
        const midY = rect.top + rect.height / 2;

        if ((idx > draggedIndex && dragMidY > midY) ||
            (idx < draggedIndex && dragMidY < midY)) {
          // Perform the swap
          const newFiles = [...files];
          const [moved] = newFiles.splice(draggedIndex, 1);
          newFiles.splice(idx, 0, moved);
          files = newFiles;

          // Adjust offset: the dragged item's DOM slot moved by (idx - draggedIndex) item heights.
          // We compensate so the item stays visually under the cursor.
          const slotDelta = (idx - draggedIndex) * itemH;
          dragStartY += slotDelta;
          // raw doesn't change — the visual position stays the same
          dragOffsetY = me.clientY - dragStartY;

          dropTargetIndex = idx;
          draggedIndex = idx;
          break;
        }
      }
    };

    const onUp = () => {
      draggedIndex = null;
      dropTargetIndex = null;
      pointerDragActive = false;
      dragOffsetY = 0;
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };

    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
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
        warning = preflight.warnings.join("\n");
      }
    } catch (e) {
      error = String(e);
      return;
    }

    progress = { stage: "preparing", percent: 0, message: "Starting\u2026" };
    displayPercent = 0;
    displayMessage = "Starting\u2026";

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
      force_transcode: !lossless,
      durations: files.map(f => f.duration),
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
    warning = null;
    completionData = null;
    progress = { stage: "", percent: 0, message: "" };
    elapsedSeconds = 0;
    appState = "setup";
  }

  // ── Helpers ───────────────────────────────────────────────────────────────

  /** @param {number} seconds */
  function formatDuration(seconds) {
    const m = Math.floor(seconds / 60);
    const s = Math.floor(seconds % 60);
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  /** @param {number} seconds */
  function formatDurationHuman(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  /** @param {number} seconds */
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

  /** @param {number} durationSec @param {number} bitrateKbps */
  function estimateFileSize(durationSec, bitrateKbps) {
    let effectiveBps;
    if (lossless && mergePlan?.strategy === "remux") {
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

  /** @param {number} bytes */
  function formatFileSize(bytes) {
    if (bytes >= 1073741824) return `${(bytes / 1073741824).toFixed(1)} GB`;
    if (bytes >= 1048576) return `${(bytes / 1048576).toFixed(1)} MB`;
    if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${bytes} B`;
  }

  /** @param {string} msg */
  function announce(msg) {
    liveAnnouncement = "";
    requestAnimationFrame(() => { liveAnnouncement = msg; });
  }

  function dismissError() {
    if (dismissingError) return;
    dismissingError = true;
    setTimeout(() => {
      error = null;
      dismissingError = false;
    }, 400);
  }

  /** @param {KeyboardEvent} e */
  function handleFileListKeydown(e) {
    if (files.length === 0) return;
    const focusEl = (/** @type {string} */ sel) => /** @type {HTMLElement|null} */ (document.querySelector(sel))?.focus();
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
      if (files.length === 0) {
        focusedFileIndex = -1;
      } else {
        focusedFileIndex = Math.max(0, Math.min(idx, files.length - 1));
      }
      requestAnimationFrame(() => {
        focusEl(`.file-item[data-index="${focusedFileIndex}"]`);
      });
    }
  }

</script>

<div class="sr-only" aria-live="polite" aria-atomic="true">{liveAnnouncement}</div>
<main>
  {#if appState === "converting"}
    <!-- ── Converting screen ─────────────────────────────────────────── -->
    <div class="converting-screen" in:fade={{ duration: 300 }}>
      <header class="converting-header">
        <div class="logo-block">
          <LogoMark />
          <h1>Bindery</h1>
        </div>
      </header>

      <div class="converting-content">
        <div class="converting-progress">
          <p class="converting-percent"><span class="percent-value">{displayPercent}</span>%</p>
          <div class="converting-bar-track" role="progressbar" aria-valuenow={displayPercent} aria-valuemin={0} aria-valuemax={100} aria-label="Conversion progress">
            <div class="converting-bar-fill" style="width: {displayPercent}%"></div>
          </div>
          <p class="converting-message">{displayMessage}</p>
          <p class="converting-elapsed">Elapsed time: {formatElapsed(elapsedSeconds)}</p>
        </div>

        <button class="btn-cancel" onclick={cancelConvert} aria-label="Cancel conversion">Cancel</button>
      </div>
    </div>

  {:else if appState === "complete"}
    <!-- ── Complete screen ───────────────────────────────────────────── -->
    <div class="complete-screen" in:fade={{ duration: 300 }}>
      <header class="complete-header">
        <div class="logo-block">
          <LogoMark />
          <h1>Bindery</h1>
        </div>
      </header>

      <div class="complete-content">
        <div class="complete-hero">
          <div class="complete-icon">
            <svg width="50" height="50" viewBox="0 0 48 48" fill="none">
              <circle class="check-circle" cx="24" cy="24" r="22" stroke="var(--accent)" stroke-width="2" fill="color-mix(in srgb, var(--accent) 10%, transparent)"/>
              <path class="check-path" d="M15 24l6 6 12-12" stroke="var(--accent)" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
            </svg>
          </div>
          <h2 class="complete-title">Audiobook binded!</h2>
        </div>

        {#if completionData}
          <div class="complete-details">
            <p class="complete-filename">{completionData.filename}</p>
            <div class="complete-stats">
              <span>{completionData.fileCount} file{completionData.fileCount !== 1 ? "s" : ""}</span>
              <span class="complete-stat-sep">&middot;</span>
              <span>{formatDurationHuman(completionData.totalDuration)}</span>
              {#if completionData.sizeBytes}
                <span class="complete-stat-sep">&middot;</span>
                <span>{formatFileSize(completionData.sizeBytes)}</span>
              {/if}
            </div>
            <p class="complete-processing-time">Processing time: {formatElapsed(completionData.elapsed)}</p>
          </div>
        {/if}

        <div class="complete-actions">
          <button class="btn-another" onclick={convertAnother}>Bind another</button>
          <button class="btn-reveal-complete" onclick={revealInFinder}>Open folder</button>
        </div>
      </div>
    </div>

  {:else}
    <!-- ── Setup screen ──────────────────────────────────────────────── -->
    {#if files.length > 0}
      <header>
        <div class="logo-block">
          <LogoMark />
          <h1>Bindery</h1>
        </div>
      </header>
    {/if}

    {#if error}
      <div class="error-banner" class:dismissing={dismissingError}>
        <div class="error-content">
          {#each error.split("\n") as line}
            <span class="error-line">{line}</span>
          {/each}
          {#if error.includes("disk space") || error.includes("Permission denied") || error.includes("moved or deleted")}
            <span class="error-retry">Fix the issue above, then try again.</span>
          {/if}
        </div>
        <button class="error-dismiss" onclick={dismissError} title="Dismiss (Esc)" aria-label="Dismiss error">×</button>
      </div>
    {/if}

    {#if warning}
      <div class="warning-banner">
        <div class="warning-content">
          {#each warning.split("\n") as line}
            <span class="warning-line">{line}</span>
          {/each}
        </div>
        <button class="warning-dismiss" onclick={() => warning = null} title="Dismiss" aria-label="Dismiss warning">×</button>
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
        role="button"
        aria-label="Drop audio files here or click to browse"
        tabindex="0"
        onclick={browseFiles}
        onkeydown={(e) => e.key === "Enter" && browseFiles()}
        ondragover={(e) => { e.preventDefault(); dragOver = true; }}
        ondragleave={() => dragOver = false}
        ondrop={handleDrop}
      >
        <div class="drop-icon">
          <LogoMark width={68} height={58} />
        </div>
        <h1 class="drop-zone-title">Bindery</h1>
        <p class="drop-title">Drop files or folders here</p>
        <p class="drop-subtitle">MP3, M4A, M4B, AAC</p>
        <div class="drop-buttons">
          <button class="btn-drop-browse" onclick={(e) => { e.stopPropagation(); browseFiles(); }}>Add files</button>
          <button class="btn-drop-browse" onclick={(e) => { e.stopPropagation(); browseFolder(); }}>Add folder</button>
        </div>
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
              <button class="btn-text" onclick={browseFolder}>+ Add folder</button>
              <button class="btn-text btn-text-danger" onclick={clearAll}>Clear all</button>
            </div>
          </div>
          <div class="file-items" role="listbox" tabindex="-1" aria-label="Chapter list" onkeydown={handleFileListKeydown}>
            {#each files as file, i (file.path)}
              <div
                class="file-item"
                class:dragging={draggedIndex === i}
                class:drop-target={dropTargetIndex === i && draggedIndex !== i}
                style={draggedIndex === i ? `transform: translateY(${dragOffsetY}px); z-index: 10;` : ''}
                role="option"
                aria-selected={i === focusedFileIndex}
                tabindex={i === focusedFileIndex ? 0 : -1}
                data-index={i}
                aria-label="Chapter {i + 1}: {file.chapter_name}"
                onpointerdown={(e) => dragStart(i, e)}
                onfocus={() => focusedFileIndex = i}
                animate:springFlip
                in:fade={{ duration: 150 }}
                out:fade={{ duration: 100 }}
              >
                <span class="drag-handle" title="Drag to reorder" aria-hidden="true">⠿</span>
                <span class="file-number" aria-hidden="true">{i + 1}</span>
                <input
                  class="chapter-name"
                  type="text"
                  value={file.chapter_name}
                  oninput={(e) => updateChapterName(i, /** @type {HTMLInputElement} */ (e.target).value)}
                  aria-label="Chapter {i + 1} name"
                />
                <span class="codec-badge" class:codec-aac={file.codec === "aac"} class:codec-mp3={file.codec === "mp3"} aria-label="{file.codec.toUpperCase()} format">
                  {file.codec.toUpperCase()}
                </span>
                <span class="file-duration">{formatDuration(file.duration)}</span>
                <span class="file-size">{formatFileSize(file.file_size)}</span>
                <button class="btn-remove" onclick={() => removeFile(i)} title="Remove" aria-label="Remove chapter {i + 1}">×</button>
              </div>
            {/each}
          </div>
        </section>

        <!-- Metadata + Quality/Output two-column layout -->
        <div class="two-col">
          <!-- Metadata panel -->
          <section class="panel metadata-panel">
            <h2>Metadata</h2>
            <div class="metadata-content">
              <div class="cover-art-container">
                {#if coverArt}
                  <button class="cover-art-btn" onclick={chooseCoverArt} aria-label="Change cover art">
                    <img class="cover-art" src={coverArt} alt="Cover art" />
                  </button>
                  <button class="btn-remove-cover" onclick={(e) => { e.stopPropagation(); coverArt = null; coverArtPath = null; }} title="Remove cover art" aria-label="Remove cover art">×</button>
                {:else}
                  <button class="cover-art-btn cover-placeholder" onclick={chooseCoverArt} aria-label="Choose cover art">
                    <svg width="32" height="32" viewBox="0 0 32 32" fill="none">
                      <rect x="4" y="4" width="24" height="24" rx="2" stroke="currentColor" stroke-width="1.5" fill="none"/>
                      <circle cx="12" cy="13" r="3" stroke="currentColor" stroke-width="1.5" fill="none"/>
                      <path d="M4 22l6-6 4 4 4-4 10 10" stroke="currentColor" stroke-width="1.5" fill="none"/>
                    </svg>
                  </button>
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

          <!-- Right column: Output + Quality -->
          <div class="right-col">
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

            {#if mergePlan}
              <section class="panel quality-panel">
                <h2>Quality</h2>
                <div class="encoding-fields">
                  <div class="quality-group">
                    <span class="quality-group-label">Channels</span>
                    <label class="checkbox-label">
                      <input type="checkbox" bind:checked={mono} />
                      <span>Mono (half output size)</span>
                    </label>
                  </div>
                  <div class="quality-group">
                    <span class="quality-group-label">AAC settings</span>
                    <label class="checkbox-label">
                      <input type="checkbox" bind:checked={lossless} />
                      <span>Preserve original quality (only AAC)</span>
                    </label>
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
                  </div>
                  <div class="quality-group">
                    <span class="quality-group-label">Expected output</span>
                    <div class="expected-output">
                      <span>{files.length} file{files.length !== 1 ? "s" : ""}</span>
                      <span class="expected-sep">&middot;</span>
                      <span>{formatDurationHuman(totalDuration())}</span>
                      <span class="expected-sep">&middot;</span>
                      <span>{estimateFileSize(totalDuration(), bitrate)}</span>
                    </div>
                  </div>
                </div>
              </section>
            {/if}
          </div>
        </div>

        <!-- Convert button -->
        <div class="convert-section">
          <button class="btn-cancel-setup" onclick={clearAll}>Cancel</button>
          <button
            class="btn-convert"
            onclick={startConvert}
            disabled={files.length < 1 || !ffmpegOk}
          >
            Bind it!
          </button>
        </div>
      </div>
    {/if}
  {/if}
</main>

<style>
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
    --radius: 8px;
    --radius-lg: 12px;
    --font: "Inter", system-ui, -apple-system, sans-serif;
    --font-display: "Instrument Serif", Georgia, serif;
    --transition: 150ms ease-out;
  }

  :global(::selection) {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    color: var(--text);
  }

  :global(body) {
    margin: 0;
    padding: 0;
    background: var(--bg);
    color: var(--text);
    font-family: var(--font);
    font-size: 16px;
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
    justify-content: center;
    align-items: baseline;
    gap: 12px;
    padding: 0 4px;
  }

  .logo-block {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .logo-block :global(.logo-mark) {
    color: var(--accent);
  }

  h1 {
    font-family: var(--font-display);
    font-size: 24px;
    font-weight: 400;
    margin: 0;
    letter-spacing: -0.02em;
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
    border-radius: var(--radius-lg);
    font-size: 12px;
    flex-shrink: 0;
    animation: fadeIn 150ms ease-out;
    overflow: hidden;
  }

  .error-banner.dismissing {
    animation: errorDismiss 400ms ease-out forwards;
  }

  @keyframes errorDismiss {
    0% { opacity: 1; max-height: 200px; padding: 8px 12px; }
    50% { opacity: 0; max-height: 200px; padding: 8px 12px; }
    100% { opacity: 0; max-height: 0; padding: 0 12px; }
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

  /* ── Warning banner ───────────────────────────────────────────────── */

  .warning-banner {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    color: var(--accent);
    padding: 8px 12px;
    border-radius: var(--radius-lg);
    font-size: 12px;
    flex-shrink: 0;
    animation: fadeIn 150ms ease-out;
  }

  .warning-content {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }

  .warning-line {
    display: block;
  }

  .warning-dismiss {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    font-size: 16px;
    padding: 0 4px;
    border-radius: 4px;
    transition: background var(--transition);
  }

  .warning-dismiss:hover {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
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
  }

  .drop-zone.probing-state {
    cursor: default;
  }

  .drop-zone-title {
    font-family: var(--font-display);
    font-size: 36px;
    font-weight: 400;
    margin: 0 0 4px 0;
    letter-spacing: -0.02em;
    color: var(--text);
  }

  .drop-zone:hover .drop-icon, .drop-zone.drag-over .drop-icon {
    color: var(--accent);
    opacity: 0.7;
  }

  .drop-icon {
    color: var(--accent);
    opacity: 0.8;
    transition: all var(--transition);
  }

  .drop-title {
    font-size: 18px;
    font-weight: 500;
    margin: 0;
    color: var(--text);
  }

  .drop-subtitle {
    font-size: 14px;
    color: var(--text-secondary);
    margin: 0;
  }

  .drop-buttons {
    display: flex;
    gap: 8px;
    margin-top: 16px;
  }

  .btn-drop-browse {
    background: var(--accent);
    border: none;
    color: white;
    font-family: var(--font);
    font-size: 15px;
    font-weight: 600;
    padding: 10px 28px;
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: background var(--transition);
  }

  .btn-drop-browse:hover {
    background: color-mix(in srgb, var(--accent) 80%, white);
  }



  /* ── Content layout ────────────────────────────────────────────────── */

  .content {
    display: flex;
    flex-direction: column;
    gap: 24px;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: 10px;
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
    font-size: 13px;
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
    background: var(--accent);
    border: none;
    color: white;
    cursor: pointer;
    font-size: 11px;
    font-weight: 600;
    padding: 6px 12px;
    min-height: 28px;
    border-radius: var(--radius-lg);
    transition: background var(--transition);
  }

  .btn-text:hover {
    background: color-mix(in srgb, var(--accent) 80%, white);
  }

  .btn-text-danger {
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-secondary);
  }

  .btn-text-danger:hover {
    background: color-mix(in srgb, var(--surface) 80%, white);
    color: var(--text);
    border-color: color-mix(in srgb, var(--border) 80%, white);
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
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-lg);
    cursor: grab;
    transition: background var(--transition), opacity var(--transition), box-shadow var(--transition);
    position: relative;
  }

  .file-item:hover {
    background: color-mix(in srgb, var(--border) 40%, transparent);
  }

  .file-item.dragging {
    opacity: 0.95;
    background: color-mix(in srgb, var(--accent) 15%, var(--surface));
    box-shadow: 0 4px 16px rgba(0,0,0,0.25);
    transition: background var(--transition), box-shadow var(--transition);
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
    font-size: 18px;
    cursor: grab;
    user-select: none;
    touch-action: none;
    opacity: 0.4;
    min-width: 32px;
    min-height: 36px;
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
    font-size: 15px;
    padding: 6px 8px;
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

  .file-size {
    color: var(--text-secondary);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
    width: 56px;
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
    opacity: 0.8;
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
    flex-direction: column;
    align-items: center;
    gap: 16px;
  }

  .cover-art-container {
    position: relative;
    flex-shrink: 0;
    display: inline-flex;
    align-self: center;
  }

  .btn-remove-cover {
    position: absolute;
    top: -14px;
    right: -14px;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-secondary);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0.8;
    transition: opacity var(--transition), background var(--transition), color var(--transition);
    z-index: 2;
  }

  .btn-remove-cover:hover {
    opacity: 1;
    color: var(--error);
    background: color-mix(in srgb, var(--error) 12%, transparent);
  }

  .cover-art-btn {
    position: relative;
    padding: 0;
    margin: 0;
    background: none;
    border: none;
    cursor: pointer;
    flex-shrink: 0;
  }

  .cover-art {
    width: 120px;
    height: 120px;
    border-radius: var(--radius-lg);
    object-fit: cover;
    display: block;
    border: 1px solid var(--border);
    transition: border-color var(--transition), box-shadow var(--transition);
  }

  .cover-art-btn:hover .cover-art {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
  }

  .cover-placeholder {
    width: 120px;
    height: 120px;
    border-radius: var(--radius-lg);
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
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .metadata-fields label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .metadata-fields label span {
    font-size: 13px;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .metadata-fields input {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 15px;
    padding: 8px 10px;
    border-radius: 4px;
    outline: none;
    min-width: 0;
    height: 40px;
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
    gap: 12px;
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


  .encoding-fields {
    display: flex;
    flex-direction: column;
    gap: 16px;
    margin-bottom: 8px;
  }

  .encoding-fields label {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .encoding-fields label > span {
    font-size: 13px;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .encoding-fields select {
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font);
    font-size: 15px;
    padding: 8px 10px;
    border-radius: 4px;
    outline: none;
    height: 40px;
    box-sizing: border-box;
    transition: border-color var(--transition);
  }

  .encoding-fields select:hover { border-color: color-mix(in srgb, var(--accent) 40%, var(--border)); }
  .encoding-fields select:focus { border-color: var(--accent); }

  .checkbox-label {
    flex-direction: row !important;
    align-items: center !important;
    gap: 8px !important;
  }

  .checkbox-label > span {
    font-size: 15px !important;
    color: var(--text) !important;
    font-weight: 400 !important;
  }

  .checkbox-label input[type="checkbox"] {
    accent-color: var(--accent);
    width: 18px;
    height: 18px;
    flex-shrink: 0;
  }

  .quality-group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .quality-group-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .expected-output {
    font-size: 15px;
    color: var(--text);
  }

  .expected-sep {
    margin: 0 4px;
    color: var(--text-secondary);
  }

  /* ── Convert button ─────────────────────────────────────────────────── */

  .convert-section {
    display: flex;
    gap: 24px;
    padding: 4px 0;
    flex-shrink: 0;
  }

  .btn-convert {
    flex: 1;
    background: var(--accent);
    color: white;
    border: none;
    font-family: var(--font);
    font-size: 16px;
    font-weight: 600;
    padding: 0 24px;
    height: 44px;
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: background var(--transition), transform var(--transition);
  }

  .btn-convert:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 80%, white);
  }
  .btn-convert:active:not(:disabled) { transform: scale(0.99); }
  .btn-convert:disabled { opacity: 0.5; cursor: not-allowed; }


  .btn-cancel-setup {
    flex: 1;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-secondary);
    font-family: var(--font);
    font-size: 16px;
    font-weight: 500;
    padding: 0 24px;
    height: 44px;
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: background var(--transition), color var(--transition), border-color var(--transition);
  }

  .btn-cancel-setup:hover {
    background: color-mix(in srgb, var(--surface) 80%, white);
    color: var(--text);
    border-color: color-mix(in srgb, var(--border) 80%, white);
  }

  .right-col {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  /* ── Converting screen ─────────────────────────────────────────────── */

  .converting-screen {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 0;
  }

  .converting-header {
    display: flex;
    justify-content: center;
    padding: 0 4px;
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
    font-feature-settings: "tnum";
    color: var(--text);
    margin: 0;
    letter-spacing: -0.02em;
    min-width: 4.5ch;
    text-align: center;
  }

  .percent-value {
    display: inline-block;
    transition: transform 0.3s ease-out;
  }

  .converting-bar-track {
    width: 100%;
    height: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 999px;
    overflow: hidden;
  }

  .converting-bar-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
    transition: width 0.5s ease-out;
    position: relative;
    overflow: hidden;
  }

  .converting-bar-fill::after {
    content: "";
    position: absolute;
    top: 0;
    left: -100%;
    width: 100%;
    height: 100%;
    background: linear-gradient(
      90deg,
      transparent 0%,
      rgba(255, 255, 255, 0.15) 20%,
      rgba(255, 255, 255, 0.35) 40%,
      rgba(255, 255, 255, 0.45) 50%,
      rgba(255, 255, 255, 0.35) 60%,
      rgba(255, 255, 255, 0.15) 80%,
      transparent 100%
    );
    animation: barSweep 2s ease-in-out infinite;
  }

  @keyframes barSweep {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(200%); }
  }

  .converting-message {
    font-size: 16px;
    color: var(--text-secondary);
    margin: 0;
    text-align: center;
    font-variant-numeric: tabular-nums;
    font-feature-settings: "tnum";
  }

  .converting-elapsed {
    font-size: 13px;
    color: var(--text-secondary);
    opacity: 0.6;
    margin: -4px 0 0 0;
    font-variant-numeric: tabular-nums;
    font-feature-settings: "tnum";
  }

  .btn-cancel {
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--text-secondary);
    font-family: var(--font);
    font-size: 16px;
    font-weight: 500;
    padding: 12px 32px;
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: background var(--transition), border-color var(--transition), color var(--transition);
  }

  .btn-cancel:hover {
    background: color-mix(in srgb, var(--surface) 80%, white);
    color: var(--text);
    border-color: color-mix(in srgb, var(--border) 80%, white);
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
    gap: 0;
  }

  .complete-hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    margin-bottom: 32px;
  }

  .complete-details {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin-bottom: 32px;
    max-width: 50%;
  }

  .complete-processing-time {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 4px 0 0 0;
  }

  .complete-icon {
    animation: springScale 0.6s cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
  }

  @keyframes springScale {
    0% { transform: scale(0.3); opacity: 0; }
    60% { transform: scale(1.15); opacity: 1; }
    80% { transform: scale(0.95); }
    100% { transform: scale(1); opacity: 1; }
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
    font-size: 22px;
    font-weight: 600;
    color: var(--text);
    margin: 0;
  }

  .complete-filename {
    font-size: 16px;
    color: var(--text-secondary);
    margin: 0;
    background: var(--surface);
    padding: 8px 18px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    max-width: 50vw;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
  }

  .complete-stats {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 15px;
    color: var(--text-secondary);
  }

  .complete-stat-sep {
    opacity: 0.4;
  }

  .complete-actions {
    display: flex;
    gap: 24px;
    width: 100%;
    max-width: 50vw;
  }

  .btn-reveal-complete {
    flex: 1;
    background: var(--accent);
    color: white;
    border: none;
    font-family: var(--font);
    font-size: 16px;
    font-weight: 600;
    padding: 0 24px;
    height: 44px;
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: background var(--transition);
  }

  .btn-reveal-complete:hover {
    background: color-mix(in srgb, var(--accent) 80%, white);
  }

  .btn-another {
    flex: 1;
    background: var(--accent);
    color: white;
    border: none;
    font-family: var(--font);
    font-size: 16px;
    font-weight: 600;
    padding: 0 24px;
    height: 44px;
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition: background var(--transition);
  }

  .btn-another:hover {
    background: color-mix(in srgb, var(--accent) 80%, white);
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
  .btn-cancel-setup:focus-visible,
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

  .cover-art-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: var(--radius-lg);
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

    .converting-bar-fill::after {
      animation: none;
    }

    .complete-icon {
      animation: none;
      transform: scale(1);
    }

    .complete-icon .check-circle,
    .complete-icon .check-path {
      animation: none;
      stroke-dashoffset: 0;
    }
  }
</style>
