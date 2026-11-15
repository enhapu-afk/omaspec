"use strict";
// Omaspec's presentation (docs/adr/0031): the play is worked out by
// Omaspec and given here as data; this script only walks it and draws the
// pane's light over the picture. It reads no diagram and fetches nothing.

const Present = (() => {
  // The pane's veil and its soft edge, in the picture's units.
  const VEIL = 0.62;
  const SOFT = 56;

  /// A scenario's steps: 0 before the first message, n after the n-th.
  function scenario(steps) {
    return {
      at: 0,
      count: steps.length,
      next() {
        const moved = this.at < this.count;
        if (moved) this.at += 1;
        return moved;
      },
      back() {
        const moved = this.at > 0;
        if (moved) this.at -= 1;
        return moved;
      },
      home() {
        this.at = 0;
      },
      go(at) {
        this.at = Math.max(0, Math.min(this.count, at | 0));
      },
      /// Where the veil starts: the first message's top, then the bottom
      /// of the message played.
      edge() {
        if (this.count === 0) return Infinity;
        return this.at === 0 ? steps[0].top : steps[this.at - 1].bottom;
      },
      lit() {
        return this.at === 0 ? null : steps[this.at - 1];
      },
    };
  }

  /// A machine's places: where it is, and the way back.
  function machine(places) {
    return {
      place: 0,
      trail: [],
      here() {
        return places[this.place];
      },
      /// Takes option `i`: what it takes to light, transitions by beats,
      /// or null where it leads nowhere (past the places worked out) or is
      /// no option.
      take(i) {
        const option = this.here().options[i];
        if (!option || option.next === null || option.next === undefined) return null;
        this.trail.push(this.place);
        this.place = option.next;
        return option.taken;
      },
      back() {
        if (this.trail.length === 0) return false;
        this.place = this.trail.pop();
        return true;
      },
      home() {
        this.place = 0;
        this.trail = [];
      },
      go(place) {
        if (place >= 0 && place < places.length) this.place = place | 0;
      },
    };
  }

  return { VEIL, SOFT, scenario, machine };
})();

if (typeof module !== "undefined") module.exports = Present;

