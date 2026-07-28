/*
 * The glyph field behind the page, and the transition between pages.
 *
 * One canvas, fixed, behind everything, filled with a grid of monospace
 * characters in the palette's own two colours.
 *
 * The page is empty. Glyphs exist only inside the waves crossing it: each wave
 * is a vertical band, bent by a sine so it reads as a wave rather than a bar,
 * that writes characters at its leading edge and clears them at its trailing
 * one. So at any moment a couple of bands carry glyphs and the rest of the page
 * carries none — the texture passes through rather than sitting there.
 *
 * Write rate and clear rate are the same, which is what keeps a band a band:
 * if it wrote faster than it erased it would fill the page behind itself.
 *
 * Deliberate choices worth keeping:
 *
 * - The field never sits under body text. The reading column carries an opaque
 *   surface (see `extra.css`), so the glyphs show in the margins only and no
 *   contrast pair measured in that file changes.
 * - Cost scales with the height of the viewport, not its area: a front touches
 *   a couple of cells per row and nothing else, so a 5K display costs about
 *   what a laptop does.
 * - `prefers-reduced-motion` draws the field once and stops. It does not remove
 *   it: the field is texture, and the animation is the part that moves.
 * - Navigation reuses the same field. Material's instant loading swaps the DOM
 *   without a reload, so the transition is an extra fast front launched across
 *   the page plus a scramble of the heading — the same wave vocabulary, and
 *   nothing is faded to white in between.
 */
