import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, open } from "@tauri-apps/plugin-dialog";

// ── Tauri invoke wrappers ───────────────────────────────────────────────────

export const checkFfmpeg = () => invoke("check_ffmpeg");
/** @param {string[]} paths */
export const probeFiles = (paths) => invoke("probe_files", { paths });
/** @param {string[]} paths */
export const resolveAudioPaths = (paths) => invoke("resolve_audio_paths", { paths });
/** @param {any[]} files */
export const getMergePlan = (files) => invoke("get_merge_plan", { files });
/** @param {string[]} paths */
export const getCoverArt = (paths) => invoke("get_cover_art", { paths });
/** @param {string} path */
export const setCustomCoverArt = (path) => invoke("set_custom_cover_art", { path });
/** @param {any} config */
export const mergeAudioFiles = (config) => invoke("merge_audio_files", { config });
export const cancelMerge = () => invoke("cancel_merge");
/** @param {{ files: string[], outputDir: string, outputFilename: string, outputExtension?: string }} args */
export const preflightCheck = (args) => invoke("preflight_check", args);

export async function revealInFolder(/** @type {string} */ path) {
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

// ── Dialog wrappers ─────────────────────────────────────────────────────────

/** @param {any} item */
function toPath(item) {
  return typeof item === "object" && item !== null && "path" in item ? item.path : item;
}

export async function browseFiles() {
  const selected = await open({
    multiple: true,
    filters: [{ name: "Audio", extensions: ["mp3", "m4a", "m4b", "aac", "wav", "flac", "wma"] }],
  });
  if (!selected) return null;
  const items = Array.isArray(selected) ? selected : [selected];
  return items.map(toPath);
}

/** @param {string} [defaultPath] */
export async function browseFolder(defaultPath) {
  const selected = await open({ directory: true, defaultPath: defaultPath || undefined });
  return selected ? toPath(selected) : null;
}

/**
 * Native async confirm. `window.confirm` is non-blocking in Tauri's webview,
 * so callers must await this instead.
 * @param {string} message
 * @param {{ title?: string, kind?: "info" | "warning" | "error", okLabel?: string, cancelLabel?: string }} [opts]
 */
export async function confirmAsk(message, opts = {}) {
  return await ask(message, {
    title: opts.title,
    kind: opts.kind ?? "warning",
    okLabel: opts.okLabel,
    cancelLabel: opts.cancelLabel,
  });
}

export async function browseImage() {
  const selected = await open({
    multiple: false,
    filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png"] }],
  });
  return selected ? toPath(selected) : null;
}

/**
 * Browse for a folder and resolve its audio files.
 * Returns { paths, folderName } or null.
 */
export async function browseFolderAndResolve() {
  const dir = await browseFolder();
  if (!dir) return null;
  const result = await resolveAudioPaths([dir]);
  return result.paths.length > 0 ? { paths: result.paths, folderName: result.folder_name } : null;
}

// ── Event listeners ─────────────────────────────────────────────────────────

/**
 * @param {object} handlers
 * @param {(payload: any) => void} handlers.onProgress
 * @param {(payload: any) => void} handlers.onComplete
 * @param {(payload: any) => void} handlers.onError
 * @param {() => void} handlers.onCancelled
 * @param {(paths: string[], folderName: string | null) => void} handlers.onFileDrop
 * @param {(message: string) => void} handlers.onDropError
 * @param {(over: boolean) => void} handlers.onDragState
 * @returns {Promise<() => void>} cleanup function
 */
export async function setupListeners(handlers) {
  /** @param {(arg?: any) => void} fn */
  const safe = (fn) => /** @param {any} [arg] */ (arg) => {
    try { fn(arg); } catch (e) { console.error("Listener handler threw:", e); }
  };
  const unlistens = await Promise.all([
    listen("merge-progress", (e) => safe(handlers.onProgress)(e.payload)),
    listen("merge-complete", (e) => safe(handlers.onComplete)(e.payload)),
    listen("merge-error", (e) => safe(handlers.onError)(e.payload)),
    listen("merge-cancelled", () => safe(handlers.onCancelled)()),
  ]);

  // The native drag-drop events are the only reliable source across
  // platforms — with dragDropEnabled, WebView2 (Windows) and webkitgtk
  // (Linux) don't deliver DOM file-drag events at all, so hover state must
  // come from here rather than DOM dragover/dragleave.
  const unlistenDrop = await getCurrentWebview().onDragDropEvent(async (event) => {
    const type = event.payload.type;
    if (type === "enter" || type === "over") {
      safe(() => handlers.onDragState(true))();
      return;
    }
    if (type === "leave") {
      safe(() => handlers.onDragState(false))();
      return;
    }
    if (type !== "drop") return;
    safe(() => handlers.onDragState(false))();
    const paths = event.payload.paths || [];
    if (paths.length === 0) return;
    try {
      const result = await resolveAudioPaths(paths);
      if (result.paths.length > 0) {
        safe(() => handlers.onFileDrop(result.paths, result.folder_name))();
      } else {
        safe(() => handlers.onDropError("No supported audio files found in dropped items."))();
      }
    } catch (e) {
      safe(() => handlers.onDropError(String(e)))();
    }
  });

  return () => {
    unlistens.forEach(fn => fn());
    unlistenDrop();
  };
}
