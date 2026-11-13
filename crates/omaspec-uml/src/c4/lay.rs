//! Laying a C4 diagram out: the shapes in rows inside their boundaries,
//! each measured from its wrapped words, with gaps for the labels.

use super::paint::{page_ink, readable, shape_paint};
use super::route::{route_all, route_hits};
use super::{Diagram, INNER, Rel, RelKind};
use crate::Declined;
use crate::color::Color;
use crate::text::{self, Block, Family, Font};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    Figure,
    Cylinder,
    Queue,
    Box,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl Rect {
    pub(super) fn cx(self) -> f32 {
        self.x + self.w / 2.0
    }

    pub(super) fn cy(self) -> f32 {
        self.y + self.h / 2.0
    }

    pub(super) fn right(self) -> f32 {
        self.x + self.w
    }

    pub(super) fn bottom(self) -> f32 {
        self.y + self.h
    }

    pub(super) fn intersects(self, other: Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    pub(super) fn inflate(self, pad: f32) -> Rect {
        Rect {
            x: self.x - pad,
            y: self.y - pad,
            w: self.w + pad * 2.0,
            h: self.h + pad * 2.0,
        }
    }
}

pub(super) struct Span {
    pub(super) left: f32,
    pub(super) right: f32,
    pub(super) top: f32,
    pub(super) bottom: f32,
}

impl Span {
    pub(super) fn at(x: f32, y: f32) -> Span {
        Span {
            left: x,
            right: x,
            top: y,
            bottom: y,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Gaps {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) lane: f32,
    pub(super) top: f32,
    pub(super) bottom: f32,
    pub(super) side: f32,
    /// Tallest relationship chip, used to keep the overhead band clear.
    pub(super) chip_h: f32,
    /// Distance between two overhead lanes.
    pub(super) pitch: f32,
}

/// Which relationships use the overhead band, and where that band starts.
pub(super) struct Lane<'a> {
    pub(super) sky: &'a [usize],
    pub(super) y0: f32,
}

pub(super) struct PlacedShape {
    pub(super) rect: Rect,
    pub(super) lines: Vec<Block>,
}

pub(super) struct PlacedBoundary {
    pub(super) rect: Rect,
    pub(super) lines: Vec<Block>,
}

pub(super) struct PlacedRel {
    pub(super) points: Vec<(f32, f32)>,
    pub(super) lines: Vec<Block>,
    pub(super) chip: Rect,
    pub(super) color: Color,
    pub(super) dashed: bool,
    pub(super) bi: bool,
}

pub(super) struct Laid {
    pub(super) title: Option<Block>,
    pub(super) shapes: Vec<PlacedShape>,
    pub(super) boundaries: Vec<PlacedBoundary>,
    pub(super) rels: Vec<PlacedRel>,
}

pub(super) struct Measured {
    pub(super) w: f32,
    pub(super) h: f32,
    pub(super) lines: Vec<Block>,
}

pub(super) fn lay(diagram: &Diagram, family: Family) -> Result<Laid, Declined> {
    let gaps = gaps(diagram, family)?;
    let title = match &diagram.title {
        Some(title) => Some(measured(
            &title.value,
            Font::new(diagram.look.font_size + 2.0, page_ink(&diagram.look)).bold(),
            family,
            title.line,
        )?),
        None => None,
    };
    let title_h = title
        .as_ref()
        .map(|block| block.height + 14.0)
        .unwrap_or(0.0);
    let mut placed = place(diagram, family, &gaps, title.clone(), title_h, 0.0)?;
    let mut rels = route_all(
        diagram,
        family,
        &shape_rects(&placed),
        &gaps,
        &header_bands(&placed.boundaries),
        &Lane {
            sky: &[],
            y0: title_h,
        },
    )?;
    let mut sky = Vec::new();
    for (index, rel) in rels.iter().enumerate() {
        let ends = (diagram.rels[index].from, diagram.rels[index].to);
        if route_hits(
            &rel.points,
            ends,
            &shape_rects(&placed),
            &header_bands(&placed.boundaries),
        ) {
            sky.push(index);
        }
    }
    if sky.is_empty() {
        placed.rels = rels;
        return Ok(placed);
    }
    // Crossing lines move into a band under the title. The boxes are placed
    // again below that band; the straight links keep their gaps.
    let band = sky.len() as f32 * gaps.pitch + 6.0;
    placed = place(diagram, family, &gaps, title, title_h, band)?;
    rels = route_all(
        diagram,
        family,
        &shape_rects(&placed),
        &gaps,
        &header_bands(&placed.boundaries),
        &Lane {
            sky: &sky,
            y0: title_h,
        },
    )?;
    placed.rels = rels;
    Ok(placed)
}

pub(super) fn place(
    diagram: &Diagram,
    family: Family,
    gaps: &Gaps,
    title: Option<Block>,
    title_h: f32,
    band: f32,
) -> Result<Laid, Declined> {
    let mut shapes = Vec::new();
    shapes.resize_with(diagram.shapes.len(), || None);
    let mut boundaries = Vec::new();
    boundaries.resize_with(diagram.boundaries.len(), || None);
    {
        let mut geom = Geom {
            diagram,
            family,
            gaps: *gaps,
            shapes: &mut shapes,
            boundaries: &mut boundaries,
        };
        geom.place_level(None, 0.0, title_h + band)?;
    }
    Ok(Laid {
        title,
        shapes: shapes.into_iter().flatten().collect(),
        boundaries: boundaries.into_iter().flatten().collect(),
        rels: Vec::new(),
    })
}

pub(super) fn shape_rects(laid: &Laid) -> Vec<Rect> {
    laid.shapes.iter().map(|shape| shape.rect).collect()
}

pub(super) fn header_bands(boundaries: &[PlacedBoundary]) -> Vec<Rect> {
    boundaries
        .iter()
        .map(|placed| {
            let width = placed
                .lines
                .iter()
                .fold(0.0_f32, |wide, line| wide.max(line.width))
                + 8.0;
            Rect {
                x: placed.rect.cx() - width / 2.0,
                y: placed.rect.y + 2.0,
                w: width,
                h: stack_height(&placed.lines) + 6.0,
            }
        })
        .collect()
}

pub(super) struct Geom<'a> {
    pub(super) diagram: &'a Diagram,
    pub(super) family: Family,
    pub(super) gaps: Gaps,
    pub(super) shapes: &'a mut [Option<PlacedShape>],
    pub(super) boundaries: &'a mut [Option<PlacedBoundary>],
}

impl Geom<'_> {
    pub(super) fn place_level(
        &mut self,
        parent: Option<usize>,
        ox: f32,
        oy: f32,
    ) -> Result<Span, Declined> {
        let shape_ids = self.child_shapes(parent);
        let bound_ids = self.child_bounds(parent);
        let top = oy + self.gaps.top;
        let left = ox + INNER;
        let shape_span = self.place_shapes(&shape_ids, left, top)?;
        let bound_x = if shape_ids.is_empty() {
            left
        } else {
            shape_span.right + self.gaps.x
        };
        let bound_span = self.place_bounds(&bound_ids, bound_x, top)?;
        if !shape_ids.is_empty() && !bound_ids.is_empty() {
            let shape_h = shape_span.bottom - shape_span.top;
            let bound_h = bound_span.bottom - bound_span.top;
            if shape_h + 1.0 < bound_h {
                self.shift_shapes(&shape_ids, (bound_h - shape_h) / 2.0);
            } else if bound_h + 1.0 < shape_h {
                for id in &bound_ids {
                    self.shift_tree(*id, (shape_h - bound_h) / 2.0);
                }
            }
        }
        Ok(self.enclose(parent, left, top))
    }

    pub(super) fn place_shapes(
        &mut self,
        ids: &[usize],
        x0: f32,
        y0: f32,
    ) -> Result<Span, Declined> {
        if ids.is_empty() {
            return Ok(Span::at(x0, y0));
        }
        let per = self.diagram.shapes_per_row.max(1);
        let mut y = y0;
        let mut right = x0;
        let mut start = 0;
        while start < ids.len() {
            let end = (start + per).min(ids.len());
            let mut x = x0;
            let mut row_h = 0.0_f32;
            let mut row = Vec::new();
            for &id in &ids[start..end] {
                let measured = self.measure_shape(id)?;
                row_h = row_h.max(measured.h);
                row.push((id, measured));
            }
            for (index, (id, measured)) in row.into_iter().enumerate() {
                self.shapes[id] = Some(PlacedShape {
                    rect: Rect {
                        x,
                        y,
                        w: measured.w,
                        h: measured.h,
                    },
                    lines: measured.lines,
                });
                right = right.max(x + measured.w);
                x += measured.w;
                if index + 1 != end - start {
                    x += self.gaps.x;
                }
            }
            y += row_h;
            start = end;
            if start < ids.len() {
                y += self.gaps.y;
            }
        }
        Ok(Span {
            left: x0,
            right,
            top: y0,
            bottom: y,
        })
    }

    pub(super) fn place_bounds(
        &mut self,
        ids: &[usize],
        x0: f32,
        y0: f32,
    ) -> Result<Span, Declined> {
        if ids.is_empty() {
            return Ok(Span::at(x0, y0));
        }
        let per = self.diagram.boundaries_per_row.max(1);
        let mut y = y0;
        let mut right = x0;
        let mut start = 0;
        while start < ids.len() {
            let end = (start + per).min(ids.len());
            let mut x = x0;
            let mut row_h = 0.0_f32;
            for (index, &id) in ids[start..end].iter().enumerate() {
                let (w, h) = self.place_boundary(id, x, y)?;
                right = right.max(x + w);
                row_h = row_h.max(h);
                x += w;
                if index + 1 != end - start {
                    x += self.gaps.x;
                }
            }
            y += row_h;
            start = end;
            if start < ids.len() {
                y += self.gaps.y;
            }
        }
        Ok(Span {
            left: x0,
            right,
            top: y0,
            bottom: y,
        })
    }

    pub(super) fn place_boundary(
        &mut self,
        id: usize,
        x: f32,
        y: f32,
    ) -> Result<(f32, f32), Declined> {
        let lines = self.header_lines(id)?;
        let header_w = lines
            .iter()
            .fold(0.0_f32, |wide, line| wide.max(line.width));
        let header_h = stack_height(&lines) + 10.0;
        let inner = self.place_level(Some(id), x, y + header_h)?;
        let right = inner.right.max(x + INNER + header_w).max(x + INNER) + self.gaps.side;
        let bottom = inner.bottom + self.gaps.bottom;
        let w = (right - x).max(header_w + INNER * 2.0);
        let h = (bottom - y).max(header_h + INNER);
        self.boundaries[id] = Some(PlacedBoundary {
            rect: Rect { x, y, w, h },
            lines,
        });
        Ok((w, h))
    }

    pub(super) fn measure_shape(&self, id: usize) -> Result<Measured, Declined> {
        let shape = &self.diagram.shapes[id];
        let look = &self.diagram.look;
        let style = shape_paint(shape, look);
        let ink = if style.mark == Mark::Figure && shape.ink.is_none() {
            page_ink(look)
        } else {
            style.ink
        };
        let mut lines = Vec::new();
        if style.mark != Mark::Figure || shape.kind.external() {
            lines.push(measured(
                shape.kind.stereotype(),
                Font::new(look.font_size * 0.75, ink).italic(),
                self.family,
                shape.line,
            )?);
        }
        lines.push(measured(
            &wrap(&shape.label, 28),
            Font::new(look.font_size, ink).bold(),
            self.family,
            shape.line,
        )?);
        if let Some(type_label) = &shape.type_label {
            lines.push(measured(
                &wrap(type_label, 32),
                Font::new(look.font_size * 0.78, ink).italic(),
                self.family,
                shape.line,
            )?);
        }
        if let Some(techn) = &shape.techn {
            lines.push(measured(
                &format!("[{}]", wrap(techn, 28)),
                Font::new(look.font_size * 0.78, ink).italic(),
                self.family,
                shape.line,
            )?);
        }
        if let Some(descr) = &shape.descr {
            lines.push(measured(
                &wrap(descr, 32),
                Font::new(look.font_size * 0.75, ink),
                self.family,
                shape.line,
            )?);
        }
        let text_w = lines
            .iter()
            .fold(0.0_f32, |wide, line| wide.max(line.width));
        let text_h = stack_height(&lines);
        let (w, h) = match style.mark {
            Mark::Figure => ((text_w + 20.0).max(96.0), 58.0 + text_h + 12.0),
            Mark::Cylinder => ((text_w + 28.0).max(132.0), (text_h + 36.0).max(88.0)),
            Mark::Queue => ((text_w + 44.0).max(148.0), (text_h + 28.0).max(76.0)),
            Mark::Box => ((text_w + 28.0).max(128.0), (text_h + 28.0).max(72.0)),
        };
        Ok(Measured { w, h, lines })
    }

    pub(super) fn header_lines(&self, id: usize) -> Result<Vec<Block>, Declined> {
        let boundary = &self.diagram.boundaries[id];
        let look = &self.diagram.look;
        let ink = boundary.ink.unwrap_or_else(|| {
            boundary
                .fill
                .map(|fill| readable(fill, look.preferred))
                .unwrap_or_else(|| page_ink(look))
        });
        let mut lines = vec![
            measured(
                &wrap(&boundary.label, 32),
                Font::new(look.font_size, ink).bold(),
                self.family,
                boundary.line,
            )?,
            measured(
                &format!("[{}]", boundary.kind_name),
                Font::new(look.font_size * 0.78, ink).italic(),
                self.family,
                boundary.line,
            )?,
        ];
        if let Some(descr) = &boundary.descr {
            lines.push(measured(
                &wrap(descr, 36),
                Font::new(look.font_size * 0.75, ink),
                self.family,
                boundary.line,
            )?);
        }
        Ok(lines)
    }

    pub(super) fn child_shapes(&self, parent: Option<usize>) -> Vec<usize> {
        self.diagram
            .shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| shape.parent == parent)
            .map(|(index, _)| index)
            .collect()
    }

    pub(super) fn child_bounds(&self, parent: Option<usize>) -> Vec<usize> {
        self.diagram
            .boundaries
            .iter()
            .enumerate()
            .filter(|(_, boundary)| boundary.parent == parent)
            .map(|(index, _)| index)
            .collect()
    }

    pub(super) fn shift_shapes(&mut self, ids: &[usize], dy: f32) {
        for id in ids {
            if let Some(shape) = &mut self.shapes[*id] {
                shape.rect.y += dy;
            }
        }
    }

    pub(super) fn shift_tree(&mut self, id: usize, dy: f32) {
        if let Some(boundary) = &mut self.boundaries[id] {
            boundary.rect.y += dy;
        }
        let shapes = self.child_shapes(Some(id));
        self.shift_shapes(&shapes, dy);
        let children = self.child_bounds(Some(id));
        for child in children {
            self.shift_tree(child, dy);
        }
    }

    pub(super) fn enclose(&self, parent: Option<usize>, left: f32, top: f32) -> Span {
        let mut span = Span::at(left, top);
        for (index, shape) in self.diagram.shapes.iter().enumerate() {
            if shape.parent == parent
                && let Some(placed) = &self.shapes[index]
            {
                span = union(span, placed.rect);
            }
        }
        for (index, boundary) in self.diagram.boundaries.iter().enumerate() {
            if boundary.parent == parent
                && let Some(placed) = &self.boundaries[index]
            {
                span = union(span, placed.rect);
            }
        }
        span
    }
}

