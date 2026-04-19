<script>
  import logomark from "$lib/icons/logomark.svg?raw";
  import checkCircle from "$lib/icons/check-circle.svg?raw";
  import imagePlaceholder from "$lib/icons/image-placeholder.svg?raw";
  import arrowDown from "$lib/icons/arrow-down.svg?raw";
  import arrowRight from "$lib/icons/arrow-right.svg?raw";
  import bolt from "$lib/icons/bolt.svg?raw";
  import book from "$lib/icons/book.svg?raw";
  import downloadTray from "$lib/icons/download-tray.svg?raw";
  import grid from "$lib/icons/grid.svg?raw";
  import gripVertical from "$lib/icons/grip-vertical.svg?raw";

  const REGISTRY = {
    logomark,
    "check-circle": checkCircle,
    "image-placeholder": imagePlaceholder,
    "arrow-down": arrowDown,
    "arrow-right": arrowRight,
    bolt,
    book,
    "download-tray": downloadTray,
    grid,
    "grip-vertical": gripVertical,
  };

  /** @type {{ name: keyof typeof REGISTRY, width?: number | string, height?: number | string, class?: string, ariaLabel?: string }} */
  let { name, width, height, class: className = "", ariaLabel } = $props();

  /** @param {number | string | undefined} v */
  const sizePx = (v) => (typeof v === "number" ? `${v}px` : v);
</script>

<span
  class="icon {className}"
  style:--icon-w={sizePx(width)}
  style:--icon-h={sizePx(height)}
  role={ariaLabel ? "img" : undefined}
  aria-label={ariaLabel}
  aria-hidden={ariaLabel ? undefined : "true"}
>{@html REGISTRY[name]}</span>

<style>
  .icon { display: inline-flex; line-height: 0; color: inherit; }
  .icon :global(svg) {
    width: var(--icon-w, 1em);
    height: var(--icon-h, 1em);
    display: block;
  }
</style>