if (typeof document !== "undefined") {
  (() => {
    const NS = "http://www.w3.org/2000/svg";
    const data = JSON.parse(document.getElementById("play").textContent);
    const svg = document.querySelector("#diagram > svg");
    const bar = document.getElementById("bar");
    const style = getComputedStyle(document.documentElement);
    const paper = style.getPropertyValue("--paper").trim();
    const accent = style.getPropertyValue("--accent").trim();
    if (!svg) return;

    const box = svg.viewBox && svg.viewBox.baseVal && svg.viewBox.baseVal.width
      ? svg.viewBox.baseVal
      : { x: 0, y: 0, width: svg.width.baseVal.value, height: svg.height.baseVal.value };
    const make = (name, attrs, parent) => {
      const el = document.createElementNS(NS, name);
      for (const [key, value] of Object.entries(attrs || {})) el.setAttribute(key, value);
      if (parent) parent.appendChild(el);
      return el;
    };
    // The picture on white, as the pane shows it; a picture in a look
    // paints its own paper over it.
    svg.insertBefore(
      make("rect", { x: box.x, y: box.y, width: box.width, height: box.height, fill: "#ffffff" }),
      svg.firstChild,
    );
    const defs = make("defs", {}, svg);
    // In the picture's units: a straight line's own box has no height,
    // and a filter in its units drew nothing.
    const region = {
      filterUnits: "userSpaceOnUse",
      x: box.x - 40,
      y: box.y - 40,
      width: box.width + 80,
      height: box.height + 80,
    };
    const glow = make("filter", { id: "omaspec-glow", ...region }, defs);
    make("feGaussianBlur", { stdDeviation: "3", result: "blur" }, glow);
    const merge = make("feMerge", {}, glow);
    make("feMergeNode", { in: "blur" }, merge);
    make("feMergeNode", { in: "SourceGraphic" }, merge);
    const soft = make("filter", { id: "omaspec-soft", ...region }, defs);
    make("feGaussianBlur", { stdDeviation: "8" }, soft);
    // Over the picture, but its links take the clicks.
    const layer = make("g", { id: "omaspec-play", "pointer-events": "none" }, svg);
    let made = 0;

    const rect = (x, y, w, h) => `M${x},${y}h${w}v${h}h${-w}z`;
    /// A veil from `edge` down, in `color`, its first SOFT units fading
    /// in; `holes` (boxes in their own veil) left out of it.
    const veil = (x, y, w, h, edge, color, holes) => {
      const top = Math.max(y, edge);
      const bottom = y + h;
      if (top >= bottom) return;
      const id = `omaspec-veil-${made++}`;
      const fade = make("linearGradient", { id, gradientUnits: "userSpaceOnUse", x1: 0, y1: edge, x2: 0, y2: edge + Present.SOFT }, defs);
      make("stop", { offset: 0, "stop-color": color, "stop-opacity": 0 }, fade);
      make("stop", { offset: 1, "stop-color": color, "stop-opacity": Present.VEIL }, fade);
      let d = rect(x, top, w, bottom - top);
      for (const [hx, hy, hw, hh] of holes || []) {
        const from = Math.max(hy, top);
        const to = Math.min(hy + hh, bottom);
        if (to > from) d += rect(hx, from, hw, to - from);
      }
      make("path", { d, "fill-rule": "evenodd", fill: `url(#${id})` }, layer);
    };
    const line = (points) => points.map((p) => p.join(",")).join(" ");
    /// A light that runs along `points`, from `delay` milliseconds on.
    const light = (points, delay) => {
      const run = make("polyline", {
        points: line(points),
        fill: "none",
        stroke: accent,
        "stroke-width": 3,
        "stroke-linecap": "round",
        "stroke-linejoin": "round",
        filter: "url(#omaspec-glow)",
      }, layer);
      const length = run.getTotalLength ? run.getTotalLength() : 0;
      if (length > 0 && run.animate) {
        run.setAttribute("stroke-dasharray", `${length} ${length}`);
        run.setAttribute("stroke-dashoffset", length);
        const duration = Math.max(300, Math.min(1200, length * 2));
        run.animate([{ strokeDashoffset: length }, { strokeDashoffset: 0 }], {
          duration,
          delay,
          fill: "forwards",
          easing: "ease-out",
        });
        return delay + duration;
      }
      return delay;
    };
    /// Lights the transitions `beats` takes, each beat's at once and the
    /// next when the longest of them is through.
    const lights = (beats) => {
      let delay = 0;
      for (const beat of beats || []) {
        let end = delay;
        for (const t of beat) end = Math.max(end, light(data.routes[t], delay));
        delay = end;
      }
    };
    const clear = () => {
      layer.replaceChildren();
      for (const old of defs.querySelectorAll("linearGradient, mask")) old.remove();
      made = 0;
    };
    const say = (...parts) => bar.replaceChildren(...parts);
    const span = (cls, text) => {
      const el = document.createElement("span");
      el.className = cls;
      el.textContent = text;
      return el;
    };

    let go = () => {};
    let back = () => {};
    let home = () => {};
    let take = () => {};

    if (data.kind === "scenario" && data.steps.length > 0) {
      const play = Present.scenario(data.steps);
      const draw = () => {
        clear();
        const edge = play.edge();
        // A box dims toward its own fill, the rest toward the paper.
        const grounds = data.grounds.map((ground) => ground.rect);
        veil(box.x, box.y, box.width, box.height, edge, paper, grounds);
        for (const ground of data.grounds) {
          const [x, y, w, h] = ground.rect;
          veil(x, y, w, h, edge, ground.fill);
        }
        const step = play.lit();
        if (step) {
          const xs = step.points.map((p) => p[0]);
          make("rect", {
            x: Math.min(...xs) - 18,
            y: step.top - 6,
            width: Math.max(...xs) - Math.min(...xs) + 36,
            height: step.bottom - step.top + 12,
            rx: 10,
            fill: accent,
            "fill-opacity": 0.09,
            filter: "url(#omaspec-soft)",
          }, layer);
          light(step.points, 0);
        }
        say(
          span("at", `${play.at} / ${play.count}`),
          span("help", "→ next · ← back · Home start · F full screen"),
        );
      };
      go = () => play.next() && draw();
      back = () => play.back() && draw();
      home = () => {
        play.home();
        draw();
      };
      take = go;
      play.go(parseInt(location.hash.slice(1), 10) || 0);
      draw();
    } else if (data.kind === "machine") {
      const play = Present.machine(data.places);
      const draw = (beats) => {
        clear();
        const here = play.here();
        // The veil, with the states the machine is in left out of it, a
        // state inside another as well.
        const id = `omaspec-lit-${made++}`;
        const area = { x: box.x, y: box.y, width: box.width, height: box.height };
        const mask = make("mask", { id, maskUnits: "userSpaceOnUse", ...area }, defs);
        make("rect", { ...area, fill: "#ffffff" }, mask);
        for (const index of here.lit) {
          const s = data.states[index];
          const [x, y, w, h] = s.rect;
          const rx = Math.min(s.radius, w / 2, h / 2);
          make("rect", { x, y, width: w, height: h, rx, fill: "#000000" }, mask);
        }
        make("rect", { ...area, fill: paper, "fill-opacity": Present.VEIL, mask: `url(#${id})` }, layer);
        for (const index of here.lit) {
          const s = data.states[index];
          const [x, y, w, h] = s.rect;
          make("rect", {
            x, y, width: w, height: h, rx: s.radius, fill: "none", stroke: accent,
            "stroke-width": 2.5, filter: "url(#omaspec-glow)",
          }, layer);
        }
        lights(beats);
        const parts = [span("at", `● ${here.ended ? "ended" : here.at}`)];
        here.options.forEach((option, i) => {
          const button = document.createElement("button");
          if (i < 9) button.appendChild(span("key", String(i + 1)));
          button.appendChild(document.createTextNode(option.label));
          button.appendChild(span("to", `→ ${option.to}`));
          button.disabled = option.next === null || option.next === undefined;
          button.addEventListener("click", (event) => {
            event.stopPropagation();
            take(i);
          });
          parts.push(button);
        });
        if (here.options.length === 0 && !here.ended) parts.push(span("help", "nothing can happen here"));
        parts.push(span("help", "1–9 take · ← back · Home start · F full screen"));
        say(...parts);
      };
      take = (i) => {
        const beats = play.take(i);
        if (beats) draw(beats);
      };
      go = () => take(0);
      back = () => play.back() && draw();
      home = () => {
        play.home();
        draw(data.begun);
      };
      play.go(parseInt(location.hash.slice(1), 10) || 0);
      draw(play.place === 0 ? data.begun : []);
    } else {
      say(span("help", "F full screen"));
    }

    const full = () => {
      if (document.fullscreenElement) document.exitFullscreen();
      else if (document.documentElement.requestFullscreen) document.documentElement.requestFullscreen();
    };
    document.addEventListener("keydown", (event) => {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      const key = event.key;
      // A button that has the keys takes Enter and Space itself.
      if ((key === "Enter" || key === " ") && event.target.tagName === "BUTTON") return;
      if (["ArrowRight", "ArrowDown", " ", "PageDown", "Enter"].includes(key)) go();
      else if (["ArrowLeft", "ArrowUp", "PageUp", "Backspace"].includes(key)) back();
      else if (key === "Home") home();
      else if (key === "f" || key === "F") full();
      else if (/^[1-9]$/.test(key)) take(Number(key) - 1);
      else return;
      event.preventDefault();
    });
    document.getElementById("diagram").addEventListener("click", (event) => {
      if (event.target.closest && event.target.closest("a")) return;
      go();
    });
  })();
}