pub(super) fn union(span: Span, rect: Rect) -> Span {
    Span {
        left: span.left.min(rect.x),
        right: span.right.max(rect.right()),
        top: span.top.min(rect.y),
        bottom: span.bottom.max(rect.bottom()),
    }
}

pub(super) fn gaps(diagram: &Diagram, family: Family) -> Result<Gaps, Declined> {
    let mut chip_w = 0.0_f32;
    let mut chip_h = 0.0_f32;
    for rel in &diagram.rels {
        let (width, height) = chip_size(diagram, rel, family)?;
        chip_w = chip_w.max(width);
        chip_h = chip_h.max(height);
    }
    let mut leave = vec![0usize; diagram.shapes.len()];
    let mut self_count = 0usize;
    let mut elbow = false;
    for rel in &diagram.rels {
        if rel.from == rel.to {
            self_count += 1;
        } else {
            leave[rel.from] += 1;
            leave[rel.to] += 1;
        }
        if !matches!(rel.kind, RelKind::Rel | RelKind::Bi | RelKind::Back) {
            elbow = true;
        }
    }
    let max_side = leave.into_iter().max().unwrap_or(1);
    let spread = max_side.saturating_sub(1) as f32 * 12.0;
    let x = (chip_w + 40.0 + spread).max(72.0);
    let y = (chip_h + 28.0).max(40.0);
    let lane = (y - chip_h / 2.0 - 8.0).clamp(14.0, 40.0);
    let self_pad = if self_count == 0 {
        0.0
    } else {
        20.0 + self_count as f32 * 14.0 + chip_w * 0.5
    };
    Ok(Gaps {
        x,
        y,
        lane,
        top: if elbow || self_count > 0 { y } else { 6.0 },
        bottom: if elbow { y * 0.45 } else { 8.0 },
        side: if self_count > 0 {
            x.max(self_pad)
        } else if elbow {
            lane + 20.0
        } else {
            10.0
        },
        chip_h,
        pitch: chip_h + 10.0,
    })
}

