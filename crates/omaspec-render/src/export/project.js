// The project as one page: its map, and a click on a tile, or on a state
// that opens a diagram of its own, zooms into what it opens, as the
// editor dives into it; Escape, ← or a name in the trail zooms back out.
// The address says where the camera is (`#Day/Order`), so a link to the
// page can open it there. With less motion asked for, the pictures
// change without the zoom.
(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("site-data").textContent);
  const stage = document.getElementById("site");
  const trail = document.getElementById("trail");
  const lit = document.getElementById("lit");
  const pictures = [...stage.querySelectorAll(".picture")];
  // How long a zoom in and one out take, in seconds.
  const IN = 0.9;
  const OUT = 0.7;
  const MARGIN = 14;
  const calm = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const { clamp, ease, fit } = Zoom;
  // Where the camera stands: the pictures from the map in, each with the
  // hop that led there.
  const path = [{ at: 0, hop: null }];
  let moving = false;
  const here = () => path[path.length - 1].at;
  const view = () => {
    const box = stage.getBoundingClientRect();
    return [MARGIN, MARGIN, box.width - 2 * MARGIN, box.height - 2 * MARGIN - 28];
  };
  const place = (index, [x, y, w, h], opacity, over) => {
    const el = pictures[index];
    el.style.visibility = opacity > 0.001 ? "visible" : "hidden";
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
    el.style.width = `${w}px`;
    el.style.height = `${h}px`;
    el.style.opacity = opacity;
    el.style.zIndex = over;
  };
  // The picture shown, fitted to the view, and the camera's way in it as
  // the trail and the address.
  const show = () => {
    pictures.forEach((_, i) => place(i, [0, 0, 0, 0], 0, 0));
    place(here(), fit(data.pictures[here()].size, view()), 1, 1);
    const parts = [];
    path.forEach((step, i) => {
      const last = i === path.length - 1;
      if (i > 0) {
        const between = document.createElement("span");
        between.textContent = "›";
        between.className = "between";
        parts.push(between);
      }
      const name = document.createElement(last ? "span" : "button");
      name.textContent = step.hop ? step.hop.name : data.pictures[step.at].name;
      if (last) {
        name.className = "here";
      } else {
        name.addEventListener("click", (event) => {
          event.stopPropagation();
          out(path.length - 1 - i);
        });
      }
      parts.push(name);
    });
    trail.replaceChildren(...parts);
    const hash = path.slice(1).map((step) => encodeURIComponent(step.hop.name)).join("/");
    history.replaceState(null, "", hash ? `#${hash}` : location.pathname + location.search);
  };
  // The picture shown fitted to the view, and how large one of its units is.
  const fitted = () => {
    const size = data.pictures[here()].size;
    const at = fit(size, view());
    return { at, unit: at[2] / Math.max(size[0], 1e-3) };
  };
  // The hop under the point (`x`, `y`) of the stage: of those it lies in,
  // the smallest, a state inside another.
  const under = (x, y) => {
    const { at, unit } = fitted();
    const [px, py] = [(x - at[0]) / unit, (y - at[1]) / unit];
    let found = null;
    for (const hop of data.hops[here()]) {
      const [hx, hy, hw, hh] = hop.rect;
      if (px >= hx && px <= hx + hw && py >= hy && py <= hy + hh && (!found || hw * hh < found.rect[2] * found.rect[3])) {
        found = hop;
      }
    }
    return found;
  };
  // A light around the hop the pointer is over.
  const light = (hop) => {
    stage.classList.toggle("over", !!hop);
    if (!hop) {
      lit.style.display = "none";
      return;
    }
    const { at, unit } = fitted();
    const [x, y, w, h] = hop.rect;
    lit.style.display = "block";
    lit.style.left = `${at[0] + x * unit - 3}px`;
    lit.style.top = `${at[1] + y * unit - 3}px`;
    lit.style.width = `${w * unit + 6}px`;
    lit.style.height = `${h * unit + 6}px`;
  };
  // The zoom of `step` in (or out), over `took` seconds, then `done`.
  const run = (step, inward, took, done) => {
    light(null);
    if (calm) {
      done();
      return;
    }
    moving = true;
    const began = performance.now();
    const frame = (time) => {
      const along = clamp((time - began) / (took * 1000));
      const k = ease(inward ? along : 1 - along);
      const sizes = [data.pictures[step.from].size, data.pictures[step.to].size];
      const at = Zoom.between(sizes[0], sizes[1], step.room, view(), k);
      pictures.forEach((_, i) => {
        if (i !== step.from && i !== step.to) place(i, [0, 0, 0, 0], 0, 0);
      });
      // The picture coming fades in as it nears the view; the one it came
      // from goes before its words grow too large to read.
      place(step.from, at.a, 1 - clamp((k - 0.55) / 0.35), 1);
      place(step.to, at.b, clamp((k - 0.15) / 0.4), 2);
      if (along < 1) {
        requestAnimationFrame(frame);
      } else {
        moving = false;
        done();
      }
    };
    requestAnimationFrame(frame);
  };
  const into = (hop) => {
    if (moving) return;
    run({ from: here(), to: hop.to, room: hop.room }, true, IN, () => {
      path.push({ at: hop.to, hop });
      show();
    });
  };
  // Back out `steps` pictures, one zoom after another.
  const out = (steps) => {
    if (moving || path.length < 2 || steps < 1) return;
    const last = path[path.length - 1];
    run({ from: path[path.length - 2].at, to: last.at, room: last.hop.room }, false, OUT, () => {
      path.pop();
      show();
      out(steps - 1);
    });
  };
  const pointer = (event) => {
    const box = stage.getBoundingClientRect();
    return under(event.clientX - box.left, event.clientY - box.top);
  };
  stage.addEventListener("mousemove", (event) => {
    if (!moving) light(pointer(event));
  });
  stage.addEventListener("mouseleave", () => light(null));
  stage.addEventListener("click", (event) => {
    if (moving) return;
    const hop = pointer(event);
    if (hop) into(hop);
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" || event.key === "Backspace" || event.key === "ArrowLeft") {
      out(1);
      event.preventDefault();
    } else if (event.key === "Home") {
      out(path.length - 1);
    } else if (event.key === "f" || event.key === "F") {
      if (document.fullscreenElement) {
        document.exitFullscreen();
      } else if (document.documentElement.requestFullscreen) {
        document.documentElement.requestFullscreen();
      }
    }
  });
  window.addEventListener("resize", () => {
    if (!moving) show();
  });
  // `#Day/Order`: straight there, by the names of the hops on the way.
  const wanted = location.hash.slice(1).split("/").filter(Boolean).map(decodeURIComponent);
  for (const name of wanted) {
    const hop = data.hops[here()].find((h) => h.name === name);
    if (!hop) break;
    path.push({ at: hop.to, hop });
  }
  show();
})();
