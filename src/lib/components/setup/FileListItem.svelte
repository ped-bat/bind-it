<script>
  import IconButton from "$lib/components/ui/IconButton.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { formatDuration, formatBytes } from "$lib/services/format.js";

  /**
   * @type {{
   *   file: { path: string, chapter_name: string, codec: string, duration: number, file_size: number },
   *   index: number,
   *   onchapterchange: (value: string) => void,
   *   onremove: () => void
   * }}
   */
  let { file, index, onchapterchange, onremove } = $props();

  const codecKind = $derived.by(() => {
    if (file.codec === "aac") return "aac";
    if (file.codec === "mp3") return "mp3";
    if (["flac", "alac", "wav"].includes(file.codec)) return "lossless";
    return "other";
  });
</script>

<span class="drag-handle" title="Drag to reorder" aria-hidden="true">
  <Icon name="grip-vertical" width={16} height={16} />
</span>
<span class="file-number" aria-hidden="true">{index + 1}</span>
<input
  class="u-input u-input--sm u-input--ghost chapter-name"
  type="text"
  value={file.chapter_name}
  oninput={(e) => onchapterchange(/** @type {HTMLInputElement} */ (e.target).value)}
  aria-label="Chapter {index + 1} name"
/>
<span class="codec-badge codec-{codecKind}" aria-label="{file.codec.toUpperCase()} format">
  {file.codec.toUpperCase()}
</span>
<span class="file-duration">{formatDuration(file.duration)}</span>
<span class="file-size">{formatBytes(file.file_size)}</span>
<IconButton
  variant="danger"
  size="md"
  shape="circle"
  onclick={onremove}
  title="Remove"
  ariaLabel="Remove chapter {index + 1}"
>
  {#snippet children()}&times;{/snippet}
</IconButton>

<style>
  .drag-handle {
    color: var(--text-secondary);
    cursor: grab;
    user-select: none;
    touch-action: none;
    opacity: var(--opacity-faint);
    width: var(--control-sm);
    height: var(--control-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    transition: opacity var(--transition), background var(--transition);
  }

  :global(.file-item):hover .drag-handle { opacity: var(--opacity-soft); }
  :global(.file-item):focus-within .drag-handle { opacity: var(--opacity-subtle); }

  .drag-handle:hover {
    opacity: 1 !important;
    background: var(--border-ghost-60);
  }

  .file-number {
    color: var(--text-secondary);
    font-size: var(--font-xs);
    font-variant-numeric: tabular-nums;
    width: 18px;
    text-align: right;
    flex-shrink: 0;
  }

  .chapter-name {
    flex: 1;
    max-width: 100%;
    text-overflow: ellipsis;
    overflow: hidden;
    white-space: nowrap;
  }

  .chapter-name:focus {
    background: var(--bg);
    text-overflow: clip;
    overflow: visible;
  }

  .codec-badge {
    font-size: var(--font-xs);
    font-weight: var(--weight-semibold);
    line-height: 1;
    padding: var(--space-1) var(--space-3);
    border-radius: var(--radius-sm);
    text-transform: uppercase;
    letter-spacing: 0.02em;
    flex-shrink: 0;
  }

  .codec-aac {
    background: var(--success-ghost-15);
    color: var(--success);
  }

  .codec-mp3 {
    background: var(--accent-ghost-15);
    color: var(--accent);
  }

  .codec-lossless {
    background: var(--accent-ghost-25);
    color: var(--accent);
  }

  .codec-other {
    background: var(--border-ghost-60);
    color: var(--text-secondary);
  }

  .file-duration,
  .file-size {
    color: var(--text-secondary);
    font-size: var(--font-sm);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
    text-align: right;
  }

  .file-duration { width: 40px; }
  .file-size { width: 56px; }
</style>