pub(super) fn chip_size(
    diagram: &Diagram,
    rel: &Rel,
    family: Family,
) -> Result<(f32, f32), Declined> {
    let lines = chip_lines(diagram, rel, family, page_ink(&diagram.look))?;
    let width = lines
        .iter()
        .fold(0.0_f32, |wide, line| wide.max(line.width));
    Ok((width + 12.0, stack_height(&lines) + 8.0))
}

pub(super) fn chip_lines(
    diagram: &Diagram,
    rel: &Rel,
    family: Family,
    ink: Color,
) -> Result<Vec<Block>, Declined> {
    let size = diagram.look.font_size;
    let mut lines = vec![measured(
        &wrap(&rel.label, 28),
        Font::new(size * 0.86, ink).bold(),
        family,
        rel.line,
    )?];
    if let Some(techn) = &rel.techn {
        lines.push(measured(
            &format!("[{}]", wrap(techn, 28)),
            Font::new(size * 0.75, ink).italic(),
            family,
            rel.line,
        )?);
    }
    if let Some(descr) = &rel.descr {
        lines.push(measured(
            &wrap(descr, 32),
            Font::new(size * 0.72, ink),
            family,
            rel.line,
        )?);
    }
    Ok(lines)
}

pub(super) fn measured(
    text: &str,
    font: Font,
    family: Family,
    line: usize,
) -> Result<Block, Declined> {
    text::block(text, font, family).map_err(|reason| Declined::at(line, reason))
}

pub(super) fn stack_height(lines: &[Block]) -> f32 {
    let gaps = lines.len().saturating_sub(1) as f32 * 2.0;
    lines.iter().map(|line| line.height).sum::<f32>() + gaps
}

pub(super) fn wrap(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        wrap_line(&mut out, line, limit);
    }
    out
}

pub(super) fn wrap_line(out: &mut String, line: &str, limit: usize) {
    if line.chars().count() <= limit {
        out.push_str(line);
        return;
    }
    let mut width = 0usize;
    let mut word = String::new();
    let flush = |out: &mut String, word: &mut String, width: &mut usize| {
        if word.is_empty() {
            return;
        }
        if *width > 0 {
            out.push(' ');
        }
        out.push_str(word);
        *width += word.chars().count();
        word.clear();
    };
    for ch in line.chars() {
        if ch.is_whitespace() {
            if width > 0 && width + 1 + word.chars().count() > limit {
                out.push('\n');
                width = 0;
            }
            flush(out, &mut word, &mut width);
        } else {
            word.push(ch);
        }
    }
    if width > 0 && width + 1 + word.chars().count() > limit {
        out.push('\n');
        width = 0;
    }
    flush(out, &mut word, &mut width);
}
