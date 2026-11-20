// Traffic: many recorded traces flowing through a state machine at once.
// Each object is a dot that rests in its state and runs along the
// transition each of its lines takes; a transition glows the brighter the
// more dots have passed it, and once all have flowed, those no dot took
// blink in grey. Everything is a function of the time alone, which the
// page's data says. A click starts it again; with less motion asked for,
// the end stands at once, and `#t=4` holds it still 4 seconds in.
(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("play").textContent);
  const NS = "http://www.w3.org/2000/svg";
  const VEIL = 0.72;
  const c = data.colors;
  const svg = document.querySelector("#picture > svg");
  const view = svg.viewBox.baseVal;
  const said = document.querySelector("#caption .said");
  const wrongSaid = document.querySelector("#caption .wrong");
  const clamp = (t) => Math.min(1, Math.max(0, t));
  const ease = (t) => {
    const s = clamp(t);
    return s * s * (3 - 2 * s);
  };
  const el = (name, attrs, parent) => {
    const node = document.createElementNS(NS, name);
    for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
    parent.appendChild(node);
    return node;
  };
  const line = (points) => points.map((p) => p.join(",")).join(" ");
  // A pixel of the picture as drawn, in its own units: what a dot's size
  // and a line's width are given in.
  const px = (pixels) => {
    const ctm = svg.getScreenCTM();
    return pixels / (ctm && ctm.a > 0 ? ctm.a : 1);
  };

  const over = el("g", { class: "traffic" }, svg);
  el("rect", { x: view.x, y: view.y, width: view.width, height: view.height, fill: c.paper, "fill-opacity": VEIL }, over);
  const most = Math.max(1, ...data.wires.map((w) => w.passes.length));
  const wires = data.wires.map((wire) => {
    if (wire.points.length < 2) return null;
    const attrs = { points: line(wire.points), fill: "none", "stroke-linecap": "round", "stroke-linejoin": "round" };
    return {
      ...wire,
      glow: el("polyline", { ...attrs, stroke: c.accent, "stroke-opacity": 0 }, over),
      run: el("polyline", { ...attrs, stroke: c.light, "stroke-opacity": 0 }, over),
      never: el("polyline", { ...attrs, stroke: c.muted, "stroke-opacity": 0 }, over),
    };
  });
  const dots = data.dots.map((dot) => ({
    ...dot,
    ring: el("circle", { fill: "none", stroke: c.wrong, opacity: 0 }, over),
    mark: el("circle", { stroke: c.paper, opacity: 0 }, over),
  }));

  const passed = (wire, t) =>
    wire.passes.reduce((sum, [start, took]) => sum + clamp((t - start) / Math.max(0.01, took)), 0);
  const along = (points, share) => {
    const runs = points.slice(1).map((p, i) => Math.hypot(p[0] - points[i][0], p[1] - points[i][1]));
    let left = runs.reduce((a, b) => a + b, 0) * clamp(share);
    for (let i = 0; i < runs.length; i++) {
      if (runs[i] >= left) {
        const k = runs[i] > 0 ? left / runs[i] : 0;
        return [points[i][0] + (points[i + 1][0] - points[i][0]) * k, points[i][1] + (points[i + 1][1] - points[i][1]) * k];
      }
      left -= runs[i];
    }
    return points[points.length - 1];
  };
  const where = (dot, t) => {
    let leg = null;
    for (const one of dot.legs) if (one.start <= t) leg = one;
    if (!leg) return null;
    return along(leg.points, ease((t - leg.start) / Math.max(0.01, leg.took)));
  };

  const draw = (t) => {
    let taken = 0;
    let routed = 0;
    wires.forEach((wire) => {
      if (!wire) return;
      routed++;
      if (wire.passes.some(([start, took]) => start + took <= t)) taken++;
      const n = passed(wire, t);
      if (n > 0) {
        const k = Math.min(1, Math.sqrt(n / most));
        wire.glow.setAttribute("stroke-width", px(4 + 10 * k));
        wire.glow.setAttribute("stroke-opacity", 0.3 * k);
        wire.run.setAttribute("stroke-width", px(1.4 + 3.2 * k));
        wire.run.setAttribute("stroke-opacity", 0.45 + 0.55 * k);
        wire.never.setAttribute("stroke-opacity", 0);
        return;
      }
      wire.glow.setAttribute("stroke-opacity", 0);
      wire.run.setAttribute("stroke-opacity", 0);
      if (wire.passes.length === 0 && t > data.end) {
        const since = t - data.end;
        const blink = since < data.blink ? 0.25 + 0.75 * Math.abs(Math.sin(since * Math.PI * 2.5)) : 0.7;
        wire.never.setAttribute("stroke-width", px(2.4));
        wire.never.setAttribute("stroke-dasharray", `${px(6)} ${px(5)}`);
        wire.never.setAttribute("stroke-opacity", blink);
      } else {
        wire.never.setAttribute("stroke-opacity", 0);
      }
    });
    let wrong = 0;
    dots.forEach((dot) => {
      const at = where(dot, t);
      if (!at) {
        dot.mark.setAttribute("opacity", 0);
        dot.ring.setAttribute("opacity", 0);
        return;
      }
      const bad = dot.wrong !== null && dot.wrong <= t;
      if (bad) wrong++;
      dot.mark.setAttribute("cx", at[0]);
      dot.mark.setAttribute("cy", at[1]);
      dot.mark.setAttribute("r", px(5.2));
      dot.mark.setAttribute("stroke-width", px(1));
      dot.mark.setAttribute("fill", bad ? c.wrong : c.accent);
      dot.mark.setAttribute("opacity", 1);
      if (bad) {
        const k = ((t - dot.wrong) / 0.9) % 1;
        dot.ring.setAttribute("cx", at[0]);
        dot.ring.setAttribute("cy", at[1]);
        dot.ring.setAttribute("r", px(5 + 12 * k));
        dot.ring.setAttribute("stroke-width", px(1.6));
        dot.ring.setAttribute("opacity", (1 - k) * 0.9);
      } else {
        dot.ring.setAttribute("opacity", 0);
      }
    });
    const objects = data.objects === 1 ? "1 object" : `${data.objects} objects`;
    const of = data.machine ? ` of ${data.machine}` : "";
    said.textContent = `${objects}${of} · ${taken} of ${routed} transitions taken`;
    wrongSaid.textContent = wrong === 0 ? "" : wrong === 1 ? "1 deviation" : `${wrong} deviations`;
  };

  const length = data.end + data.blink + data.hold;
  let begun = null;
  const tick = (time) => {
    if (begun === null) begun = time;
    const t = (time - begun) / 1000;
    draw(Math.min(t, length));
    if (t >= length) begun = null;
    requestAnimationFrame(tick);
  };
  // `#t=4`: the traffic as it stands 4 seconds in, still.
  const still = /(?:^#|&)t=([\d.]+)/.exec(location.hash);
  if (still) {
    draw(Number(still[1]));
  } else if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    draw(length);
  } else {
    requestAnimationFrame(tick);
    document.getElementById("traffic").addEventListener("click", () => {
      begun = null;
    });
  }
})();
