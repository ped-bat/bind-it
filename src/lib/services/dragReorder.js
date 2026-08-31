/**
 * Spring-FLIP transition. Pass to `animate:` directive on each list child.
 *
 * @param {Element} node
 * @param {{ from: DOMRect, to: DOMRect }} rects
 */
export function springFlip(node, { from, to }) {
  if (node.classList.contains("dragging")) return { duration: 0 };
  const dy = from.top - to.top;
  const dx = from.left - to.left;
  if (dy === 0 && dx === 0) return { duration: 0 };
  return {
    duration: 300,
    css: (/** @type {number} */ t) => {
      const s = t === 0 ? 0 : t === 1 ? 1
        : 1 - Math.pow(2, -8 * t) * Math.cos(t * Math.PI * 2);
      return `transform: translate(${dx * (1 - s)}px, ${dy * (1 - s)}px)`;
    },
  };
}

/**
 * Pointer-driven reorder drag. Wires global pointermove/pointerup, clamps
 * within container bounds, and swaps via `onReorder` when midpoints cross.
 *
 * Returns a teardown function the caller MUST keep around to clean up if the
 * component unmounts mid-drag.
 *
 * @param {{
 *   index: number,
 *   event: PointerEvent,
 *   itemSelector: string,
 *   containerSelector: string,
 *   onReorder: (from: number, to: number) => void,
 *   onUpdate: (state: { draggedIndex: number | null, dropTargetIndex: number | null, offsetY: number }) => void,
 * }} opts
 * @returns {() => void} teardown
 */
export function startReorderDrag({ index, event, itemSelector, containerSelector, onReorder, onUpdate }) {
  const target = /** @type {HTMLElement | null} */ (event.target);
  const el = target?.closest(itemSelector);
  if (!(el instanceof HTMLElement)) return () => {};

  const listEl = /** @type {HTMLElement | null} */ (el.closest(containerSelector));
  const listRect = listEl ? listEl.getBoundingClientRect() : null;
  const itemH = el.getBoundingClientRect().height;

  let dragged = index;
  let dragStartY = event.clientY;
  let offsetY = 0;
  onUpdate({ draggedIndex: dragged, dropTargetIndex: null, offsetY: 0 });

  /** @param {PointerEvent} me */
  const onMove = (me) => {
    let raw = me.clientY - dragStartY;

    const curEl = document.querySelector(`${itemSelector}[data-index="${dragged}"]`);
    if (!curEl) return;

    if (listRect) {
      const curRect = curEl.getBoundingClientRect();
      const naturalTop = curRect.top - offsetY;
      raw = Math.max(listRect.top, Math.min(naturalTop + raw, listRect.bottom - itemH)) - naturalTop;
    }
    offsetY = raw;

    const curRect = curEl.getBoundingClientRect();
    const dragMidY = curRect.top + curRect.height / 2;

    let dropTarget = null;
    for (const other of document.querySelectorAll(itemSelector)) {
      const idx = parseInt(/** @type {HTMLElement} */ (other).dataset.index ?? "");
      if (isNaN(idx) || idx === dragged) continue;
      const otherRect = other.getBoundingClientRect();
      const midY = otherRect.top + otherRect.height / 2;

      // `>=` / `<=` so the drop fires when midpoints exactly coincide. With
      // strict comparisons, dragging to position 0 (or the last slot) fails
      // because the clamped dragged midpoint equals the boundary item's
      // midpoint exactly — equal but not strictly less/greater.
      if ((idx > dragged && dragMidY >= midY) || (idx < dragged && dragMidY <= midY)) {
        onReorder(dragged, idx);
        dragStartY += (idx - dragged) * itemH;
        offsetY = me.clientY - dragStartY;
        dropTarget = idx;
        dragged = idx;
        break;
      }
    }

    onUpdate({ draggedIndex: dragged, dropTargetIndex: dropTarget, offsetY });
  };

  const onUp = () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onUp);
    window.removeEventListener("blur", onUp);
    try { el.releasePointerCapture(event.pointerId); } catch { /* already released */ }
    onUpdate({ draggedIndex: null, dropTargetIndex: null, offsetY: 0 });
  };

  // Capture the pointer so a release outside the window still ends the drag,
  // and treat pointercancel (touch/pen interruption) and window blur as
  // drag-end — otherwise the item stays stranded mid-drag with live listeners.
  try { el.setPointerCapture(event.pointerId); } catch { /* unsupported */ }
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onUp);
  window.addEventListener("blur", onUp);

  return onUp;
}
