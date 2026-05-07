<script>
  import lottie from "lottie-web";
  import introData from "$lib/animations/intro.json";
  import pulseData from "$lib/animations/pulse.json";

  /** @type {{ animation?: 'intro' | 'pulse', loop?: boolean, width?: number, height?: number, class?: string, ariaLabel?: string }} */
  let {
    animation = "pulse",
    loop = false,
    width = 60,
    height = 50,
    class: className = "",
    ariaLabel,
  } = $props();

  /** @type {HTMLDivElement | undefined} */
  let container;
  /** @type {any} */
  let anim;

  /**
   * @param {string} css
   * @returns {[number, number, number] | null}
   */
  function parseCssColor(css) {
    const s = css.trim();
    if (s.startsWith("#")) {
      const hex = s.slice(1);
      const full = hex.length === 3 ? hex.split("").map((c) => c + c).join("") : hex;
      if (full.length < 6) return null;
      const r = parseInt(full.slice(0, 2), 16);
      const g = parseInt(full.slice(2, 4), 16);
      const b = parseInt(full.slice(4, 6), 16);
      if ([r, g, b].some(Number.isNaN)) return null;
      return [r / 255, g / 255, b / 255];
    }
    const m = s.match(/rgba?\(([^)]+)\)/i);
    if (m) {
      const parts = m[1].split(/[\s,\/]+/).filter(Boolean).slice(0, 3).map(Number);
      if (parts.length === 3 && parts.every((n) => !Number.isNaN(n))) {
        return [parts[0] / 255, parts[1] / 255, parts[2] / 255];
      }
    }
    return null;
  }

  /**
   * @param {any} data
   * @param {[number, number, number]} rgb
   */
  function tintLottie(data, rgb) {
    /** @param {any} obj */
    function visit(obj) {
      if (!obj || typeof obj !== "object") return;
      if (Array.isArray(obj)) {
        for (const v of obj) visit(v);
        return;
      }
      for (const key of Object.keys(obj)) {
        const val = obj[key];
        if (key === "c" && val && typeof val === "object" && "k" in val) {
          if (val.a === 0 && Array.isArray(val.k) && val.k.length >= 3) {
            val.k[0] = rgb[0];
            val.k[1] = rgb[1];
            val.k[2] = rgb[2];
          } else if (Array.isArray(val.k)) {
            for (const kf of val.k) {
              if (kf && Array.isArray(kf.s) && kf.s.length >= 3) {
                kf.s[0] = rgb[0];
                kf.s[1] = rgb[1];
                kf.s[2] = rgb[2];
              }
              if (kf && Array.isArray(kf.e) && kf.e.length >= 3) {
                kf.e[0] = rgb[0];
                kf.e[1] = rgb[1];
                kf.e[2] = rgb[2];
              }
            }
          }
        }
        visit(val);
      }
    }
    visit(data);
    return data;
  }

  $effect(() => {
    if (!container) return;
    const which = animation;
    const shouldLoop = loop;

    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent");
    const rgb = parseCssColor(accent) ?? [0.831, 0.537, 0.227];
    const src = which === "intro" ? introData : pulseData;
    const data = tintLottie(structuredClone(src), rgb);
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    anim?.destroy();
    anim = lottie.loadAnimation({
      container,
      renderer: "svg",
      loop: shouldLoop,
      autoplay: !reduced,
      animationData: data,
      rendererSettings: { preserveAspectRatio: "xMidYMid meet" },
    });

    if (reduced) anim.goToAndStop(anim.totalFrames - 1, true);

    return () => {
      anim?.destroy();
      anim = null;
    };
  });
</script>

<div
  bind:this={container}
  class="lottie-mark {className}"
  style:--lottie-w="{width}px"
  style:--lottie-h="{height}px"
  role={ariaLabel ? "img" : undefined}
  aria-label={ariaLabel}
  aria-hidden={ariaLabel ? undefined : "true"}
></div>

<style>
  .lottie-mark {
    width: var(--lottie-w);
    height: var(--lottie-h);
    display: inline-block;
    line-height: 0;
  }

  .lottie-mark :global(svg) {
    width: 100% !important;
    height: 100% !important;
    display: block;
  }
</style>
