// A flight through a project: the camera zooms from the map into a tile,
// the diagram growing out of the tile's room until it fills the view, on
// along a link into the next, and back out the same way. A zoom keeps
// one point of the view where it is, so the room in one picture becomes
// the whole of the next. Everything is a function of the time alone. A
// click starts it again; with less motion asked for, the map stands, and
// `#t=4` holds it still 4 seconds in.
(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("play").textContent);
  const stage = document.getElementById("flight");
  const trail = document.getElementById("trail");
  const pictures = [...stage.querySelectorAll(".picture")];
  // How long a zoom in and one out take, and how long each stays.
  const IN = 1.6;
  const OUT = 1.3;
  const STAY = 1.0;
  const MARGIN = 14;
  const { clamp, ease, fit } = Zoom;
  const view = () => {
    const box = stage.getBoundingClientRect();
    return [MARGIN, MARGIN, box.width - 2 * MARGIN, box.height - 2 * MARGIN - 20];
  };
  // `over`: the picture coming lies over the one it grows out of, whichever
  // came first on the page (a flight back to a diagram drawn before).
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
  // The steps of all the flights in time: each step in, a stay, and back
  // out the other way; the map a while between flights.
  const moves = [];
  let clock = STAY;
  for (const steps of data.flights) {
    for (const step of steps) {
      moves.push({ step, from: clock, took: IN, inward: true });
      clock += IN + STAY;
    }
    for (const step of [...steps].reverse()) {
      moves.push({ step, from: clock, took: OUT, inward: false });
      clock += OUT + STAY * 0.6;
    }
    clock += STAY * 0.4;
  }
  const length = clock;
  // Where the two pictures of `step` lie when the zoom has come `k` of
  // its way in.
  const zoom = (step, k) =>
    Zoom.between(data.pictures[step.from].size, data.pictures[step.to].size, step.room, view(), k);
  const draw = (t) => {
    const time = ((t % length) + length) % length;
    pictures.forEach((_, i) => place(i, [0, 0, 0, 0], 0, 0));
    // The move under way, else the picture the last one left on screen.
    let shown = null;
    for (const move of moves) {
      if (move.from <= time) shown = move;
    }
    if (!shown) {
      place(0, fit(data.pictures[0].size, view()), 1, 1);
      trail.textContent = data.pictures[0].name;
      return;
    }
    const { step, inward } = shown;
    const along = clamp((time - shown.from) / shown.took);
    const k = ease(inward ? along : 1 - along);
    const at = zoom(step, k);
    // The picture coming fades in as it nears the view; the one it came
    // from goes before its words grow too large to read.
    place(step.from, at.a, 1 - clamp((k - 0.55) / 0.35), 1);
    place(step.to, at.b, clamp((k - 0.15) / 0.4), 2);
    const names = [data.pictures[step.from].name, data.pictures[step.to].name];
    trail.textContent = k > 0.5 ? names.join(" › ") : names[0];
  };
  let begun = null;
  const tick = (time) => {
    if (begun === null) begun = time;
    draw((time - begun) / 1000);
    requestAnimationFrame(tick);
  };
  // `#t=4`: the flight as it stands 4 seconds in, still.
  const still = /(?:^#|&)t=([\d.]+)/.exec(location.hash);
  if (still) {
    draw(Number(still[1]));
    window.addEventListener("resize", () => draw(Number(still[1])));
  } else if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    draw(0);
  } else {
    requestAnimationFrame(tick);
    stage.addEventListener("click", () => {
      begun = null;
    });
  }
})();
