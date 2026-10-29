//! Line diffs. [`line_diff`] is for Compare: which lines changed between two
//! versions of a diagram, where blank lines and indentation do not count, so
//! re-formatting shows no changes. [`changes`] is exact, for marking the
//! lines edited since the last save.

use std::ops::Range;

use serde::Serialize;

/// Diffs needing more edits than this are refused (they are not readable anyway).
pub const MAX_EDITS: usize = 600;

/// A non-blank line of one side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// One-based line number in its text.
    pub number: usize,
    /// The line without trailing spaces.
    pub text: String,
    #[serde(skip)]
    key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    Same { old: Line, new: Line },
    Removed { old: Line },
    Added { new: Line },
}

impl Op {
    pub fn is_change(&self) -> bool {
        !matches!(self, Op::Same { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineDiff {
    pub ops: Vec<Op>,
    pub removed: usize,
    pub added: usize,
}

/// The diff from `old` to `new`, or `None` if it needs more than
/// [`MAX_EDITS`] edits.
pub fn line_diff(old: &str, new: &str) -> Option<LineDiff> {
    let a = lines(old);
    let b = lines(new);
    let mut start = 0;
    while start < a.len() && start < b.len() && a[start].key == b[start].key {
        start += 1;
    }
    let (mut end_a, mut end_b) = (a.len(), b.len());
    while end_a > start && end_b > start && a[end_a - 1].key == b[end_b - 1].key {
        end_a -= 1;
        end_b -= 1;
    }
    let middle = script(&keys(&a[start..end_a]), &keys(&b[start..end_b]))?;

    let mut ops = Vec::with_capacity(a.len().max(b.len()));
    for k in 0..start {
        ops.push(Op::Same {
            old: a[k].clone(),
            new: b[k].clone(),
        });
    }
    ops.extend(middle.into_iter().map(|step| match step {
        Step::Same(x, y) => Op::Same {
            old: a[start + x].clone(),
            new: b[start + y].clone(),
        },
        Step::Removed(x) => Op::Removed {
            old: a[start + x].clone(),
        },
        Step::Added(y) => Op::Added {
            new: b[start + y].clone(),
        },
    }));
    for k in 0..a.len() - end_a {
        ops.push(Op::Same {
            old: a[end_a + k].clone(),
            new: b[end_b + k].clone(),
        });
    }
    let removed = ops
        .iter()
        .filter(|o| matches!(o, Op::Removed { .. }))
        .count();
    let added = ops.iter().filter(|o| matches!(o, Op::Added { .. })).count();
    Some(LineDiff {
        ops,
        removed,
        added,
    })
}

/// A run of edited lines, for marks beside the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Zero-based lines of the new text that are new or changed.
    Lines(Range<usize>),
    /// Lines of the old text are gone before this zero-based line of the
    /// new text.
    Removed(usize),
}

/// Where `new` differs from `old`, line by line and exactly. A change too
/// large to diff marks everything between the common start and end.
pub fn changes(old: &str, new: &str) -> Vec<Change> {
    let a: Vec<&str> = old.split('\n').collect();
    let b: Vec<&str> = new.split('\n').collect();
    let mut start = 0;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        start += 1;
    }
    let (mut end_a, mut end_b) = (a.len(), b.len());
    while end_a > start && end_b > start && a[end_a - 1] == b[end_b - 1] {
        end_a -= 1;
        end_b -= 1;
    }
    if start == end_a && start == end_b {
        return Vec::new();
    }
    let Some(steps) = script(&a[start..end_a], &b[start..end_b]) else {
        return vec![if end_b > start {
            Change::Lines(start..end_b)
        } else {
            Change::Removed(start)
        }];
    };
    // Each run of steps between two unchanged lines is one change.
    let mut out = Vec::new();
    let (mut from, mut added, mut removed) = (start, 0, false);
    let flush = |at: usize, added: usize, removed: bool, out: &mut Vec<Change>| {
        if added > 0 {
            out.push(Change::Lines(at..at + added));
        } else if removed {
            out.push(Change::Removed(at));
        }
    };
    for step in steps {
        match step {
            Step::Same(_, y) => {
                flush(from, added, removed, &mut out);
                (from, added, removed) = (start + y + 1, 0, false);
            }
            Step::Removed(_) => removed = true,
            Step::Added(_) => added += 1,
        }
    }
    flush(from, added, removed, &mut out);
    out
}

/// The changes with `context` unchanged lines around them, merged where
/// they touch.
pub fn hunks(ops: &[Op], context: usize) -> Vec<&[Op]> {
    let mut keep = vec![false; ops.len()];
    for (k, op) in ops.iter().enumerate() {
        if op.is_change() {
            let to = (k + context).min(ops.len() - 1);
            for flag in &mut keep[k.saturating_sub(context)..=to] {
                *flag = true;
            }
        }
    }
    let mut hunks = Vec::new();
    let mut k = 0;
    while k < ops.len() {
        if !keep[k] {
            k += 1;
            continue;
        }
        let mut end = k;
        while end < ops.len() && keep[end] {
            end += 1;
        }
        hunks.push(&ops[k..end]);
        k = end;
    }
    hunks
}

fn keys(lines: &[Line]) -> Vec<&str> {
    lines.iter().map(|l| l.key.as_str()).collect()
}

fn lines(text: &str) -> Vec<Line> {
    text.split('\n')
        .enumerate()
        .filter_map(|(i, line)| {
            let key = line.trim();
            (!key.is_empty()).then(|| Line {
                number: i + 1,
                text: line.trim_end().to_owned(),
                key: key.to_owned(),
            })
        })
        .collect()
}

/// One step of an edit script from `a` to `b` ([`script`]), with indices
/// into the two sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Same(usize, usize),
    Removed(usize),
    Added(usize),
}

/// Myers' O(ND) diff, giving up after [`MAX_EDITS`] edits.
pub fn script<T: PartialEq>(a: &[T], b: &[T]) -> Option<Vec<Step>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m).min(MAX_EDITS as isize);
    let at = |k: isize| (k + max + 1) as usize;
    let mut v = vec![0isize; (2 * max + 3) as usize];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    for d in 0..=max {
        trace.push(v.clone());
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x < n || y < m {
                continue;
            }
            // Walk back through the saved rounds, last edit first.
            let mut ops = Vec::new();
            for e in (0..=d).rev() {
                let prev = &trace[e as usize];
                let kk = x - y;
                let down = kk == -e || (kk != e && prev[at(kk - 1)] < prev[at(kk + 1)]);
                let (px, py) = if e == 0 {
                    (0, 0)
                } else {
                    let pk = if down { kk + 1 } else { kk - 1 };
                    (prev[at(pk)], prev[at(pk)] - pk)
                };
                while x > px && y > py {
                    x -= 1;
                    y -= 1;
                    ops.push(Step::Same(x as usize, y as usize));
                }
                if e == 0 {
                    break;
                }
                if down {
                    ops.push(Step::Added(py as usize));
                } else {
                    ops.push(Step::Removed(px as usize));
                }
                x = px;
                y = py;
            }
            ops.reverse();
            return Some(ops);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(d: &LineDiff) -> Vec<String> {
        d.ops
            .iter()
            .map(|o| match o {
                Op::Same { new, .. } => format!("  {}", new.text.trim()),
                Op::Removed { old } => format!("- {}", old.text.trim()),
                Op::Added { new } => format!("+ {}", new.text.trim()),
            })
            .collect()
    }

    #[test]
    fn finds_changes() {
        let d = line_diff("a\nb\nc\nd", "a\nc\nx\nd").unwrap();
        assert_eq!(summary(&d), ["  a", "- b", "  c", "+ x", "  d"]);
        assert_eq!((d.removed, d.added), (1, 1));
    }

    #[test]
    fn ignores_blank_lines_and_indentation() {
        let d = line_diff("alt x\nA -> B\nend", "alt x\n\n    A -> B   \nend\n").unwrap();
        assert_eq!((d.removed, d.added), (0, 0));
    }

    #[test]
    fn numbers_lines_in_their_own_text() {
        let d = line_diff("a", "\n\na\nb").unwrap();
        let Op::Added { new } = &d.ops[1] else {
            panic!()
        };
        assert_eq!(new.number, 4);
    }

    #[test]
    fn refuses_huge_diffs() {
        let old: String = (0..700).map(|i| format!("a{i}\n")).collect();
        let new: String = (0..700).map(|i| format!("b{i}\n")).collect();
        assert!(line_diff(&old, &new).is_none());
        assert!(line_diff("", "").unwrap().ops.is_empty());
    }

    #[test]
    fn marks_changed_and_removed_lines_exactly() {
        let old = "a\nb\nc\nd\ne";
        assert_eq!(changes(old, old), []);
        // Indentation counts here.
        assert_eq!(changes(old, "a\n  b\nc\nd\ne"), [Change::Lines(1..2)]);
        assert_eq!(
            changes(old, "a\nX\nY\nc\ne"),
            [Change::Lines(1..3), Change::Removed(4)]
        );
        assert_eq!(changes(old, "a\nb\nc\nd\ne\nf"), [Change::Lines(5..6)]);
        assert_eq!(changes(old, "b\nc\nd\ne"), [Change::Removed(0)]);
        assert_eq!(changes(old, "a\nb\nc\nd"), [Change::Removed(4)]);
    }

    #[test]
    fn marks_everything_between_when_the_change_is_huge() {
        let old: String = (0..700).map(|i| format!("a{i}\n")).collect();
        let new: String = (0..700).map(|i| format!("b{i}\n")).collect();
        assert_eq!(changes(&old, &new), [Change::Lines(0..700)]);
    }

    #[test]
    fn hunks_merge_close_changes() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10";
        let new = "1\nX\n3\n4\n5\n6\n7\n8\nY\n10";
        let d = line_diff(old, new).unwrap();
        assert_eq!(hunks(&d.ops, 1).len(), 2);
        assert_eq!(hunks(&d.ops, 3).len(), 1);
    }
}
