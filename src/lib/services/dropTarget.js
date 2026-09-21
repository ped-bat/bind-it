// Native (Tauri) drops never reach the DOM as drop events, so the only way to
// know what the user dropped onto is to hit-test the position Tauri reports.
// Elements opt in as targets with a `data-drop-target` attribute.

/** @typedef {"cover" | null} DropTarget */

const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png"];

/**
 * Mirrors the backend's rule: extension after the last dot of the file name,
 * and a leading-dot name like ".png" has no extension at all.
 * @param {string} path
 */
export function isImagePath(path) {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return false;
  return IMAGE_EXTENSIONS.includes(name.slice(dot + 1).toLowerCase());
}

/**
 * First dropped path that looks like a cover image, or null.
 * @param {string[]} paths
 */
export function firstImagePath(paths) {
  return paths.find(isImagePath) ?? null;
}

/**
 * The drop target under a Tauri drag position. Tauri reports physical pixels
 * relative to the webview; the DOM wants CSS pixels.
 * @param {{ x: number, y: number } | undefined | null} position
 * @param {{ scale?: number, doc?: Document }} [opts]
 * @returns {DropTarget}
 */
export function dropTargetAt(position, { scale = globalThis.devicePixelRatio || 1, doc = globalThis.document } = {}) {
  if (!position || !doc) return null;
  const el = doc.elementFromPoint(position.x / scale, position.y / scale);
  const target = el?.closest?.("[data-drop-target]")?.getAttribute("data-drop-target");
  return target === "cover" ? "cover" : null;
}

/**
 * Whether a drop target is currently on screen (the cover square only exists
 * once chapters have been added).
 * @param {"cover"} target
 * @param {Document} [doc]
 */
export function hasDropTarget(target, doc = globalThis.document) {
  return Boolean(doc?.querySelector(`[data-drop-target="${target}"]`));
}
