// A theme sweep: the diagram in one theme after another, each sweeping
// over the one before from the left with a soft edge, the last into the
// first. Everything is a function of the time alone. A click starts it
// again; with less motion asked for, the first theme stands, and `#t=4`
// holds it still 4 seconds in.
(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("play").textContent);
  const themes = [...document.querySelectorAll(".theme")];
  const step = data.stay + data.sweep;
  const length = step * data.count;
  const smooth = (k) => {
    const s = Math.min(1, Math.max(0, k));
    return s * s * (3 - 2 * s);
  };
  // Only the theme shown and the one sweeping in are drawn: the one in
  // front masked to the left of the edge, its edge soft.
  const draw = (t) => {
    const time = ((t % length) + length) % length;
    const at = Math.floor(time / step);
    const into = time - at * step - data.stay;
    const next = (at + 1) % data.count;
    const k = into > 0 ? smooth(into / data.sweep) : 0;
    const edge = data.edge * 100;
    const front = -edge + k * (100 + 2 * edge);
    themes.forEach((theme, i) => {
      const shown = i === at || (i === next && k > 0);
      theme.style.visibility = shown ? "visible" : "hidden";
      theme.style.zIndex = i === next && k > 0 ? 2 : 1;
      if (i === next && k > 0) {
        const mask = `linear-gradient(to right, #000 ${front - edge}%, transparent ${front}%)`;
        theme.style.maskImage = mask;
        theme.style.webkitMaskImage = mask;
      } else {
        theme.style.maskImage = "none";
        theme.style.webkitMaskImage = "none";
      }
    });
  };
  let begun = null;
  const tick = (time) => {
    if (begun === null) begun = time;
    draw((time - begun) / 1000);
    requestAnimationFrame(tick);
  };
  // `#t=4`: the sweep as it stands 4 seconds in, still.
  const still = /(?:^#|&)t=([\d.]+)/.exec(location.hash);
  if (still) {
    draw(Number(still[1]));
  } else if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    draw(0);
  } else {
    requestAnimationFrame(tick);
    document.getElementById("sweep").addEventListener("click", () => {
      begun = null;
    });
  }
})();
