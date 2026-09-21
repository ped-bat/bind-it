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
 * Divisor that turns a Tauri drag position into CSS pixels.
 *
 * Tauri types the position as physical, but only WebView2 on Windows really
 * reports physical client pixels. wry's macOS handler passes the NSView point
 * through untouched and webkitgtk hands over GTK logical coordinates (checked
 * in wry 0.54.3 and 0.55.1, tauri-runtime-wry 2.10.1), and both of those are
 * already CSS pixels. Dividing those by the device pixel ratio would put the
 * hit-test at half the real distance from the corner on a 2x display.
 * @param {{ platform?: string, userAgent?: string } | undefined} [nav]
 * @param {number} [dpr]
 */
export function dragPositionScale(nav = globalThis.navigator, dpr = globalThis.devicePixelRatio || 1) {
  const onWindows = /^Win/i.test(nav?.platform ?? "") || /Windows/i.test(nav?.userAgent ?? "");
  return onWindows ? dpr : 1;
}

/**
 * The drop target under a Tauri drag position (webview-relative).
 * @param {{ x: number, y: number } | undefined | null} position
 * @param {{ scale?: number, doc?: Document }} [opts]
 * @returns {DropTarget}
 */
export function dropTargetAt(position, { scale = dragPositionScale(), doc = globalThis.document } = {}) {
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
