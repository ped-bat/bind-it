(function () {
  'use strict';

  var SPRITE = 'icons/sprite.svg';
  var ANIM_CLASSES = ['animate__animated', 'animate__fadeInUp'];

  function reveal(el) {
    el.classList.add.apply(el.classList, ANIM_CLASSES);
  }

  /* ── Reveal: hero elements on load ───────────────────────────── */
  document.querySelectorAll('.reveal-load').forEach(function (el) {
    var delay = parseInt(el.dataset.revealDelay || '0', 10);
    el.style.animationDelay = delay + 'ms';
    reveal(el);
  });

  /* ── Reveal: everything else as it scrolls into view ─────────── */
  var scrollTargets = document.querySelectorAll('.reveal:not(.reveal-load)');
  if (!('IntersectionObserver' in window)) {
    scrollTargets.forEach(reveal);
  } else {
    var ROW_BUCKET = 24;   // items within this Y range share a row
    var STEP_MS = 50;
    var io = new IntersectionObserver(function (entries) {
      var hit = entries.filter(function (e) { return e.isIntersecting; });
      if (!hit.length) return;
      var buckets = hit
        .map(function (e) {
          return { entry: e, bucket: Math.round(e.boundingClientRect.top / ROW_BUCKET) };
        })
        .sort(function (a, b) { return a.bucket - b.bucket; });
      var order = 0;
      var prev = null;
      buckets.forEach(function (b) {
        if (prev !== null && b.bucket !== prev) order++;
        prev = b.bucket;
        b.entry.target.style.animationDelay = (order * STEP_MS) + 'ms';
        reveal(b.entry.target);
        io.unobserve(b.entry.target);
      });
    }, { threshold: 0.12, rootMargin: '0px 0px -40px 0px' });
    scrollTargets.forEach(function (el) { io.observe(el); });
  }

  /* ── OS detection for primary download button ────────────────── */
  var OS = {
    macos:   { label: 'Download for macOS',   arch: 'Apple Silicon · Universal build', icon: 'i-apple' },
    windows: { label: 'Download for Windows', arch: 'x64 · Windows 10+',                icon: 'i-windows' },
    linux:   { label: 'Download for Linux',   arch: 'AppImage · x86_64',                icon: 'i-linux' }
  };

  function detectOS() {
    var ua = (navigator.userAgent || '').toLowerCase();
    var platform = (navigator.platform || '').toLowerCase();
    if (/mac|iphone|ipad|ipod/.test(platform) || /mac os/.test(ua)) return 'macos';
    if (/win/.test(platform) || /windows/.test(ua)) return 'windows';
    if (/linux|x11/.test(platform) || /linux/.test(ua)) return 'linux';
    return null;
  }

  var os = detectOS();
  var primary = document.getElementById('primary-download');
  var primaryLabel = document.getElementById('primary-download-label');
  var primaryIcon = document.getElementById('primary-download-icon');
  var meta = document.getElementById('download-meta');
  var altWrap = document.getElementById('download-alt');

  if (os && primary && primaryLabel && primaryIcon && meta) {
    var cfg = OS[os];
    primary.setAttribute('data-os', os);
    primaryLabel.textContent = cfg.label;
    meta.textContent = cfg.arch;
    var use = primaryIcon.querySelector('use');
    if (use) use.setAttribute('href', SPRITE + '#' + cfg.icon);

    if (altWrap) {
      altWrap.setAttribute('data-detected', os);
      var detectedPill = altWrap.querySelector('a[data-os="' + os + '"]');
      if (detectedPill) detectedPill.hidden = true;
    }
  }

  /* ── Hero logo: Lottie intro, tinted to --accent ─────────────── */
  function parseCssColor(css) {
    var s = (css || '').trim();
    if (s.charAt(0) === '#') {
      var hex = s.slice(1);
      if (hex.length === 3) hex = hex.split('').map(function (c) { return c + c; }).join('');
      if (hex.length < 6) return null;
      var r = parseInt(hex.slice(0, 2), 16);
      var g = parseInt(hex.slice(2, 4), 16);
      var b = parseInt(hex.slice(4, 6), 16);
      if ([r, g, b].some(Number.isNaN)) return null;
      return [r / 255, g / 255, b / 255];
    }
    var m = s.match(/rgba?\(([^)]+)\)/i);
    if (m) {
      var parts = m[1].split(/[\s,\/]+/).filter(Boolean).slice(0, 3).map(Number);
      if (parts.length === 3 && parts.every(function (n) { return !Number.isNaN(n); })) {
        return [parts[0] / 255, parts[1] / 255, parts[2] / 255];
      }
    }
    return null;
  }

  function tintLottie(data, rgb) {
    (function visit(obj) {
      if (!obj || typeof obj !== 'object') return;
      if (Array.isArray(obj)) { obj.forEach(visit); return; }
      Object.keys(obj).forEach(function (key) {
        var val = obj[key];
        if (key === 'c' && val && typeof val === 'object' && 'k' in val) {
          if (val.a === 0 && Array.isArray(val.k) && val.k.length >= 3) {
            val.k[0] = rgb[0]; val.k[1] = rgb[1]; val.k[2] = rgb[2];
          } else if (Array.isArray(val.k)) {
            val.k.forEach(function (kf) {
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
    var container = document.getElementById('hero-logo');
    if (!container) return;
    if (typeof window.lottie === 'undefined' || !window.__introLottie) return;

    var accent = getComputedStyle(document.documentElement).getPropertyValue('--accent');
    var rgb = parseCssColor(accent) || [0.831, 0.537, 0.227];
    var tinted = tintLottie(JSON.parse(JSON.stringify(window.__introLottie)), rgb);
    var reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    var fallback = container.querySelector('.hero-logo-fallback');
    if (fallback) fallback.remove();

    var anim = window.lottie.loadAnimation({
      container: container,
      renderer: 'svg',
      loop: false,
      autoplay: !reduced,
      animationData: tinted,
      rendererSettings: { preserveAspectRatio: 'xMidYMid meet' }
    });
    if (reduced) anim.goToAndStop(anim.totalFrames - 1, true);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', playHeroLogo);
  } else {
    playHeroLogo();
  }
})();
