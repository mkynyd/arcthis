/* arcthis site — theme, copy, scroll reveals, scroll stack. */

(function () {
  'use strict';

  var reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  /* ---------- theme ---------- */

  var THEME_KEY = 'arcthis-theme';

  function systemTheme() {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }

  function currentPref() {
    try { return localStorage.getItem(THEME_KEY) || 'system'; } catch (e) { return 'system'; }
  }

  function applyTheme(pref) {
    var resolved = pref === 'system' ? systemTheme() : pref;
    document.documentElement.dataset.theme = resolved;
    document.querySelectorAll('[data-theme-toggle]').forEach(function (btn) {
      btn.setAttribute('aria-label', 'Theme: ' + pref);
      btn.dataset.state = pref;
    });
  }

  applyTheme(currentPref());

  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function () {
    if (currentPref() === 'system') applyTheme('system');
  });

  document.querySelectorAll('[data-theme-toggle]').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var order = ['system', 'light', 'dark'];
      var next = order[(order.indexOf(currentPref()) + 1) % order.length];
      try { localStorage.setItem(THEME_KEY, next); } catch (e) { /* private mode */ }
      applyTheme(next);
    });
  });

  /* ---------- language preference ---------- */

  document.querySelectorAll('.lang-switch[data-lang]').forEach(function (a) {
    a.addEventListener('click', function () {
      try { localStorage.setItem('arcthis-lang', a.getAttribute('data-lang')); } catch (e) { /* private mode */ }
    });
  });

  /* ---------- copy buttons ---------- */

  function flashCopied(btn, label) {
    var zh = (document.documentElement.lang || '').toLowerCase().indexOf('zh') === 0;
    btn.classList.add('copied');
    var textNode = btn.querySelector('[data-copy-label]');
    var prev = textNode ? textNode.textContent : null;
    if (textNode) textNode.textContent = zh ? '已复制' : 'Copied';
    var live = document.getElementById('copy-live');
    if (live) live.textContent = zh ? (label || '命令') + '已复制到剪贴板' : (label || 'Command') + ' copied to clipboard';
    window.setTimeout(function () {
      btn.classList.remove('copied');
      if (textNode) textNode.textContent = prev;
    }, 1500);
  }

  document.querySelectorAll('[data-copy]').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var text = btn.getAttribute('data-copy');
      var done = function () { flashCopied(btn, btn.getAttribute('data-copy-name')); };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(done, done);
      } else {
        var ta = document.createElement('textarea');
        ta.value = text;
        ta.style.position = 'fixed';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.select();
        try { document.execCommand('copy'); } catch (e) { /* noop */ }
        document.body.removeChild(ta);
        done();
      }
    });
  });

  /* ---------- GSAP reveals ---------- */

  var hasGsap = typeof window.gsap !== 'undefined' && typeof window.ScrollTrigger !== 'undefined';
  if (hasGsap) {
    window.gsap.registerPlugin(window.ScrollTrigger);
  }

  if (hasGsap && !reduceMotion) {
    window.gsap.utils.toArray('.reveal').forEach(function (el) {
      window.gsap.from(el, {
        y: 26,
        opacity: 0,
        duration: 0.6,
        ease: 'power3.out',
        scrollTrigger: { trigger: el, start: 'top 88%', once: true }
      });
    });
    window.gsap.utils.toArray('.reveal-stagger').forEach(function (group) {
      window.gsap.from(group.children, {
        y: 18,
        opacity: 0,
        duration: 0.5,
        ease: 'power3.out',
        stagger: 0.07,
        scrollTrigger: { trigger: group, start: 'top 88%', once: true }
      });
    });
  }

  /* ---------- scroll stack (port of React Bits ScrollStack, window scroll) ---------- */

  var stackInner = document.querySelector('.stack-inner');
  var wideEnough = window.matchMedia('(min-width: 1024px)');

  if (stackInner && wideEnough.matches) {
    var cards = Array.prototype.slice.call(stackInner.querySelectorAll('.stack-card'));
    var endEl = stackInner.querySelector('.stack-end');

    var CFG = {
      itemScale: 0.03,
      itemStackDistance: 30,
      stackPosition: 0.2,   // fraction of viewport height
      scaleEndPosition: 0.1,
      baseScale: 0.85
    };

    var cardTops = [];
    var endTop = 0;
    var lastTransforms = new Map();
    var ticking = false;

    function measure() {
      // reset transforms and heights so layout positions are clean
      cards.forEach(function (card) { card.style.transform = 'none'; card.style.height = 'auto'; });
      var maxH = 0;
      cards.forEach(function (card) {
        maxH = Math.max(maxH, card.getBoundingClientRect().height);
      });
      cards.forEach(function (card) { card.style.height = Math.ceil(maxH) + 'px'; });
      cardTops = cards.map(function (card) {
        return card.getBoundingClientRect().top + window.scrollY;
      });
      endTop = endEl ? endEl.getBoundingClientRect().top + window.scrollY : 0;
      lastTransforms.clear();
      update();
    }

    function progress(scrollTop, start, end) {
      if (scrollTop < start) return 0;
      if (scrollTop > end) return 1;
      return (scrollTop - start) / (end - start);
    }

    function update() {
      ticking = false;
      var scrollTop = window.scrollY;
      var vh = window.innerHeight;
      var stackPosPx = CFG.stackPosition * vh;
      var scaleEndPx = CFG.scaleEndPosition * vh;
      var pinEnd = endTop - vh / 2;

      cards.forEach(function (card, i) {
        var cardTop = cardTops[i];
        var triggerStart = cardTop - stackPosPx - CFG.itemStackDistance * i;
        var triggerEnd = cardTop - scaleEndPx;
        var pinStart = triggerStart;

        var scaleProgress = reduceMotion ? 1 : progress(scrollTop, triggerStart, triggerEnd);
        var targetScale = CFG.baseScale + i * CFG.itemScale;
        var scale = 1 - scaleProgress * (1 - targetScale);

        var translateY = 0;
        if (scrollTop >= pinStart && scrollTop <= pinEnd) {
          translateY = scrollTop - cardTop + stackPosPx + CFG.itemStackDistance * i;
        } else if (scrollTop > pinEnd) {
          translateY = pinEnd - cardTop + stackPosPx + CFG.itemStackDistance * i;
        }

        var next = {
          y: Math.round(translateY * 100) / 100,
          s: Math.round(scale * 1000) / 1000
        };
        var last = lastTransforms.get(i);
        if (!last || Math.abs(last.y - next.y) > 0.1 || Math.abs(last.s - next.s) > 0.001) {
          card.style.transform = 'translate3d(0, ' + next.y + 'px, 0) scale(' + next.s + ')';
          lastTransforms.set(i, next);
        }
      });
    }

    function requestUpdate() {
      if (!ticking) {
        ticking = true;
        window.requestAnimationFrame(update);
      }
    }

    /* ---------- lenis smooth scroll (landing only) ---------- */

    var lenis = null;
    if (typeof window.Lenis !== 'undefined' && !reduceMotion) {
      lenis = new window.Lenis({
        duration: 1.1,
        easing: function (t) { return Math.min(1, 1.001 - Math.pow(2, -10 * t)); },
        smoothWheel: true
      });
      lenis.on('scroll', update);
      if (hasGsap) {
        lenis.on('scroll', window.ScrollTrigger.update);
        window.gsap.ticker.add(function (time) { lenis.raf(time * 1000); });
        window.gsap.ticker.lagSmoothing(0);
      } else {
        var raf = function (time) { lenis.raf(time); window.requestAnimationFrame(raf); };
        window.requestAnimationFrame(raf);
      }
    } else {
      window.addEventListener('scroll', requestUpdate, { passive: true });
    }

    // in-page anchors through lenis
    document.querySelectorAll('a[href^="#"]').forEach(function (a) {
      a.addEventListener('click', function (ev) {
        var id = a.getAttribute('href');
        if (id.length < 2) return;
        var target = document.querySelector(id);
        if (!target) return;
        ev.preventDefault();
        if (lenis) {
          lenis.scrollTo(target, { offset: -72 });
        } else {
          target.scrollIntoView({ behavior: reduceMotion ? 'auto' : 'smooth' });
        }
      });
    });

    measure();
    window.addEventListener('resize', measure);
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(measure);
    }
    window.addEventListener('load', measure);
  } else {
    // narrow viewports / no stack: plain anchor scrolling
    document.querySelectorAll('a[href^="#"]').forEach(function (a) {
      a.addEventListener('click', function (ev) {
        var id = a.getAttribute('href');
        if (id.length < 2) return;
        var target = document.querySelector(id);
        if (!target) return;
        ev.preventDefault();
        target.scrollIntoView({ behavior: reduceMotion ? 'auto' : 'smooth' });
      });
    });
  }
})();
