// From sketch to ink: the sketch's lines drawn as with a pen, each in the
// time the page's data gives it, its fills and words coming as the pen
// reaches them; then the clean picture inked over it. Each line is
// measured here. A click starts it again; with less motion asked for,
// the clean picture stands, and `#t=4` holds it still 4 seconds in.
(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("play").textContent);
  const sketch = document.querySelector("#sketch > svg");
  const clean = document.getElementById("clean");
  const hidden = "defs, clipPath, mask, marker, pattern, symbol, style";
  const firstGroup = sketch.querySelector("g");
  // The elements the pen draws, as Omaspec found them: the paper, a
  // rectangle before the first group (every one, where there is none),
  // left out.
  const elements = [...sketch.querySelectorAll(data.drawn)].filter((el) => {
    if (el.closest(hidden)) return false;
    const paper = el.tagName === "rect"
      && (!firstGroup || (el.compareDocumentPosition(firstGroup) & Node.DOCUMENT_POSITION_FOLLOWING));
    return !paper;
  });
  const pen = elements.map((el, i) => {
    const stroke = data.strokes[i];
    const own = Number(el.getAttribute("opacity") || 1);
    if (Array.isArray(stroke) && el.getTotalLength) {
      // A dash starts again with each run of a line, a sketch's line drawn
      // twice: its runs are drawn at once, as far along as the longest.
      const length = el.getTotalLength() * (stroke[2] ?? 1);
      return { el, start: stroke[0], end: stroke[1], length, own };
    }
    return { el, at: Array.isArray(stroke) ? stroke[0] : stroke, own };
  });
  const clamp = (t) => Math.min(1, Math.max(0, t));
  const smooth = (t) => {
    const s = clamp(t);
    return s * s * (3 - 2 * s);
  };
  const inkAt = data.end + data.hold;
  const length = inkAt + data.ink + data.inked;
  // The ink in three: the sketch's words go, its lines and fills turn into
  // the clean picture's, and the clean words come; words of both at once
  // stood as ghosts over each other.
  const sketchWords = pen.filter((p) => p.el.tagName === "text");
  const cleanWords = [...clean.querySelectorAll("text")].map((el) => ({
    el,
    own: Number(el.getAttribute("opacity") || 1),
  }));
  const go = data.wordsGo;
  const come = data.wordsCome;
  const draw = (t) => {
    pen.forEach((p) => {
      if (p.length !== undefined) {
        const k = clamp((t - p.start) / Math.max(0.001, p.end - p.start));
        p.el.style.opacity = k > 0 ? p.own : 0;
        // Drawn: the line as it is, no dash left to cut its end short.
        p.el.style.strokeDasharray = k >= 1 ? "" : `${p.length} ${p.length}`;
        p.el.style.strokeDashoffset = k >= 1 ? "" : p.length * (1 - k);
      } else {
        p.el.style.opacity = p.own * clamp((t - p.at) / data.fade);
      }
    });
    const k = (t - inkAt) / data.ink;
    const gone = smooth(k / go);
    sketchWords.forEach((p) => {
      p.el.style.opacity = Number(p.el.style.opacity) * (1 - gone);
    });
    clean.style.opacity = smooth((k - go) / (1 - go - come));
    const came = smooth((k - (1 - come)) / come);
    cleanWords.forEach((w) => {
      w.el.style.opacity = w.own * came;
    });
  };
  let begun = null;
  const tick = (time) => {
    if (begun === null) begun = time;
    const t = (time - begun) / 1000;
    draw(Math.min(t, length));
    if (t >= length) begun = null;
    requestAnimationFrame(tick);
  };
  // `#t=4`: as it stands 4 seconds in, still.
  const still = /(?:^#|&)t=([\d.]+)/.exec(location.hash);
  if (still) {
    draw(Number(still[1]));
  } else if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    draw(length);
  } else {
    requestAnimationFrame(tick);
    document.getElementById("ink").addEventListener("click", () => {
      begun = null;
    });
  }
})();
