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
      /// of the message played; past the last, nowhere: nothing is to
      /// come under it, the lifelines' feet neither (REQ-139).
      edge() {
        if (this.count === 0 || this.at >= this.count) return Infinity;
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
      /// Takes option `i`: the steps it takes, each with when it lights,
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

  // A glide between two pictures (docs/adr/0029), as the pane times it:
  // the old lines have gone by LINE_OUT, the new ones come from LINE_IN.
  const LINE_OUT = 0.45;
  const LINE_IN = 0.55;
  const lerp = (a, b, t) => a + (b - a) * t;
  const mix = (a, b, t) => a.map((v, i) => lerp(v, b[i], t));
  const smooth = (t) => {
    const k = Math.max(0, Math.min(1, t));
    return k * k * (3 - 2 * k);
  };
  const ease = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
  const gone = (t) => (t >= LINE_OUT ? 0 : 1 - smooth(t / LINE_OUT));
  const come = (t) => (t <= LINE_IN ? 0 : smooth((t - LINE_IN) / (1 - LINE_IN)));
  /// A picture of `size` as large as fits `view`, in its middle.
  const fit = ([w, h], [vw, vh]) => {
    const k = Math.min(vw / Math.max(w, 1e-3), vh / Math.max(h, 1e-3));
    return [(vw - w * k) / 2, (vh - h * k) / 2, w * k, h * k];
  };
  /// `rect` of a picture of `size`, where that picture lies on `paper`.
  const onto = ([x, y, w, h], [sw, sh], [px, py, pw, ph]) =>
    [px + (x * pw) / sw, py + (y * ph) / sh, (w * pw) / sw, (h * ph) / sh];
  const point = ([x, y], size, paper) => onto([x, y, 0, 0], size, paper).slice(0, 2);

  /// Where everything of `glide` is when it has gone `t` of its way (0 the
  /// picture before, 1 the new one), on a `view` wide and high: the sheet,
  /// how strong each ground is, each box's place and how strong its old
  /// and its new picture are, each line's points, marks and strength.
  /// `sizes` are the two pictures' own.
  function frame(glide, sizes, view, t) {
    const [was, is] = sizes;
    const papers = [fit(was, view), fit(is, view)];
    const paper = mix(papers[0], papers[1], t);
    const between = (from, to) => mix(onto(from, was, papers[0]), onto(to, is, papers[1]), t);
    // How wide a unit of each picture is on its paper, and on the way.
    const unit = (size, on) => on[2] / Math.max(size[0], 1e-3);
    const scale = lerp(unit(was, papers[0]), unit(is, papers[1]), t);
    const boxes = glide.boxes.map((box) => {
      const parent = box.into ? between(box.into[0], box.into[1]) : null;
      if (box.role === "stay") return { rect: between(box.from, box.to), old: 1, now: t, scale };
      if (box.role === "leave") {
        const own = onto(box.from, was, paper);
        return { rect: parent ? mix(own, parent, smooth(t / LINE_OUT)) : own, old: gone(t), now: 0, scale: unit(was, paper) };
      }
      const own = onto(box.to, is, paper);
      const k = smooth((t - LINE_IN) / (1 - LINE_IN));
      return { rect: parent ? mix(parent, own, k) : own, old: 0, now: come(t), scale: unit(is, paper) };
    });
    const lines = glide.lines.map((line) => {
      const shaped = (route, size, on) => ({
        points: route.points.map((p) => point(p, size, on)),
        heads: route.heads.map((head) => ({ ...head, points: head.points.map((p) => point(p, size, on)) })),
      });
      if (line.from && line.to) {
        const a = shaped(line.from, was, papers[0]);
        const b = shaped(line.to, is, papers[1]);
        // Marks of one shape travel; others change where the line is
        // half way.
        const alike = a.heads.length === b.heads.length
          && a.heads.every((head, i) => head.points.length === b.heads[i].points.length);
        const heads = alike
          ? a.heads.map((head, i) => ({ ...(t < 0.5 ? head : b.heads[i]), points: head.points.map((p, j) => mix(p, b.heads[i].points[j], t)) }))
          : (t < 0.5 ? a : b).heads;
        return {
          points: a.points.map((p, i) => mix(p, b.points[Math.min(i, b.points.length - 1)], t)),
          heads,
          color: line.colors[t < 0.5 ? 0 : 1],
          width: line.width * scale,
          alpha: line.fold === "leave" ? gone(t) : line.fold === "arrive" ? come(t) : 1,
        };
      }
      const old = Boolean(line.from);
      const size = old ? was : is;
      return {
        ...shaped(old ? line.from : line.to, size, paper),
        color: line.colors[old ? 0 : 1],
        width: line.width * unit(size, paper),
        alpha: old ? gone(t) : come(t),
      };
    });
    return { paper, grounds: [gone(t), come(t)], boxes, lines };
  }

  /// Where a line's words are: `was` and `is` their places in the two
  /// pictures (null where a picture has none). Words the line keeps
  /// travel, the new over the old; others go or come with their picture.
  function words(line, was, is, sizes, view, t) {
    const papers = [fit(sizes[0], view), fit(sizes[1], view)];
    const paper = mix(papers[0], papers[1], t);
    if (line.fold) return { old: null, now: null };
    const kept = line.from && line.to;
    if (kept && was && is) {
      const rect = mix(onto(was, sizes[0], papers[0]), onto(is, sizes[1], papers[1]), t);
      return { old: { rect, alpha: t < 1 ? 1 : 0 }, now: { rect, alpha: t } };
    }
    const on = kept ? papers : [paper, paper];
    return {
      old: was && line.from ? { rect: onto(was, sizes[0], on[0]), alpha: gone(t) } : null,
      now: is && line.to ? { rect: onto(is, sizes[1], on[1]), alpha: come(t) } : null,
    };
  }

  return { VEIL, SOFT, scenario, machine, frame, words, fit, onto, ease, gone, come };
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
    // paints its own paper over it, and one whose root has a background of
    // its own (as the Java PlantUML writes it) keeps that.
    const own = getComputedStyle(svg).backgroundColor;
    const bare = !own || own === "transparent" || /^rgba\(.*,\s*0\)$/.test(own);
    /// Lays the ground under `picture`, whose own box is `size`; none
    /// under one that paints its whole box itself, as a picture in a look
    /// does: the ground's edge would show around that paper. The color
    /// the picture stands on.
    const grounded = (picture, size) => {
      const first = [...picture.children].find((el) => !["metadata", "style", "defs", "title"].includes(el.tagName));
      const length = (name) => (first.getAttribute(name) === null ? 0 : Number(first.getAttribute(name)));
      const painted = first && first.tagName === "rect"
        && !["none", "transparent", null].includes(first.getAttribute("fill"))
        && length("x") <= size.x && length("y") <= size.y
        && length("x") + length("width") >= size.x + size.width
        && length("y") + length("height") >= size.y + size.height;
      if (painted) return first.getAttribute("fill");
      const fill = bare ? "#ffffff" : own;
      picture.insertBefore(
        make("rect", { x: size.x, y: size.y, width: size.width, height: size.height, fill }),
        picture.firstChild,
      );
      return fill;
    };
    const under = grounded(svg, box);
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
    /// A light that runs along `points` from `delay` milliseconds on, for
    /// `duration` (as the pane's, worked out with the play).
    const light = (points, delay, duration) => {
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
        run.animate([{ strokeDashoffset: length }, { strokeDashoffset: 0 }], {
          duration: Math.max(1, duration),
          delay,
          fill: "forwards",
          easing: "ease-in-out",
        });
      }
    };
    // What waits to happen on the page: a state lit as its light arrives.
    let timers = [];
    const clear = () => {
      for (const timer of timers) clearTimeout(timer);
      timers = [];
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
        // A box dims toward its own fill, the rest toward the paper; a
        // box inside a box is left out of the outer one's veil, so each
        // place has one veil.
        const rects = data.grounds.map((ground) => ground.rect);
        const within = (a, b) =>
          a !== b && a[0] >= b[0] && a[1] >= b[1] && a[0] + a[2] <= b[0] + b[2] && a[1] + a[3] <= b[1] + b[3];
        const inner = (outer) =>
          rects.filter((r) => within(r, outer) && !rects.some((m) => within(r, m) && within(m, outer)));
        veil(box.x, box.y, box.width, box.height, edge, paper, rects.filter((r) => !rects.some((o) => within(r, o))));
        for (const ground of data.grounds) {
          const [x, y, w, h] = ground.rect;
          veil(x, y, w, h, edge, ground.fill, inner(ground.rect));
        }
        const step = play.lit();
        if (step) {
          // Across its arrow and the boxes lit with it: a note beside it,
          // or the note that is the step.
          const xs = step.points.map((p) => p[0])
            .concat((step.also || []).flatMap(([x, , w]) => [x, x + w]));
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
          // A note has no arrow for a light: its band alone lights it.
          if (step.points.length > 1) light(step.points, 0, step.took);
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
      // The states lit: the veil leaves them out, a state inside another
      // as well, and each shines in the accent.
      const shine = (group, lit, words) => {
        group.replaceChildren();
        for (const old of defs.querySelectorAll("mask")) old.remove();
        const id = `omaspec-lit-${made++}`;
        const area = { x: box.x, y: box.y, width: box.width, height: box.height };
        const mask = make("mask", { id, maskUnits: "userSpaceOnUse", ...area }, defs);
        make("rect", { ...area, fill: "#ffffff" }, mask);
        for (const index of lit) {
          const s = data.states[index];
          const [x, y, w, h] = s.rect;
          const rx = Math.min(s.radius, w / 2, h / 2);
          make("rect", { x, y, width: w, height: h, rx, fill: "#000000" }, mask);
        }
        // The words of what can happen next come through as well.
        for (const [x, y, w, h] of words || []) {
          make("rect", { x: x - 2, y: y - 2, width: w + 4, height: h + 4, fill: "#000000" }, mask);
        }
        make("rect", { ...area, fill: paper, "fill-opacity": Present.VEIL, mask: `url(#${id})` }, group);
        for (const index of lit) {
          const s = data.states[index];
          const [x, y, w, h] = s.rect;
          make("rect", {
            x, y, width: w, height: h, rx: s.radius, fill: "none", stroke: accent,
            "stroke-width": 2.5, filter: "url(#omaspec-glow)",
          }, group);
        }
      };
      /// The place the machine is at, reached by `steps` from where the
      /// states `before` were lit: each step's light sets off and travels
      /// when the play worked out, the states it leaves go dark as it sets
      /// off, those it enters light as it arrives, as in the pane. Without
      /// steps, the place as it is.
      const draw = (steps, before) => {
        clear();
        const here = play.here();
        const group = make("g", {}, layer);
        const moving = steps && steps.length > 0;
        const lit = new Set(moving ? before : here.lit);
        const offered = here.options.flatMap((option) => option.words || []);
        shine(group, lit, moving ? [] : offered);
        let through = 0;
        for (const step of steps || []) {
          light(data.routes[step.t], step.at, step.took);
          through = Math.max(through, step.at + step.took);
          timers.push(setTimeout(() => {
            for (const state of step.left) lit.delete(state);
            shine(group, lit);
          }, step.at));
          timers.push(setTimeout(() => {
            for (const state of step.entered) lit.add(state);
            shine(group, lit);
          }, step.at + step.took));
        }
        if (moving) timers.push(setTimeout(() => shine(group, here.lit, offered), through));
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
        const before = play.here().lit;
        const steps = play.take(i);
        if (steps) draw(steps, before);
      };
      go = () => take(0);
      back = () => play.back() && draw();
      home = () => {
        play.home();
        draw(data.begun, []);
      };
      play.go(parseInt(location.hash.slice(1), 10) || 0);
      if (play.place === 0) draw(data.begun, []);
      else draw();
    } else if ((data.kind === "fold" || data.kind === "history") && data.glides.length > 0) {
      // One picture for each step: 0 with nothing folded, n with the first
      // n containers. Only the one the play is at shows; a glide draws a
      // sheet of its own over them.
      const stage = document.getElementById("diagram");
      const pictures = [...stage.querySelectorAll(":scope > svg")];
      const play = Present.scenario(data.glides);
      const calm = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      // A diagram's history: each commit stays a little longer, and the
      // last starts it again.
      const history = data.kind === "history";
      const TOOK = history ? 700 : 400;
      const HOLD = history ? 1300 : 900;
      // What each picture stands on: a mark that is hollow is filled with it.
      const unders = [under, ...pictures.slice(1).map((picture) => grounded(picture, picture.viewBox.baseVal))];
      /// What moves the groups around `group` in its picture: the
      /// `transform`s of those above it, the outermost first, as one text
      /// and as one matrix.
      const lifted = (picture, group) => {
        const above = [];
        for (let el = group.parentNode; el && el !== picture; el = el.parentNode) above.unshift(el);
        let matrix = picture.createSVGMatrix();
        const text = [];
        for (const el of above) {
          const own = el.transform && el.transform.baseVal.consolidate();
          if (!own) continue;
          matrix = matrix.multiply(own.matrix);
          text.push(el.getAttribute("transform"));
        }
        return { matrix, text: text.join(" ") };
      };
      const show = (at) => pictures.forEach((picture, i) => picture.classList.toggle("away", i !== at));
      /// The groups of the part whose class is `name`, in its picture.
      const groups = (picture, name) => [...picture.getElementsByClassName(name)].filter((g) => g.tagName === "g");
      /// The class of `group` that names its part, none for a page's own.
      const partOf = (group) => {
        const name = group.classList[0];
        return name && !name.startsWith("omaspec-") ? name : null;
      };
      /// Where a part lies in its picture: the box around its groups.
      const lies = (picture, name) => around(picture, groups(picture, name));
      /// The box around `some` groups of `picture`, in its units.
      const around = (picture, some) => {
        let found = null;
        for (const group of some) {
          const b = group.getBBox();
          if (b.width <= 0 && b.height <= 0) continue;
          const { matrix } = lifted(picture, group);
          const corner = (x, y) => {
            const p = picture.createSVGPoint();
            p.x = x;
            p.y = y;
            return p.matrixTransform(matrix);
          };
          const [a, c] = [corner(b.x, b.y), corner(b.x + b.width, b.y + b.height)];
          const box = [Math.min(a.x, c.x), Math.min(a.y, c.y), Math.max(a.x, c.x), Math.max(a.y, c.y)];
          found = found
            ? [Math.min(found[0], box[0]), Math.min(found[1], box[1]), Math.max(found[2], box[2]), Math.max(found[3], box[3])]
            : box;
        }
        return found && [found[0], found[1], found[2] - found[0], found[3] - found[1]];
      };
      /// A part of a picture drawn where the glide has it: its groups, in
      /// a group that is faded. Its shapes are stretched to the place of
      /// the moment. Its words are not: each group of words alone keeps
      /// its size and is moved to where it lay in the box.
      const piece = (parent, picture, name, source) => {
        const wrap = make("g", {}, parent);
        const shapes = make("g", { opacity: 0 }, wrap);
        const worded = [];
        for (const group of groups(picture, name)) {
          const { text } = lifted(picture, group);
          const copy = group.cloneNode(true);
          const only = group.children.length > 0 && [...group.children].every((el) => el.tagName === "text");
          // This group alone.
          const at = only && around(picture, [group]);
          const into = at ? make("g", { opacity: 0 }, wrap) : shapes;
          if (at) worded.push({ into, at });
          if (text) make("g", { transform: text }, into).appendChild(copy);
          else into.appendChild(copy);
        }
        return { wrap, shapes, worded, source };
      };
      /// `piece` at `rect`, `alpha` strong, its words `words` strong (as
      /// its shapes where none is given); a unit of its picture is
      /// `scale` wide there.
      const place = (piece, rect, alpha, scale, words) => {
        if (!piece) return;
        const [sx, sy, sw, sh] = piece.source;
        const [x, y, w, h] = rect;
        const [kx, ky] = [w / Math.max(sw, 1e-3), h / Math.max(sh, 1e-3)];
        piece.wrap.setAttribute("opacity", 1);
        piece.shapes.setAttribute("opacity", alpha);
        for (const { into } of piece.worded) into.setAttribute("opacity", words === undefined ? alpha : words);
        piece.shapes.setAttribute("transform", `translate(${x},${y}) scale(${kx},${ky}) translate(${-sx},${-sy})`);
        for (const { into, at } of piece.worded) {
          // The middle of the words, where it lay in the box.
          const [cx, cy] = [at[0] + at[2] / 2, at[1] + at[3] / 2];
          const k = scale || Math.min(kx, ky);
          into.setAttribute(
            "transform",
            `translate(${x + (cx - sx) * kx},${y + (cy - sy) * ky}) scale(${k}) translate(${-cx},${-cy})`,
          );
        }
      };

      /// The sheet of the glide `index`, as large as `room`, from its
      /// picture to the next (played `forward`, or back): the
      /// two grounds (each picture without its lines and the parts that
      /// travel), a piece for each box and each line's words, the lines.
      const sheet = (index, room, forward = true) => {
        const glide = data.glides[index];
        const sides = [pictures[index], pictures[index + 1]];
        const sizes = [data.sizes[index], data.sizes[index + 1]];
        const taken = [new Set(), new Set()];
        for (const box of glide.boxes) box.parts.forEach((name, side) => taken[side].add(name));
        for (const line of glide.lines) {
          line.words.forEach((names, side) => names.forEach((name) => name && taken[side].add(name)));
        }
        // A part no box and no line takes that is in both pictures, as a
        // title, travels too: from where it lies to where it will.
        const named = (side) => new Set(
          [...sides[side].querySelectorAll("g[class]")].map(partOf).filter(Boolean),
        );
        const after = named(1);
        const kept = [...named(0)]
          .filter((name) => after.has(name))
          .map((name) => [name, name])
          .filter(([a, b]) => !taken[0].has(a) && !taken[1].has(b))
          .map((parts) => ({ parts, from: lies(sides[0], parts[0]), to: lies(sides[1], parts[1]), role: "stay", into: null }))
          .filter((box) => box.from && box.to);
        for (const box of kept) box.parts.forEach((name, side) => taken[side].add(name));
        const moving = { boxes: [...kept, ...glide.boxes], lines: glide.lines };

        // As large as a picture is shown, in the pixels of the page.
        const view = [Math.max(1, room.width), Math.max(1, room.height)];
        // In the page's pixels, and with the page should its size change.
        const top = make("svg", { id: "glide", viewBox: `0 0 ${view[0]} ${view[1]}` }, stage);
        const grounds = sides.map((picture, side) => {
          const ground = make("g", {}, top);
          // A style holds for the whole page from where it is: the faces
          // in it are not copied for each glide.
          for (const child of picture.children) {
            if (!["defs", "metadata", "style"].includes(child.tagName)) ground.appendChild(child.cloneNode(true));
          }
          for (const marked of [...ground.querySelectorAll("[id], g[class]")]) {
            if (marked.id === data.lines[index + side] || taken[side].has(partOf(marked))) marked.remove();
          }
          return ground;
        });
        const boxes = moving.boxes.map((box) => ({
          old: box.role === "arrive" ? null : piece(top, sides[0], box.parts[0], box.from),
          now: box.role === "leave" ? null : piece(top, sides[1], box.parts[1], box.to),
        }));
        // In a history what went takes on red as it fades out; played
        // back, it comes again, and does not.
        const went = moving.boxes.map((box) => (forward && box.glow === "went" && data.glows
          ? make("rect", {
            rx: 6,
            fill: data.glows.went,
            "fill-opacity": 0.14,
            stroke: data.glows.went,
            "stroke-width": 2.5,
            opacity: 0,
          }, top)
          : null));
        // A box that changes its size and what it says keeps its shapes
        // and changes its words: the old ones go before the new ones
        // come, so no word stands on another.
        const said = (piece) => (piece ? piece.wrap.textContent.replace(/\s+/g, " ").trim() : "");
        const resized = moving.boxes.map((box, i) => box.role === "stay"
          && (Math.abs(box.from[2] - box.to[2]) > 1 || Math.abs(box.from[3] - box.to[3]) > 1)
          && said(boxes[i].old) !== said(boxes[i].now));
        const lines = moving.lines.map(() => ({
          run: make("polyline", { fill: "none", "stroke-linejoin": "round" }, top),
          heads: [],
        }));
        // Lines that become one, or one that becomes several, share their
        // words: each is drawn once, with the first of them.
        const drawn = [new Set(), new Set()];
        const worded = moving.lines.map((line) => [0, 1, 2].map((slot) => {
          const [was, is] = [0, 1].map((side) => {
            const name = line.words[side][slot];
            if (!name || drawn[side].has(name)) return null;
            drawn[side].add(name);
            const at = lies(sides[side], name);
            return at && { at, piece: piece(top, sides[side], name, at) };
          });
          return { line, was, is };
        }));

        return {
          draw(t) {
            const now = Present.frame(moving, sizes, view, t);
            // The ground of the picture nearer, as a mark changes with it.
            const hollow = unders[index + (t < 0.5 ? 0 : 1)];
            grounds.forEach((ground, side) => {
              const [x, y, w, h] = now.paper;
              ground.setAttribute("opacity", now.grounds[side]);
              ground.setAttribute(
                "transform",
                `translate(${x},${y}) scale(${w / sizes[side][0]},${h / sizes[side][1]})`,
              );
            });
            now.boxes.forEach((box, i) => {
              place(boxes[i].old, box.rect, box.old, box.scale, resized[i] ? Present.gone(t) : undefined);
              place(boxes[i].now, box.rect, box.now, box.scale, resized[i] ? Present.come(t) : undefined);
              if (went[i]) {
                const [x, y, w, h] = box.rect;
                const k = t / 0.45;
                went[i].setAttribute("x", x);
                went[i].setAttribute("y", y);
                went[i].setAttribute("width", w);
                went[i].setAttribute("height", h);
                went[i].setAttribute("opacity", k < 1 ? 1 - k * k : 0);
              }
            });
            now.lines.forEach((line, i) => {
              const dash = moving.lines[i].dash;
              const set = (el, attrs) => Object.entries(attrs).forEach(([key, value]) => el.setAttribute(key, value));
              set(lines[i].run, {
                points: line.points.map((p) => p.join(",")).join(" "),
                stroke: line.color,
                "stroke-width": line.width,
                opacity: line.alpha,
                "stroke-dasharray": dash ? dash.map((d) => (d * line.width) / moving.lines[i].width).join(" ") : "none",
              });
              while (lines[i].heads.length < line.heads.length) lines[i].heads.push(make("path", {}, top));
              lines[i].heads.forEach((mark, j) => {
                const head = line.heads[j];
                if (!head) return mark.setAttribute("opacity", 0);
                set(mark, {
                  d: `M${head.points.map((p) => p.join(",")).join("L")}${head.closed ? "z" : ""}`,
                  fill: head.fill === "ink" ? line.color : head.fill === "paper" ? hollow : "none",
                  stroke: line.color,
                  "stroke-width": line.width,
                  "stroke-linejoin": "round",
                  opacity: line.alpha,
                });
              });
            });
            for (const { line, was, is } of worded.flat()) {
              const at = Present.words(line, was && was.at, is && is.at, sizes, view, t);
              if (was) place(was.piece, at.old ? at.old.rect : was.at, at.old ? at.old.alpha : 0);
              if (is) place(is.piece, at.now ? at.now.rect : is.at, at.now ? at.now.alpha : 0);
            }
          },
          done() {
            top.remove();
          },
        };
      };

      /// In a history, what came glows green a moment once the glide to
      /// picture `at` from the one before has ended, what changed amber,
      /// over the picture where each part lies in it.
      const glowIn = (at) => {
        const glide = data.glides[at - 1];
        if (!data.glows || !glide) return;
        const picture = pictures[at];
        const group = make("g", { class: "glow" }, picture);
        for (const box of glide.boxes) {
          const color = box.glow === "came" || box.glow === "changed" ? data.glows[box.glow] : null;
          const rect = color && lies(picture, box.parts[1]);
          if (!rect) continue;
          const [x, y, w, h] = rect;
          make("rect", {
            x: x - 4, y: y - 4, width: w + 8, height: h + 8, rx: 10,
            fill: "none", stroke: color, "stroke-width": 6, "stroke-opacity": 0.28,
          }, group);
          make("rect", {
            x, y, width: w, height: h, rx: 6,
            fill: color, "fill-opacity": 0.14, stroke: color, "stroke-width": 2.5,
          }, group);
        }
        // Gone once it has glowed, also where the animation does not run.
        group.addEventListener("animationend", () => group.remove());
        setTimeout(() => group.remove(), 1600);
        glowing.push(group);
      };
      let glowing = [];

      let running = null;
      // `#1.5`: the glide from picture 1 to the next, half its way, still.
      const midway = /^#(\d+)(\.\d+)$/.exec(location.hash);
      const halfway = midway && Number(midway[1]) < play.count;
      let auto = !/^#\d+$/.test(location.hash) && !halfway;
      let down = false;
      /// The sheet a glide held still on stands until the play moves.
      let stood = null;
      const unstand = () => {
        if (!stood) return;
        stood.done();
        stood = null;
        show(play.at);
      };
      const tell = () => {
        if (history) {
          say(
            span("at", `${data.captions[play.at]} · ${play.at + 1} / ${play.count + 1}`),
            span("help", auto ? "plays by itself · → or a click steps · F full screen" : "→ the next commit · ← the one before · Home plays again · F full screen"),
          );
          return;
        }
        const folded = data.names.slice(0, play.at);
        say(
          span("at", folded.length ? `${folded.join(", ")} folded` : "all open"),
          span("help", auto ? "plays by itself · → or a click steps · F full screen" : "→ fold · ← unfold · Home plays again · F full screen"),
        );
      };
      /// Stops the step that waits to be taken by itself.
      const still = () => {
        for (const timer of timers) clearTimeout(timer);
        timers = [];
      };
      const wait = () => {
        if (!auto) return;
        // One step waits at a time: Home while one waits starts no second.
        still();
        // A history ends on its last commit a while, and starts again.
        if (history && play.at === play.count) {
          timers.push(setTimeout(() => {
            play.home();
            show(0);
            tell();
            wait();
          }, HOLD * 2.5));
          return;
        }
        timers.push(setTimeout(() => {
          if (play.at === play.count) down = true;
          if (play.at === 0) down = false;
          move(down ? -1 : 1);
        }, HOLD));
      };
      /// One step on (`by` 1) or back (-1), the glide between its two
      /// pictures run to its end.
      const move = (by) => {
        if (running) return;
        unstand();
        for (const group of glowing) group.remove();
        glowing = [];
        const from = play.at;
        if (!(by > 0 ? play.next() : play.back())) return;
        tell();
        if (calm) {
          show(play.at);
          return wait();
        }
        // The sheet takes the room the picture had.
        const room = pictures[from].getBoundingClientRect();
        pictures.forEach((picture) => picture.classList.add("away"));
        const on = sheet(Math.min(from, play.at), room, by > 0);
        const began = performance.now();
        const tick = (time) => {
          const k = Math.min(1, (time - began) / TOOK);
          const t = Present.ease(k);
          on.draw(by > 0 ? t : 1 - t);
          if (k < 1) {
            running = requestAnimationFrame(tick);
            return;
          }
          running = null;
          show(play.at);
          on.done();
          if (by > 0) glowIn(play.at);
          wait();
        };
        on.draw(by > 0 ? 0 : 1);
        running = requestAnimationFrame(tick);
      };
      const hand = (by) => {
        auto = false;
        still();
        move(by);
        tell();
      };
      go = () => hand(1);
      back = () => hand(-1);
      take = go;
      home = () => {
        if (running) return;
        unstand();
        play.home();
        show(0);
        auto = true;
        down = false;
        tell();
        wait();
      };
      play.go(parseInt(location.hash.slice(1), 10) || 0);
      show(play.at);
      tell();
      if (halfway) {
        const from = Number(midway[1]);
        const room = pictures[from].getBoundingClientRect();
        pictures.forEach((picture) => picture.classList.add("away"));
        stood = sheet(from, room);
        stood.draw(Present.ease(Number(midway[2])));
      } else {
        wait();
      }
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
