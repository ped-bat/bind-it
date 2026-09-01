import './styles.css';
import '@fontsource/inter/latin-400.css';
import '@fontsource/inter/latin-500.css';
import '@fontsource/inter/latin-600.css';
import '@fontsource/instrument-serif/latin-400.css';
import '@fontsource/instrument-serif/latin-400-italic.css';

const SPRITE = 'icons/sprite.svg';

/* ── Reveal ───────────────────────────────────────────────────────
   Hero content reveals on load; everything below reveals as it
   scrolls in, staggered by visual row so a grid lights up in reading
   order rather than all at once.                                  */
function reveal(el, delay = 0) {
  el.style.animationDelay = `${delay}ms`;
  el.classList.add('is-revealed');
}

document.querySelectorAll('.reveal-load').forEach((el) => {
  reveal(el, parseInt(el.dataset.revealDelay || '0', 10));
});

const scrollTargets = document.querySelectorAll('.reveal:not(.reveal-load)');

if (!('IntersectionObserver' in window)) {
  scrollTargets.forEach((el) => reveal(el));
} else {
  const ROW_BUCKET = 24;
  const STEP_MS = 55;

  const io = new IntersectionObserver((entries) => {
    const hits = entries.filter((e) => e.isIntersecting);
    if (!hits.length) return;

    const rows = hits
      .map((e) => ({ el: e.target, row: Math.round(e.boundingClientRect.top / ROW_BUCKET) }))
      .sort((a, b) => a.row - b.row);

    let order = 0;
    let prevRow = null;
    rows.forEach(({ el, row }) => {
      if (prevRow !== null && row !== prevRow) order += 1;
      prevRow = row;
      reveal(el, order * STEP_MS);
      io.unobserve(el);
    });
    // Fire well before the element reaches the fold, so a fast scroll or an
    // anchor jump never lands on a section that is still invisible.
  }, { threshold: 0, rootMargin: '0px 0px 20% 0px' });

  scrollTargets.forEach((el) => io.observe(el));

  // Safety net: never leave anything stuck at opacity 0.
  window.setTimeout(() => {
    scrollTargets.forEach((el) => {
      if (!el.classList.contains('is-revealed')) reveal(el);
    });
  }, 4000);
}

/* ── Sticky header: hairline appears once the page moves ───────── */
const header = document.getElementById('site-header');
if (header) {
  const setScrolled = () => {
    header.dataset.scrolled = window.scrollY > 8 ? 'true' : 'false';
  };
  setScrolled();
  window.addEventListener('scroll', setScrolled, { passive: true });
}

/* ── Point the primary download at the visitor's platform ──────── */
const OS = {
  macos:   { label: 'Download for macOS',   icon: 'i-apple',   meta: 'Universal · macOS 11+ · No tracking' },
  windows: { label: 'Download for Windows', icon: 'i-windows', meta: 'x64 · Windows 10 and 11 · No tracking' },
  linux:   { label: 'Download for Linux',   icon: 'i-linux',   meta: 'AppImage · x86_64 · No tracking' },
};

function detectOS() {
  const ua = (navigator.userAgent || '').toLowerCase();
  const platform = (navigator.platform || '').toLowerCase();
  if (/mac/.test(platform) || /mac os/.test(ua)) return 'macos';
  if (/win/.test(platform) || /windows/.test(ua)) return 'windows';
  if (/linux|x11/.test(platform) || /linux/.test(ua)) return 'linux';
  return null;
}

const os = detectOS();
const primary = document.getElementById('primary-download');
const primaryLabel = document.getElementById('primary-download-label');
const primaryIcon = document.getElementById('primary-download-icon');
const metaLine = document.getElementById('download-meta');
const altWrap = document.getElementById('download-alt');

if (os && primary && primaryLabel && primaryIcon && metaLine) {
  const cfg = OS[os];
  primary.dataset.os = os;
  primaryLabel.textContent = cfg.label;
  metaLine.textContent = cfg.meta;

  const use = primaryIcon.querySelector('use');
  if (use) use.setAttribute('href', `${SPRITE}#${cfg.icon}`);

  // The detected platform is already the main button; drop it from the alternates.
  const detected = altWrap && altWrap.querySelector(`a[data-os="${os}"]`);
  if (detected) detected.hidden = true;
}
