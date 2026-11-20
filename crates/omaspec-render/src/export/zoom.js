// The camera of a zoom from one picture into another, for the pages that
// fly through a project (flight.js, project.js): a picture fitted to a
// room, and the zoom that keeps one point of the view where it is, so
// the room in one picture becomes the whole of the next.
const Zoom = (() => {
  "use strict";
  const clamp = (t) => Math.min(1, Math.max(0, t));
  const ease = (t) => {
    const s = clamp(t);
    return s < 0.5 ? 4 * s * s * s : 1 - Math.pow(-2 * s + 2, 3) / 2;
  };
  // A picture of `size` as large as fits `room`, in its middle.
  const fit = ([w, h], [x, y, rw, rh]) => {
    const k = Math.min(rw / Math.max(w, 1e-3), rh / Math.max(h, 1e-3));
    return [x + (rw - w * k) / 2, y + (rh - h * k) / 2, w * k, h * k];
  };
  // Where a picture of size `a` and one of size `b`, which grows out of
  // `room` in the first (in its units, from its top left), lie on a stage
  // `view` large when the zoom has come `k` of its way in.
  const between = (a, b, room, view, k) => {
    const aFit = fit(a, view);
    const unit = aFit[2] / Math.max(a[0], 1e-3);
    const inA = [aFit[0] + room[0] * unit, aFit[1] + room[1] * unit, room[2] * unit, room[3] * unit];
    const bIn = fit(b, inA);
    const bFit = fit(b, view);
    const grow = bFit[2] / Math.max(bIn[2], 1e-3);
    // The point the zoom keeps where it is.
    const fixed = (at, to) => (Math.abs(grow - 1) < 1e-6 ? at : (to - at * grow) / (1 - grow));
    const px = fixed(bIn[0], bFit[0]);
    const py = fixed(bIn[1], bFit[1]);
    const c = Math.pow(grow, k);
    const moved = ([x, y, w, h]) => [px + (x - px) * c, py + (y - py) * c, w * c, h * c];
    return { a: moved(aFit), b: moved(bIn) };
  };
  return { clamp, ease, fit, between };
})();