(function () {
  "use strict";

  // Digits and dense punctuation, the same vocabulary the reference uses. `#`
  // and `8` read heavy, `.` and `'` read light, so a random pick already gives
  // the field its tonal variation without varying the ink.
  var GLYPHS = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ<>=+-*/\\|!?[]{}()#$%&@.,:;'\"^~";
  var CELL_W = 9;   // px, at the font size below
  var CELL_H = 14;
  var FONT = '11px ui-monospace, "SF Mono", SFMono-Regular, Menlo, monospace';

  var FRAME_MS = 70;          // ~14fps. Enough for a front to read as moving.

  // Cells a front rewrites per row it crosses. Two is enough to leave a visible
  // wake without turning the front into a solid stripe.
  var PER_ROW = 2;

  var canvas, ctx, cols, rows, grid, ink, paper, timer;
  var waves = [];
  var reduced = window.matchMedia("(prefers-reduced-motion: reduce)");

  function colours() {
    var style = getComputedStyle(document.body);
    // The field is drawn in the page's own two colours, read from the palette
    // rather than hard-coded, so the scheme toggle needs no second definition.
    // A ROLE token, not a ramp step: `--md-default-fg-color--lighter` is the
    // faint-ink step for whichever scheme is active, so it stays lighter than
    // the page on a dark one and darker on a light one. Reading a fixed ramp
    // step instead put a near-black ink on a near-black page — 1.08:1, which is
    // why the field was invisible rather than subtle.
    ink = style.getPropertyValue("--md-default-fg-color--lighter").trim() || "#8c8c8a";
    paper = style.getPropertyValue("--md-default-bg-color").trim() || "#181818";
  }

  function glyph() {
    return GLYPHS.charAt((Math.random() * GLYPHS.length) | 0);
  }

  function size() {
    var dpr = Math.min(window.devicePixelRatio || 1, 2);
    var w = window.innerWidth;
    var h = window.innerHeight;
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    canvas.style.width = w + "px";
    canvas.style.height = h + "px";
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    cols = Math.ceil(w / CELL_W);
    rows = Math.ceil(h / CELL_H);
    // Empty. Every glyph on screen is one a wave put there.
    grid = new Array(cols * rows);
    for (var i = 0; i < grid.length; i++) grid[i] = " ";
  }

  /* Repaint from `grid`. Called on mount, on resize and when the scheme
     toggles; the per-frame path touches individual cells instead. */
  function paintAll() {
    ctx.fillStyle = paper;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.font = FONT;
    ctx.textBaseline = "top";
    ctx.fillStyle = ink;
    for (var y = 0; y < rows; y++) {
      for (var x = 0; x < cols; x++) {
        var c = grid[y * cols + x];
        if (c !== " ") ctx.fillText(c, x * CELL_W, y * CELL_H);
      }
    }
  }

  /* A front is a vertical line at `x`, bent by a sine down its length, drifting
     at `speed` columns per frame. `width` is how many columns either side of it
     get rewritten, so a wide slow front reads as a swell and a narrow fast one
     as a ripple. `spawn` seeds one off the edge it travels from. */
  function spawn(opts) {
    var right = opts && opts.dir === -1;
    return {
      x: right ? cols + 8 : -8,
      dir: right ? -1 : 1,
      speed: (opts && opts.speed) || 0.45 + Math.random() * 0.4,
      width: (opts && opts.width) || 1 + Math.random() * 1.5,
      // How far behind the front the band is erased — the visible width of the
      // wave. Small: a handful of columns, so it reads as a ripple crossing the
      // page rather than a curtain drawn over it.
      band: (opts && opts.band) || 3 + Math.random() * 3,
      amp: 3 + Math.random() * 5,           // rows of bend, shallow
      freq: 0.10 + Math.random() * 0.16,    // bend wavelength, per row
      phase: Math.random() * Math.PI * 2,
      density: (opts && opts.density) || 0.55
    };
  }

  function reseed() {
    // Two standing waves crossing in opposite directions, plus the odd fast
    // ripple. More than a handful and the "wave" reads as noise again.
    // Five thin ripples rather than a couple of swells, staggered across the
    // page so they do not arrive together.
    waves = [
      spawn({ speed: 0.45, width: 1.5, band: 4 }),
      spawn({ dir: -1, speed: 0.7, width: 1, band: 3 }),
      spawn({ speed: 0.6, width: 2, band: 5 }),
      spawn({ dir: -1, speed: 0.5, width: 1.5, band: 4 }),
      spawn({ speed: 0.8, width: 1, band: 3 })
    ];
    // Stagger: push each one further along its own path so they are spread out
    // from the first frame instead of entering in a row.
    for (var i = 0; i < waves.length; i++) {
      waves[i].x += waves[i].dir * (cols / waves.length) * i;
    }
  }

  function cell(x, y, c) {
    if (x < 0 || x >= cols || y < 0 || y >= rows) return;
    grid[y * cols + x] = c;
    ctx.fillStyle = paper;
    ctx.fillRect(x * CELL_W, y * CELL_H, CELL_W, CELL_H);
    if (c !== " ") {
      ctx.fillStyle = ink;
      ctx.fillText(c, x * CELL_W, y * CELL_H);
    }
  }

  /* Erase every cell a retiring wave still owns. */
  function clearBand(w) {
    for (var y = 0; y < rows; y++) {
      var front = w.x + Math.sin(y * w.freq + w.phase) * w.amp * 0.35;
      var from = Math.round(front - w.dir * (w.band + 6));
      var to = Math.round(front + w.dir * (w.width + 2));
      var lo = Math.min(from, to), hi = Math.max(from, to);
      for (var x = lo; x <= hi; x++) {
        if (grid[y * cols + x] !== undefined && grid[y * cols + x] !== " ") cell(x, y, " ");
      }
    }
  }

  function step() {
    ctx.font = FONT;
    ctx.textBaseline = "top";

    for (var i = waves.length - 1; i >= 0; i--) {
      var w = waves[i];
      w.x += w.speed * w.dir;

      // Off the far edge: retire it and send a fresh one from the other side, so
      // the page always has something crossing it without accumulating waves.
      if (w.x < -(w.band + 16) || w.x > cols + w.band + 16) {
        // Sweep up whatever the band still has on screen, or a strip of glyphs
        // is left frozen at the edge for as long as the page is open.
        clearBand(w);
        waves.splice(i, 1);
        if (waves.length < 5) waves.push(spawn({ dir: -w.dir }));
        continue;
      }

      for (var y = 0; y < rows; y++) {
        // The bend: the front leads or lags by up to `amp` rows-worth of
        // columns, which is what makes it a wave and not a wipe.
        var front = w.x + Math.sin(y * w.freq + w.phase) * w.amp * 0.35;

        for (var n = 0; n < PER_ROW; n++) {
          // Leading edge: lay glyphs down.
          var dx = (Math.random() * 2 - 1) * w.width;
          // Falloff: the crest writes nearly every cell, the edges barely.
          if (Math.random() <= 1 - Math.abs(dx) / w.width) {
            cell(Math.round(front + dx), y, Math.random() < w.density ? glyph() : " ");
          }
        }

        // Trailing edge. This clears the whole strip the tail crossed this
        // frame, not a sample of it: sampling lets cells slip through, and what
        // slips through is left behind as speckle on a page that is meant to be
        // empty between the waves.
        var tail = front - w.dir * w.band;
        var sweep = Math.ceil(w.speed) + 1;
        for (var k = 0; k <= sweep; k++) {
          var tx = Math.round(tail - w.dir * k);
          if (grid[y * cols + tx] !== undefined && grid[y * cols + tx] !== " ") {
            cell(tx, y, " ");
          }
        }
      }
    }
  }

  function start() {
    stop();
    if (reduced.matches) return;
    timer = window.setInterval(step, FRAME_MS);
  }

  function stop() {
    if (timer) window.clearInterval(timer);
    timer = null;
  }

  /* The transition: the field surges for a few frames while the new page's
     heading resolves out of random glyphs. Both halves are the same idea, so a
     navigation reads as the field rewriting itself rather than as a page load. */
  function scramble(el) {
    if (!el || reduced.matches) return;
    var final = el.textContent;
    if (!final || final.length > 60) return;
    var frame = 0;
    var steps = 9;
    var id = window.setInterval(function () {
      frame++;
      var settled = Math.floor((final.length * frame) / steps);
      var out = final.slice(0, settled);
      for (var i = settled; i < final.length; i++) {
        out += final.charAt(i) === " " ? " " : glyph();
      }
      el.textContent = out;
      if (frame >= steps) {
        window.clearInterval(id);
        el.textContent = final;
      }
    }, 34);
  }

  /* An autoplaying video is motion, and this file is where the page's motion
     policy lives — so the demo recording answers to the same query the field
     does. `autoplay` is a load-time attribute and cannot be un-set after the
     fact, hence pausing rather than preventing. */
  function holdVideo() {
    var videos = document.querySelectorAll("video[autoplay]");
    for (var i = 0; i < videos.length; i++) {
      if (reduced.matches) {
        videos[i].pause();
        videos[i].controls = true;
      }
    }
  }

  function onPage() {
    colours();
    holdVideo();
    // The transition: one fast, wide front thrown across the page. Same
    // vocabulary as the idle waves, just travelling faster than they do.
    if (!reduced.matches && cols) {
      waves.push(spawn({ speed: 2.8, width: 3, band: 10, density: 0.65 }));
    }
    scramble(document.querySelector(".md-content__inner h1"));
  }

  function mount() {
    canvas = document.createElement("canvas");
    canvas.className = "site-glyphs";
    canvas.setAttribute("aria-hidden", "true");
    document.body.insertBefore(canvas, document.body.firstChild);
    ctx = canvas.getContext("2d", { alpha: false });

    colours();
    size();
    reseed();
    paintAll();
    start();

    var resizing;
    window.addEventListener("resize", function () {
      window.clearTimeout(resizing);
      resizing = window.setTimeout(function () {
        size();
        reseed();
        paintAll();
      }, 150);
    });

    // The scheme toggle rewrites the palette variables on <body>; re-read them
    // and repaint, or the field keeps the colours of the scheme it was born in.
    new MutationObserver(function () {
      colours();
      paintAll();
    }).observe(document.body, { attributes: true, attributeFilter: ["data-md-color-scheme"] });

    reduced.addEventListener("change", function () {
      if (reduced.matches) stop();
      else start();
      holdVideo();
    });
    holdVideo();

    document.addEventListener("visibilitychange", function () {
      // Nothing to animate behind another tab.
      if (document.hidden) stop();
      else start();
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", mount);
  } else {
    mount();
  }

  // Material's instant navigation emits `document$` on every page swap. When it
  // is absent (a hard load, or the feature turned off) the first-load path above
  // has already run, so this only ever adds the per-navigation behaviour.
  if (window.document$ && typeof window.document$.subscribe === "function") {
    window.document$.subscribe(onPage);
  }
})();
