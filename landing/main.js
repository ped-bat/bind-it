(function () {
  'use strict';

  // Fade-in observer
  var targets = document.querySelectorAll('.fade-in');
  if (!('IntersectionObserver' in window)) {
    targets.forEach(function (el) { el.classList.add('visible'); });
  } else {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add('visible');
          io.unobserve(entry.target);
        }
      });
    }, { threshold: 0.1, rootMargin: '0px 0px -40px 0px' });
    targets.forEach(function (el) { io.observe(el); });
  }

  // Sticky nav border on scroll
  var nav = document.querySelector('.nav');
  if (nav) {
    var onScroll = function () {
      nav.classList.toggle('scrolled', window.scrollY > 4);
    };
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
  }

  // OS detection
  function detectOS() {
    var ua = (navigator.userAgent || '').toLowerCase();
    var platform = (navigator.platform || '').toLowerCase();
    if (/mac|iphone|ipad|ipod/.test(platform) || /mac os/.test(ua)) return 'macos';
    if (/win/.test(platform) || /windows/.test(ua)) return 'windows';
    if (/linux|x11/.test(platform) || /linux/.test(ua)) return 'linux';
    return null;
  }

  var labels = {
    macos: 'Download for macOS',
    windows: 'Download for Windows',
    linux: 'Download for Linux'
  };
  var arches = {
    macos: 'Apple Silicon · Universal build',
    windows: 'x64 · Windows 10+',
    linux: 'AppImage · x86_64'
  };
  var iconIds = {
    macos: 'i-apple',
    windows: 'i-windows',
    linux: 'i-linux'
  };

  var os = detectOS();
  var primary = document.getElementById('primary-download');
  var primaryLabel = document.getElementById('primary-download-label');
  var primaryIcon = document.getElementById('primary-download-icon');
  var meta = document.getElementById('download-meta');
  var altWrap = document.getElementById('download-alt');

  if (os && primary && primaryLabel && primaryIcon && meta) {
    primary.setAttribute('data-os', os);
    primaryLabel.textContent = labels[os];
    meta.textContent = arches[os];
    var use = primaryIcon.querySelector('use');
    if (use) use.setAttribute('href', '#' + iconIds[os]);

    // Hide detected OS from the "Also for" row
    if (altWrap) {
      altWrap.setAttribute('data-detected', os);
      var detectedPill = altWrap.querySelector('a[data-os="' + os + '"]');
      if (detectedPill) {
        detectedPill.hidden = true;
        // Also drop an adjacent separator for clean spacing
        var next = detectedPill.nextElementSibling;
        var prev = detectedPill.previousElementSibling;
        if (next && next.classList.contains('sep')) next.remove();
        else if (prev && prev.classList.contains('sep')) prev.remove();
      }
    }
  }
})();
