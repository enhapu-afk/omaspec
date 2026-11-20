// A lockstep: the scenario's messages run one after another, and as each
// reaches its receiver, the receiver's machine takes its step. Everything
// on the page is a function of the time alone, which the page's data says:
// a veil over what is still to come, the lights, the bars. A click starts
// it again; with less motion asked for, the end stands at once, and `#t=4`
// after the page's address holds it still 4 seconds in.
(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("play").textContent);
  const NS = "http://www.w3.org/2000/svg";
  // How long a state's light takes to come on or go out, a travelled line
  // stays lit, the veil takes to lift off the next message, a flash lasts.
  const FADE = 0.42;
  const LINGER = 0.52;
  const REVEAL = 0.46;
  const FLASH = 1.1;
  const VEIL = 0.62;
  const clamp = (t) => Math.min(1, Math.max(0, t));
  const easeOut = (t) => 1 - Math.pow(1 - clamp(t), 3);
  const el = (name, attrs, parent) => {
    const node = document.createElementNS(NS, name);
    for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
    if (parent) parent.appendChild(node);
    return node;
  };
  const line = (points) => points.map((p) => p.join(",")).join(" ");
  const length = (points) =>
    points.slice(1).reduce((sum, p, i) => sum + Math.hypot(p[0] - points[i][0], p[1] - points[i][1]), 0);
  // A layer of its own over a picture, in the picture's own units, with a
  // soft glow to draw lights with.
  let glows = 0;
  const layer = (svg) => {
    const id = `lockstep-glow-${glows++}`;
    const g = el("g", { class: "lockstep" }, svg);
    const defs = el("defs", {}, g);
    const filter = el("filter", { id, x: "-50%", y: "-50%", width: "200%", height: "200%" }, defs);
    el("feGaussianBlur", { stdDeviation: "3" }, filter);
    return { g, defs, glow: `url(#${id})` };
  };
  // How far a light along `points` has come by `k`: its line drawn so far.
  const run = (path, total, k) => {
    path.setAttribute("stroke-dasharray", `${total * clamp(k)} ${total + 1}`);
  };

  // The scenario: a veil over the messages still to come, lifted message
  // by message, and a light along each message as it is sent.
  const scenarioSvg = document.querySelector(".scenario svg");
  const scenario = layer(scenarioSvg);
  const messages = data.scenario.messages;
  const box = scenarioSvg.viewBox.baseVal;
  const veil = el("rect", { x: box.x, width: box.width, fill: data.scenario.paper, "fill-opacity": VEIL }, scenario.g);
  const band = el("rect", { rx: 6, "fill-opacity": 0 }, scenario.g);
  const sent = messages.map((m) => {
    const color = m.wrong ? data.wrong : data.accent;
    const total = length(m.points);
    const glow = el("polyline", {
      points: line(m.points), fill: "none", stroke: color, "stroke-width": 6,
      "stroke-linecap": "round", "stroke-linejoin": "round", filter: scenario.glow, opacity: 0,
    }, scenario.g);
    const path = el("polyline", {
      points: line(m.points), fill: "none", stroke: color, "stroke-width": 2.4,
      "stroke-linecap": "round", "stroke-linejoin": "round", opacity: 0,
    }, scenario.g);
    return { ...m, color, total, glow, path };
  });
  const scenarioBar = document.querySelector(".scenario .bar .head");

  // The machines: a veil with the states they are in let through it, a
  // glow around each, a light along each transition taken, and a flash
  // where they cannot follow.
  const machines = data.machines.map((m, index) => {
    const pane = document.querySelector(`[data-machine="${index}"]`);
    const svg = pane.querySelector("svg");
    const over = layer(svg);
    const view = svg.viewBox.baseVal;
    const maskId = `lockstep-holes-${index}`;
    const mask = el("mask", { id: maskId, maskUnits: "userSpaceOnUse", x: view.x, y: view.y, width: view.width, height: view.height }, over.defs);
    el("rect", { x: view.x, y: view.y, width: view.width, height: view.height, fill: "white" }, mask);
    const holes = m.states.map((s) => s && el("rect", { x: s[0] - 3, y: s[1] - 3, width: s[2] + 6, height: s[3] + 6, rx: s[4] + 3, fill: "black", "fill-opacity": 0 }, mask));
    el("rect", { x: view.x, y: view.y, width: view.width, height: view.height, fill: m.paper, "fill-opacity": VEIL, mask: `url(#${maskId})` }, over.g);
    const ring = (s, color, width, blur) => s && el("rect", {
      x: s[0] - 2, y: s[1] - 2, width: s[2] + 4, height: s[3] + 4, rx: s[4] + 2, fill: "none",
      stroke: color, "stroke-width": width, opacity: 0, ...(blur ? { filter: over.glow } : {}),
    }, over.g);
    const glows = m.states.map((s) => [ring(s, data.accent, 5, true), ring(s, data.accent, 2, false)]);
    const rings = m.states.map((s) => ring(s, data.wrong, 5, true));
    // Where nothing is known of where it is, its whole picture's edge.
    const edge = ring([view.x + 4, view.y + 4, view.width - 8, view.height - 8, 8], data.wrong, 4, true);
    const travels = m.travels.map((t) => {
      const total = length(t.points);
      const path = el("polyline", {
        points: line(t.points), fill: "none", stroke: data.accent, "stroke-width": 2.6,
        "stroke-linecap": "round", "stroke-linejoin": "round", opacity: 0,
      }, over.g);
      return { ...t, total, path };
    });
    const bar = pane.querySelector(".bar");
    return { ...m, holes, glows, rings, edge, travels, bar, shown: -1 };
  });

  // How bright a state's light is at `t`, from the times it went on and off.
  const level = (events, t) => {
    let on = false;
    let since = -Infinity;
    for (const [at, now] of events) {
      if (at > t) break;
      on = now;
      since = at;
    }
    const k = easeOut((t - since) / FADE);
    return on ? k : 1 - k;
  };
  // Two pulses, the second softer.
  const flashed = (k) => (k >= 1 || k < 0 ? 0 : Math.pow(Math.sin(k * 2 * Math.PI), 2) * (1 - 0.45 * k));

  const draw = (t) => {
    // The message sent last, and the veil lifted to below it.
    let current = -1;
    messages.forEach((m, i) => {
      if (m.start <= t) current = i;
    });
    const bottom = (i) => (i < 0 ? (messages[0] ? messages[0].top : 0) : messages[i].bottom + 4);
    const edge = current < 0
      ? bottom(-1)
      : bottom(current - 1) + (bottom(current) - bottom(current - 1)) * easeOut((t - messages[current].start) / REVEAL);
    veil.setAttribute("y", edge);
    veil.setAttribute("height", Math.max(0, box.y + box.height - edge));
    sent.forEach((m, i) => {
      if (i !== current) {
        // Only the message sent last is lit; those before it rest as drawn.
        m.path.setAttribute("opacity", 0);
        m.glow.setAttribute("opacity", 0);
        return;
      }
      const k = (t - m.start) / Math.max(0.01, m.arrive - m.start);
      run(m.path, m.total, easeOut(k));
      run(m.glow, m.total, easeOut(k));
      m.path.setAttribute("opacity", 1);
      m.glow.setAttribute("opacity", k < 1 ? 0.55 : 0.55 * (1 - easeOut((t - m.arrive) / LINGER)) + 0.15);
    });
    if (current >= 0) {
      const m = sent[current];
      const xs = m.points.map((p) => p[0]);
      band.setAttribute("x", Math.min(...xs) - 12);
      band.setAttribute("width", Math.max(...xs) - Math.min(...xs) + 24);
      band.setAttribute("y", m.top - 2);
      band.setAttribute("height", m.bottom - m.top + 4);
      band.setAttribute("fill", m.color);
      band.setAttribute("fill-opacity", (m.wrong ? 0.16 : 0.09) * easeOut((t - m.start) / REVEAL));
    } else {
      band.setAttribute("fill-opacity", 0);
    }
    scenarioBar.textContent = `● ${Math.max(0, current + 1)} / ${messages.length}`;

    machines.forEach((m) => {
      const levels = m.lights.map((events) => level(events, t));
      const flash = m.flashes.reduce((most, at) => Math.max(most, flashed((t - at) / FLASH)), 0);
      levels.forEach((lit, s) => {
        if (m.holes[s]) m.holes[s].setAttribute("fill-opacity", lit.toFixed(3));
        const [soft, sharp] = m.glows[s];
        if (soft) soft.setAttribute("opacity", (0.7 * lit).toFixed(3));
        if (sharp) sharp.setAttribute("opacity", lit.toFixed(3));
        if (m.rings[s]) m.rings[s].setAttribute("opacity", (flash * lit).toFixed(3));
      });
      const lit = levels.some((l, s) => l > 0.004 && m.rings[s]);
      m.edge.setAttribute("opacity", (lit ? 0 : flash).toFixed(3));
      m.travels.forEach((travel) => {
        const k = (t - travel.start) / Math.max(0.01, travel.took);
        if (k < 0) {
          travel.path.setAttribute("opacity", 0);
          return;
        }
        run(travel.path, travel.total, easeOut(k));
        const after = t - travel.start - travel.took;
        travel.path.setAttribute("opacity", after <= 0 ? 1 : Math.max(0, 1 - after / LINGER));
      });
      // The bar says what holds from its moment on.
      let shown = 0;
      m.bars.forEach((bar, i) => {
        if (bar.from <= t) shown = i;
      });
      if (shown !== m.shown) {
        m.shown = shown;
        const bar = m.bars[shown];
        m.bar.querySelector(".head").textContent = bar.at ? `● ${m.name}: ${bar.at}` : `○ ${m.name}`;
        m.bar.querySelector(".said").textContent = bar.said;
        m.bar.querySelector(".note").textContent = bar.note;
        m.bar.classList.toggle("wrong", bar.wrong);
      }
    });
  };

  const total = data.end + data.hold;
  let begun = null;
  const tick = (time) => {
    if (begun === null) begun = time;
    const t = (time - begun) / 1000;
    draw(Math.min(t, data.end + data.hold));
    if (t >= total) begun = null;
    requestAnimationFrame(tick);
  };
  // `#t=4`: the play as it stands 4 seconds in, still.
  const still = /(?:^#|&)t=([\d.]+)/.exec(location.hash);
  if (still) {
    draw(Number(still[1]));
  } else if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    draw(data.end + data.hold);
  } else {
    requestAnimationFrame(tick);
    document.getElementById("lockstep").addEventListener("click", () => {
      begun = null;
    });
  }
})();
