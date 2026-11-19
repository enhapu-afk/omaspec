// Code becomes a picture: the caret walks down the source and the diagram
// builds itself up, each line setting down what it draws. The timing is
// the one `omaspec play --build` draws its frames with: a box grows in, a
// line draws itself to its head, its words come after it. A click starts
// it again; with less motion asked for, the whole stands at once, and
// `#t=4` after the page's address holds it still 4 seconds in.
(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("play").textContent);
  const svg = document.querySelector("#diagram > svg");
  const source = document.getElementById("source");
  const rows = [...source.querySelectorAll(".line")];
  const NS = "http://www.w3.org/2000/svg";
  const SMALL = 0.82;
  const clamp = (t) => Math.min(1, Math.max(0, t));
  const ease = (t) => {
    const s = clamp(t);
    return s * s * (3 - 2 * s);
  };
  const pace = data.pace || 1;

  // When the caret stands on a line, how far its boxes have grown, its
  // lines drawn and its words come.
  const lineAt = (t) => {
    let line = -1;
    data.starts.forEach((start, i) => {
      if (start <= t) line = i;
    });
    return line;
  };
  const grown = (line, t) => clamp(((t - data.starts[line]) * pace) / data.grow);
  const drawing = (line) =>
    data.starts[line] + (data.boxes[line] ? (data.grow * data.overlap) / pace : 0);
  const drawn = (line, t) => clamp(((t - drawing(line)) * pace) / data.draw);
  const worded = (line, t) =>
    clamp(((t - drawing(line) - data.draw / pace) * pace) / data.words);

  // Each part's groups, one a layer.
  const parts = data.parts.map((part) => {
    const groups = [...svg.querySelectorAll(`[id^="${part.prefix}"]`)].filter((g) =>
      /^\d+$/.test(g.id.slice(part.prefix.length)),
    );
    return { ...part, groups };
  });
  // A point of the picture's own units where `group` draws.
  const at = (group, [x, y]) => {
    const own = group.getCTM();
    const root = svg.getCTM();
    if (!own || !root) return [x, y];
    const point = svg.createSVGPoint();
    point.x = x;
    point.y = y;
    const inRoot = point.matrixTransform(root);
    const local = inRoot.matrixTransform(own.inverse());
    return [local.x, local.y];
  };

  // The lines drawn anew, in the picture's own units, where its lines'
  // group stands.
  const linesGroup = svg.querySelector('[id="lines"]');
  let drawnLines = null;
  if (linesGroup && data.linesMatrix) {
    linesGroup.replaceChildren();
    drawnLines = document.createElementNS(NS, "g");
    drawnLines.setAttribute("transform", `matrix(${data.linesMatrix.join(" ")})`);
    linesGroup.appendChild(drawnLines);
  }
  const wires = data.wires.map((wire) => {
    if (!drawnLines) return null;
    const path = document.createElementNS(NS, "polyline");
    path.setAttribute("points", wire.points.map((p) => p.join(",")).join(" "));
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", wire.color);
    path.setAttribute("stroke-width", wire.width);
    path.setAttribute("stroke-linejoin", "round");
    drawnLines.appendChild(path);
    const length = wire.points.reduce(
      (sum, p, i) => (i === 0 ? 0 : sum + Math.hypot(p[0] - wire.points[i - 1][0], p[1] - wire.points[i - 1][1])),
      0,
    );
    const heads = wire.heads.map((head) => {
      const mark = document.createElementNS(NS, head.closed ? "polygon" : "polyline");
      mark.setAttribute("points", head.points.map((p) => p.join(",")).join(" "));
      mark.setAttribute("fill", head.fill);
      mark.setAttribute("stroke", wire.color);
      mark.setAttribute("stroke-width", Math.min(wire.width, 1.5));
      mark.setAttribute("stroke-linejoin", "round");
      drawnLines.appendChild(mark);
      return mark;
    });
    return { ...wire, path, length, heads };
  });

  // Without parts, the picture is uncovered from the top down.
  let edge = null;
  if (parts.length === 0) {
    const [w, h] = data.size;
    const defs = document.createElementNS(NS, "defs");
    defs.innerHTML =
      `<linearGradient id="build-fade" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="1">` +
      `<stop offset="0" stop-color="white"/><stop offset="1" stop-color="white" stop-opacity="0"/></linearGradient>` +
      `<mask id="build-reveal" maskUnits="userSpaceOnUse" x="0" y="0" width="${w}" height="${h}">` +
      `<rect width="${w}" height="0" fill="url(#build-fade)"/></mask>`;
    const holder = document.createElementNS(NS, "g");
    holder.setAttribute("mask", "url(#build-reveal)");
    while (svg.firstChild) holder.appendChild(svg.firstChild);
    svg.appendChild(defs);
    svg.appendChild(holder);
    edge = { gradient: defs.querySelector("linearGradient"), rect: defs.querySelector("rect") };
  }
  const reached = (line) =>
    line < 0
      ? data.firstReach
      : data.reach.reduce((most, [at, y]) => (at <= line ? Math.max(most, y + 6) : most), data.firstReach);

  const draw = (t) => {
    const line = lineAt(t);
    rows.forEach((row, i) => {
      row.classList.toggle("ahead", line < 0 || i > line);
      row.classList.toggle("current", i === line);
    });
    if (line >= 0 && rows[line]) {
      const top = rows[line].offsetTop - source.clientHeight / 3;
      source.scrollTop = Math.max(0, top);
    }
    if (edge) {
      const to = reached(line);
      const from = reached(line - 1);
      const y = t >= data.end ? data.size[1] + 40 : from + (to - from) * (line >= 0 ? ease(grown(line, t)) : 1);
      edge.gradient.setAttribute("y1", y - 24);
      edge.gradient.setAttribute("y2", y);
      edge.rect.setAttribute("height", Math.max(0, y));
      return;
    }
    parts.forEach((part) => {
      let opacity;
      let scale = 1;
      if (line < part.line) {
        opacity = 0;
      } else if (part.words) {
        opacity = worded(part.line, t);
      } else {
        const g = ease(grown(part.line, t));
        opacity = g;
        scale = SMALL + (1 - SMALL) * g;
      }
      part.groups.forEach((group) => {
        group.setAttribute("opacity", opacity.toFixed(3));
        if (scale < 1) {
          const [x, y] = at(group.parentNode, part.middle);
          group.setAttribute("transform", `translate(${x} ${y}) scale(${scale}) translate(${-x} ${-y})`);
        } else {
          group.removeAttribute("transform");
        }
      });
    });
    wires.forEach((wire) => {
      if (!wire) return;
      const share = line < wire.line ? 0 : ease(drawn(wire.line, t));
      wire.path.setAttribute("stroke-dasharray", `${wire.length} ${wire.length}`);
      wire.path.setAttribute("stroke-dashoffset", wire.length * (1 - share));
      wire.path.setAttribute("opacity", share > 0 ? 1 : 0);
      wire.heads.forEach((head) => head.setAttribute("opacity", share >= 1 ? 1 : 0));
    });
  };

  const length = data.end + data.hold;
  let begun = null;
  const tick = (time) => {
    if (begun === null) begun = time;
    const t = (time - begun) / 1000;
    draw(Math.min(t, data.end + 0.01));
    if (t < length) {
      requestAnimationFrame(tick);
    } else {
      begun = null;
      requestAnimationFrame(tick);
    }
  };
  // `#t=4`: the build as it stands 4 seconds in, still.
  const still = /(?:^#|&)t=([\d.]+)/.exec(location.hash);
  if (still) {
    draw(Number(still[1]));
  } else if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    draw(data.end + 1);
  } else {
    requestAnimationFrame(tick);
    document.getElementById("build").addEventListener("click", () => {
      begun = null;
    });
  }
})();
