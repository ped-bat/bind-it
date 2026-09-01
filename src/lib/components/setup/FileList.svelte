<script>
  import { fade } from "svelte/transition";
  import { onDestroy } from "svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import FileListItem from "$lib/components/setup/FileListItem.svelte";
  import { fileStore } from "$lib/stores/files.svelte.js";
  import { appStore } from "$lib/stores/app.svelte.js";
  import { addFilesFromBrowse, addFilesFromFolder, clearAllWithConfirm } from "$lib/services/actions.js";
  import { springFlip, startReorderDrag } from "$lib/services/dragReorder.js";

  let focusedFileIndex = $state(-1);

  /** @type {number | null} */
  let draggedIndex = $state(null);
  /** @type {number | null} */
  let dropTargetIndex = $state(null);
  let dragOffsetY = $state(0);
  /** @type {(() => void) | null} */
  let cleanupDrag = null;

  /** @param {number} index @param {PointerEvent} e */
  function dragStart(index, e) {
    if (!/** @type {HTMLElement} */ (e.target)?.closest('.drag-handle')) return;
    e.preventDefault();
    // Tear down any previous drag before starting a new one, so stale
    // move/up listeners can't stack and fight over the state.
    cleanupDrag?.();
    cleanupDrag = startReorderDrag({
      index,
      event: e,
      itemSelector: '.file-item',
      containerSelector: '.file-items',
      onReorder: (from, to) => fileStore.reorder(from, to),
      onUpdate: ({ draggedIndex: d, dropTargetIndex: dt, offsetY }) => {
        draggedIndex = d;
        dropTargetIndex = dt;
        dragOffsetY = offsetY;
        if (d === null) cleanupDrag = null;
      },
    });
  }

  onDestroy(() => { cleanupDrag?.(); });

  /** @param {string} sel */
  const focusEl = (sel) => /** @type {HTMLElement|null} */ (document.querySelector(sel))?.focus();

  /** @param {KeyboardEvent} e */
  function handleFileListKeydown(e) {
    if (fileStore.count === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      focusedFileIndex = Math.min(focusedFileIndex + 1, fileStore.count - 1);
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
      fileStore.remove(idx);
      appStore.announce(`Removed chapter ${idx + 1}`);
      focusedFileIndex = fileStore.count === 0 ? -1 : Math.min(idx, fileStore.count - 1);
      requestAnimationFrame(() => focusEl(`.file-item[data-index="${focusedFileIndex}"]`));
    }
  }
</script>

<Panel title="Chapters">
  {#snippet actions()}
    <div class="panel-actions">
      {#if fileStore.probing}<span class="spinner-inline" aria-label="Loading"></span>{/if}
      <Button variant="secondary" size="sm" onclick={addFilesFromBrowse}>
        {#snippet children()}+ Files{/snippet}
      </Button>
      <Button variant="secondary" size="sm" onclick={addFilesFromFolder}>
        {#snippet children()}+ Folder{/snippet}
      </Button>
      <Button variant="secondary" size="sm" onclick={clearAllWithConfirm}>
        {#snippet children()}Clear{/snippet}
      </Button>
    </div>
  {/snippet}
  <div class="file-items" role="listbox" tabindex="-1" aria-label="Chapter list" onkeydown={handleFileListKeydown}>
    {#each fileStore.items as file, i (file.path)}
      <div
        class="file-item"
        class:dragging={draggedIndex === i}
        class:drop-target={dropTargetIndex === i && draggedIndex !== i}
        style={draggedIndex === i ? `transform: translateY(${dragOffsetY}px); z-index: var(--z-dragging);` : ''}
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
        <FileListItem
          {file}
          index={i}
          onchapterchange={(v) => fileStore.updateChapterName(i, v)}
          onremove={() => fileStore.remove(i)}
        />
      </div>
    {/each}
  </div>
</Panel>

<style>
  .panel-actions {
    display: flex;
    gap: var(--space-3);
    align-items: center;
  }

  .spinner-inline {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: var(--radius-full);
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  .file-items {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: var(--space-5);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    cursor: grab;
    transition:
      background var(--transition),
      opacity var(--transition),
      box-shadow var(--transition);
    position: relative;
  }

  .file-item:hover {
    background: var(--border-ghost-40);
  }

  .file-item.dragging {
    opacity: var(--opacity-near);
    background: var(--accent-on-surface);
    box-shadow: var(--shadow-drag);
  }

  .file-item.drop-target::before {
    content: "";
    position: absolute;
    top: -2px;
    left: var(--space-4);
    right: var(--space-4);
    height: 2px;
    background: var(--accent);
    border-radius: var(--radius-pill);
  }

  .file-item:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
    background: var(--accent-ghost-6);
  }
</style>
