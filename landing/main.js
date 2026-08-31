import './styles.css';
import '@fontsource/inter/latin-400.css';
import '@fontsource/inter/latin-500.css';
import '@fontsource/inter/latin-600.css';
import '@fontsource/inter/latin-700.css';
import '@fontsource/instrument-serif/latin-400.css';
import '@fontsource/instrument-serif/latin-400-italic.css';

import lottie from 'lottie-web';
import animationData from './animations/intro.json';

const SPRITE = 'icons/sprite.svg';
const ANIM_CLASSES = ['animate__animated', 'animate__fadeInUp'];

function reveal(el) {
  el.classList.add(...ANIM_CLASSES);
}

/* ── Reveal: hero elements on load ───────────────────────────── */
document.querySelectorAll('.reveal-load').forEach((el) => {
  const delay = parseInt(el.dataset.revealDelay || '0', 10);
  el.style.animationDelay = `${delay}ms`;
  reveal(el);
});

/* ── Reveal: everything else as it scrolls into view ─────────── */
const scrollTargets = document.querySelectorAll('.reveal:not(.reveal-load)');
if (!('IntersectionObserver' in window)) {
  scrollTargets.forEach(reveal);
} else {
  const ROW_BUCKET = 24;
  const STEP_MS = 50;
  const io = new IntersectionObserver((entries) => {
    const hit = entries.filter((e) => e.isIntersecting);
    if (!hit.length) return;
    const buckets = hit
      .map((e) => ({ entry: e, bucket: Math.round(e.boundingClientRect.top / ROW_BUCKET) }))
      .sort((a, b) => a.bucket - b.bucket);
    let order = 0;
    let prev = null;
    buckets.forEach((b) => {
      if (prev !== null && b.bucket !== prev) order++;
      prev = b.bucket;
      b.entry.target.style.animationDelay = `${order * STEP_MS}ms`;
      reveal(b.entry.target);
      io.unobserve(b.entry.target);
    });
  }, { threshold: 0.12, rootMargin: '0px 0px -40px 0px' });
  scrollTargets.forEach((el) => io.observe(el));
}

/* ── OS detection for primary download button ────────────────── */
const OS = {
  macos:   { label: 'Download for macOS',   arch: 'Apple Silicon · Universal build', icon: 'i-apple' },
  windows: { label: 'Download for Windows', arch: 'x64 · Windows 10+',                icon: 'i-windows' },
  linux:   { label: 'Download for Linux',   arch: 'AppImage · x86_64',                icon: 'i-linux' },
};

function detectOS() {
  const ua = (navigator.userAgent || '').toLowerCase();
  const platform = (navigator.platform || '').toLowerCase();
  if (/mac|iphone|ipad|ipod/.test(platform) || /mac os/.test(ua)) return 'macos';
  if (/win/.test(platform) || /windows/.test(ua)) return 'windows';
  if (/linux|x11/.test(platform) || /linux/.test(ua)) return 'linux';
  return null;
}

const os = detectOS();
const primary = document.getElementById('primary-download');
const primaryLabel = document.getElementById('primary-download-label');
const primaryIcon = document.getElementById('primary-download-icon');
const meta = document.getElementById('download-meta');
const altWrap = document.getElementById('download-alt');

if (os && primary && primaryLabel && primaryIcon && meta) {
  const cfg = OS[os];
  primary.setAttribute('data-os', os);
  primaryLabel.textContent = cfg.label;
  meta.textContent = cfg.arch;
  const use = primaryIcon.querySelector('use');
  if (use) use.setAttribute('href', `${SPRITE}#${cfg.icon}`);

  if (altWrap) {
    altWrap.setAttribute('data-detected', os);
    const detectedPill = altWrap.querySelector(`a[data-os="${os}"]`);
    if (detectedPill) detectedPill.hidden = true;
  }
}

/* ── Hero logo: Lottie intro, tinted to --accent ─────────────── */
function parseCssColor(css) {
  const s = (css || '').trim();
  if (s.charAt(0) === '#') {
    let hex = s.slice(1);
    if (hex.length === 3) hex = hex.split('').map((c) => c + c).join('');
    if (hex.length < 6) return null;
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);
    if ([r, g, b].some(Number.isNaN)) return null;
    return [r / 255, g / 255, b / 255];
  }
  const m = s.match(/rgba?\(([^)]+)\)/i);
  if (m) {
    const parts = m[1].split(/[\s,/]+/).filter(Boolean).slice(0, 3).map(Number);
    if (parts.length === 3 && parts.every((n) => !Number.isNaN(n))) {
      return [parts[0] / 255, parts[1] / 255, parts[2] / 255];
    }
  }
  return null;
}

function tintLottie(data, rgb) {
  (function visit(obj) {
    if (!obj || typeof obj !== 'object') return;
    if (Array.isArray(obj)) { obj.forEach(visit); return; }
    Object.keys(obj).forEach((key) => {
      const val = obj[key];
      if (key === 'c' && val && typeof val === 'object' && 'k' in val) {
        if (val.a === 0 && Array.isArray(val.k) && val.k.length >= 3) {
          val.k[0] = rgb[0]; val.k[1] = rgb[1]; val.k[2] = rgb[2];
        } else if (Array.isArray(val.k)) {
          val.k.forEach((kf) => {
            if (kf && Array.isArray(kf.s) && kf.s.length >= 3) {
              kf.s[0] = rgb[0]; kf.s[1] = rgb[1]; kf.s[2] = rgb[2];
            }
            if (kf && Array.isArray(kf.e) && kf.e.length >= 3) {
              kf.e[0] = rgb[0]; kf.e[1] = rgb[1]; kf.e[2] = rgb[2];
            }
          });
        }
      }
      visit(val);
    });
  })(data);
  return data;
}

function playHeroLogo() {
  const container = document.getElementById('hero-logo');
  if (!container) return;

  const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent');
  const rgb = parseCssColor(accent) || [0.831, 0.537, 0.227];
  const tinted = tintLottie(JSON.parse(JSON.stringify(animationData)), rgb);
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const fallback = container.querySelector('.hero-logo-fallback');
  if (fallback) fallback.remove();

  const anim = lottie.loadAnimation({
    container,
    renderer: 'svg',
    loop: false,
    autoplay: !reduced,
    animationData: tinted,
    rendererSettings: { preserveAspectRatio: 'xMidYMid meet' },
  });
  if (reduced) anim.goToAndStop(anim.totalFrames - 1, true);
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', playHeroLogo);
} else {
  playHeroLogo();
}
