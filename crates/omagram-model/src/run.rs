//! A state machine played step by step, the way a specification writer
//! tries it out: it starts, offers what can happen next, and tells what
//! each step did, so a viewer can show it.
//!
//! The machine is in its innermost states, one per region that runs, and
//! waits at a choice or a join until it can go on. A transition leaves
//! every state up to the one that holds both of its ends and enters its
//! target; a composite state starts each of its regions, and with no start
//! to begin at it is entered as a whole. What needs nobody is taken by
//! itself: the way out of a start, of a fork, of a choice with one way, the
//! one transition without event out of a state once it is entered or done.
//! Where a choice or a state has several such ways, the writer picks.
//! Guards are not evaluated: a guarded transition is offered, and the
//! writer decides.
//!
//! A history brings its region back to where it was when the state around
//! it was last left: `[H]` to the state it was in, which starts afresh,
//! `[H*]` to everything inside. Before that, or once the region had ended,
//! the region begins at its start.

use crate::{Kind, Machine};

/// Steps taken by themselves in a row stop after this many, so a loop of
/// transitions without events cannot hang the viewer.
const MAX_CHAIN: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The states it is in, by index, in order.
    active: Vec<usize>,
    /// The transitions that reached a join during this stay. Left behind
    /// when the join is left.
    arrived: Vec<usize>,
    /// Joins every way in has reached: they go on by themselves.
    ready: Vec<usize>,
    /// Where each composite state was when it was last left, for its
    /// history.
    memory: Vec<Memory>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Memory {
    state: usize,
    /// The innermost states it was in.
    inside: Vec<usize>,
    /// The ways that had reached a join among them.
    arrived: Vec<usize>,
}

/// One transition taken, and what changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    pub transition: usize,
    /// The states left.
    pub left: Vec<usize>,
    /// The states entered.
    pub entered: Vec<usize>,
    /// Steps with one `beat` travel at once: the arms of a fork or a join,
    /// or the starts of one state's regions. A later beat waits for them.
    pub beat: u32,
}

/// Numbers the beats of one [`Run::start`] or [`Run::take`].
struct Clock {
    next: u32,
}

impl Clock {
    fn new() -> Self {
        Self { next: 0 }
    }

    fn fresh(&mut self) -> u32 {
        let beat = self.next;
        self.next += 1;
        beat
    }
}

impl Run {
    /// The machine started: the ways out of its top start taken, and what
    /// followed by itself. `None` without a start.
    pub fn start(m: &Machine) -> Option<(Run, Vec<Taken>)> {
        let starts: Vec<usize> = (0..m.states.len())
            .filter(|&s| m.states[s].kind == Kind::Start && m.states[s].parent.is_none())
            .collect();
        if starts.is_empty() {
            return None;
        }
        let mut run = Run::empty();
        let mut steps = Vec::new();
        let mut clock = Clock::new();
        let beat = clock.fresh();
        for start in starts {
            run.leave_start(m, start, &mut steps, &mut clock, beat);
        }
        run.settle(m, &mut steps, &mut clock);
        Some((run, steps))
    }

    /// The machine in `state`, as if a transition from outside had just
    /// entered it: the states around it too, their other regions started,
    /// and what follows by itself taken. `None` unless `state` is a state,
    /// not a start, an end or another mark.
    pub(crate) fn at(m: &Machine, state: usize) -> Option<Run> {
        if m.states.get(state)?.kind != Kind::State {
            return None;
        }
        let mut run = Run::empty();
        let mut after = run.around(m, state, None);
        let mut entered = Vec::new();
        if m.is_composite(state) {
            after.extend(run.regions(m, state).map(|r| (state, r)));
        } else {
            run.add(m, state, &mut entered);
        }
        run.stand_in(m, &after, &mut entered);
        let mut steps = Vec::new();
        let mut clock = Clock::new();
        run.start_all(m, after, &mut steps, &mut clock);
        run.settle(m, &mut steps, &mut clock);
        Some(run)
    }

    fn empty() -> Run {
        Run {
            active: Vec::new(),
            arrived: Vec::new(),
            ready: Vec::new(),
            memory: Vec::new(),
        }
    }

    /// The states it is in, innermost: states, and a choice, join or end it
    /// waits at.
    pub fn active(&self) -> &[usize] {
        &self.active
    }

    /// Whether it is in `state`, also as a state around one it is in.
    pub fn is_in(&self, m: &Machine, state: usize) -> bool {
        self.active
            .iter()
            .any(|&a| a == state || m.ancestors(a).any(|p| p == state))
    }

    /// Whether the machine has ended: its top end reached.
    pub fn ended(&self, m: &Machine) -> bool {
        self.active
            .iter()
            .any(|&a| m.states[a].kind == Kind::End && m.states[a].parent.is_none())
    }

    /// What can be taken now, in the order the transitions are written: a
    /// transition from a state it is in (or one around it) with an event
    /// or a guard, and the ways out of a choice it waits at. An inner
    /// state's transition hides an outer one for the same event.
    pub fn options(&self, m: &Machine) -> Vec<usize> {
        let current = |t: usize| {
            let from = m.transitions[t].from;
            match m.states[from].kind {
                // Without an event, a composite state goes once it is done.
                Kind::State
                    if m.transitions[t].trigger.events.is_empty() && m.is_composite(from) =>
                {
                    self.done(m, from)
                }
                Kind::State => self.is_in(m, from),
                Kind::Choice => self.active.contains(&from),
                _ => false,
            }
        };
        let mut candidates: Vec<usize> = (0..m.transitions.len())
            .filter(|&t| current(t) && !self.automatic(m, t))
            .collect();
        candidates.sort_by_key(|&t| m.transitions[t].line);
        candidates
            .iter()
            .copied()
            .filter(|&t| {
                let tr = &m.transitions[t];
                !candidates.iter().any(|&u| {
                    let inner = &m.transitions[u];
                    u != t
                        && m.ancestors(inner.from).any(|a| a == tr.from)
                        && inner.trigger.guard.is_none()
                        && inner
                            .trigger
                            .events
                            .iter()
                            .any(|e| tr.trigger.events.contains(e))
                })
            })
            .collect()
    }

    /// Takes `transition`, then what follows by itself; what each step did.
    pub fn take(&mut self, m: &Machine, transition: usize) -> Vec<Taken> {
        let mut steps = Vec::new();
        let mut clock = Clock::new();
        let beat = clock.fresh();
        self.step(m, transition, &mut steps, &mut clock, beat);
        self.settle(m, &mut steps, &mut clock);
        steps
    }

    /// Whether `t` is taken without anyone choosing it.
    fn automatic(&self, m: &Machine, t: usize) -> bool {
        let tr = &m.transitions[t];
        if !tr.trigger.events.is_empty() || tr.trigger.guard.is_some() {
            return false;
        }
        match m.states[tr.from].kind {
            // The one way out of a choice; with several, the writer picks.
            Kind::Choice => m.transitions.iter().filter(|u| u.from == tr.from).count() == 1,
            // The one way out of a state without an event; with several,
            // the writer picks too.
            Kind::State => {
                m.transitions
                    .iter()
                    .filter(|u| u.from == tr.from && u.trigger.events.is_empty())
                    .count()
                    == 1
            }
            _ => true,
        }
    }

    /// Takes what follows by itself until nothing does. A fork or a join
    /// goes all its ways in one beat.
    fn settle(&mut self, m: &Machine, steps: &mut Vec<Taken>, clock: &mut Clock) {
        for _ in 0..MAX_CHAIN {
            let Some(next) = self.next_automatic(m) else {
                return;
            };
            let from = m.transitions[next].from;
            let beat = clock.fresh();
            match m.states[from].kind {
                Kind::Fork | Kind::Join => {
                    self.ready.retain(|&j| j != from);
                    for out in (0..m.transitions.len()).filter(|&u| m.transitions[u].from == from) {
                        self.step(m, out, steps, clock, beat);
                    }
                }
                _ => self.step(m, next, steps, clock, beat),
            }
        }
    }

    fn next_automatic(&self, m: &Machine) -> Option<usize> {
        (0..m.transitions.len()).find(|&t| {
            if !self.automatic(m, t) {
                return false;
            }
            let from = m.transitions[t].from;
            match m.states[from].kind {
                Kind::State if m.is_composite(from) => self.done(m, from),
                Kind::State | Kind::Choice | Kind::Fork => self.active.contains(&from),
                Kind::Join => self.ready.contains(&from),
                _ => false,
            }
        })
    }

    /// Whether every region of the composite `state` reached its end, or
    /// it was entered as a whole.
    fn done(&self, m: &Machine, state: usize) -> bool {
        if self.active.contains(&state) {
            return true;
        }
        let children: Vec<usize> = (0..m.states.len())
            .filter(|&c| m.states[c].parent == Some(state))
            .collect();
        let regions = children.iter().map(|&c| m.states[c].region).max();
        regions.is_some_and(|last| {
            (0..=last).all(|r| {
                children.iter().any(|&c| {
                    m.states[c].region == r
                        && m.states[c].kind == Kind::End
                        && self.active.contains(&c)
                })
            })
        })
    }

    /// Takes one transition, on `beat`. Regions it starts share a later
    /// beat per state, so a fork's arms stay on `beat` even when one arm
    /// enters a composite and starts it in between.
    fn step(
        &mut self,
        m: &Machine,
        t: usize,
        steps: &mut Vec<Taken>,
        clock: &mut Clock,
        beat: u32,
    ) {
        let tr = &m.transitions[t];
        let (from, to) = (tr.from, tr.to);
        // The innermost state that holds both ends, strictly.
        let holder = m
            .ancestors(from)
            .find(|&a| a != to && (m.ancestors(to).any(|b| b == a)));
        // The outermost state left: the one below the holder on the way
        // to the source.
        let outer = std::iter::once(from)
            .chain(m.ancestors(from))
            .find(|&s| m.states[s].parent == holder)
            .unwrap_or(from);
        let left: Vec<usize> = self
            .active
            .iter()
            .copied()
            .filter(|&a| a == outer || m.ancestors(a).any(|p| p == outer))
            .collect();
        self.remember(m, outer, &left);
        self.active.retain(|a| !left.contains(a));
        if m.states[from].kind != Kind::Start && !left.contains(&from) {
            self.active.retain(|&a| a != from);
            self.forget_join(m, from);
        }
        for &state in &left {
            self.forget_join(m, state);
        }
        let mut entered = Vec::new();
        let mut after = self.around(m, to, holder);
        self.enter(m, to, t, &mut entered, &mut after);
        self.stand_in(m, &after, &mut entered);
        steps.push(Taken {
            transition: t,
            left,
            entered,
            beat,
        });
        self.start_all(m, after, steps, clock);
    }

    /// The other regions of the states around `to` that are entered with
    /// it, below `holder`: they start at their starts.
    fn around(&self, m: &Machine, to: usize, holder: Option<usize>) -> Vec<(usize, usize)> {
        let mut path: Vec<usize> = m.ancestors(to).take_while(|&a| Some(a) != holder).collect();
        path.reverse();
        let mut after = Vec::new();
        for (i, &state) in path.iter().enumerate() {
            let toward = path.get(i + 1).copied().unwrap_or(to);
            let region = m.states[toward].region;
            for other in self.regions(m, state).filter(|&r| r != region) {
                after.push((state, other));
            }
        }
        after
    }

    /// Starts the regions in `after`; the regions of one state share a
    /// beat.
    fn start_all(
        &mut self,
        m: &Machine,
        after: Vec<(usize, usize)>,
        steps: &mut Vec<Taken>,
        clock: &mut Clock,
    ) {
        let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
        for (state, region) in after {
            if let Some((_, regions)) = groups.iter_mut().find(|(s, _)| *s == state) {
                regions.push(region);
            } else {
                groups.push((state, vec![region]));
            }
        }
        for (state, regions) in groups {
            let region_beat = clock.fresh();
            for region in regions {
                self.start_region(m, state, region, steps, clock, region_beat);
            }
        }
    }

    /// Enters `state` by transition `t`. Regions to start once this step is
    /// told go to `after`.
    fn enter(
        &mut self,
        m: &Machine,
        state: usize,
        t: usize,
        entered: &mut Vec<usize>,
        after: &mut Vec<(usize, usize)>,
    ) {
        match m.states[state].kind {
            Kind::State if m.is_composite(state) => {
                for region in self.regions(m, state) {
                    after.push((state, region));
                }
            }
            Kind::Join => {
                self.arrived.push(t);
                self.add(m, state, entered);
                let incoming: Vec<usize> = (0..m.transitions.len())
                    .filter(|&u| m.transitions[u].to == state)
                    .collect();
                // Every way in arrived: the join goes on by itself.
                if incoming.iter().all(|u| self.arrived.contains(u)) {
                    self.arrived.retain(|u| !incoming.contains(u));
                    self.ready.push(state);
                }
            }
            Kind::History | Kind::DeepHistory => {
                let Some(parent) = m.states[state].parent else {
                    return;
                };
                let region = m.states[state].region;
                let deep = m.states[state].kind == Kind::DeepHistory;
                let (back, arrived) = self.recall(m, parent, region, deep);
                if back.is_empty() {
                    after.push((parent, region));
                }
                for s in back {
                    if !deep && m.is_composite(s) {
                        self.enter(m, s, t, entered, after);
                    } else {
                        self.add(m, s, entered);
                    }
                }
                self.arrived.extend(arrived);
            }
            _ => self.add(m, state, entered),
        }
    }

    /// Notes where the machine was inside each composite state this step
    /// leaves: `outer` and every state between it and `left`, the
    /// innermost states left.
    fn remember(&mut self, m: &Machine, outer: usize, left: &[usize]) {
        let mut composites: Vec<usize> = Vec::new();
        for &s in left {
            for a in std::iter::once(s).chain(m.ancestors(s)) {
                if m.is_composite(a) && !composites.contains(&a) {
                    composites.push(a);
                }
                if a == outer {
                    break;
                }
            }
        }
        for state in composites {
            let inside: Vec<usize> = left
                .iter()
                .copied()
                .filter(|&s| m.ancestors(s).any(|a| a == state))
                .collect();
            let arrived = self
                .arrived
                .iter()
                .copied()
                .filter(|&u| inside.contains(&m.transitions[u].to))
                .collect();
            self.memory.retain(|mem| mem.state != state);
            if !inside.is_empty() {
                self.memory.push(Memory {
                    state,
                    inside,
                    arrived,
                });
            }
        }
    }

    /// What the history of `region` of `state` brings back, and the ways
    /// that had reached a join among it: the innermost states it was in
    /// (`deep`), or the states of the region they sat in. Nothing before it
    /// was left, or once the region had ended.
    fn recall(
        &self,
        m: &Machine,
        state: usize,
        region: usize,
        deep: bool,
    ) -> (Vec<usize>, Vec<usize>) {
        let Some(memory) = self.memory.iter().find(|mem| mem.state == state) else {
            return (Vec::new(), Vec::new());
        };
        // The state of this region each remembered state sits in.
        let child = |s: usize| {
            std::iter::once(s)
                .chain(m.ancestors(s))
                .find(|&a| m.states[a].parent == Some(state))
                .filter(|&c| m.states[c].region == region)
        };
        let mut back = Vec::new();
        for &s in &memory.inside {
            let Some(c) = child(s) else {
                continue;
            };
            let s = if deep { s } else { c };
            if !back.contains(&s) {
                back.push(s);
            }
        }
        let ended = back
            .iter()
            .all(|&s| m.states[s].kind == Kind::End && m.states[s].parent == Some(state));
        if ended {
            return (Vec::new(), Vec::new());
        }
        let arrived = memory
            .arrived
            .iter()
            .copied()
            .filter(|&u| back.contains(&m.transitions[u].to))
            .collect();
        (back, arrived)
    }

    /// A composite state whose regions to start have no start between them,
    /// with nothing inside it entered, is entered as a whole: it stays
    /// lit, and its own transitions are offered.
    fn stand_in(&mut self, m: &Machine, after: &[(usize, usize)], entered: &mut Vec<usize>) {
        for &(state, _) in after {
            let begins = after
                .iter()
                .any(|&(s, region)| s == state && start_of(m, s, region).is_some());
            let inside = self
                .active
                .iter()
                .any(|&a| a == state || m.ancestors(a).any(|p| p == state));
            if !begins && !inside {
                self.add(m, state, entered);
            }
        }
    }

    fn add(&mut self, m: &Machine, state: usize, entered: &mut Vec<usize>) {
        // A composite state that stood in for its inside gives way.
        self.active.retain(|&a| !m.ancestors(state).any(|p| p == a));
        if !self.active.contains(&state) {
            self.active.push(state);
            self.active.sort_unstable();
        }
        entered.push(state);
    }

    /// The regions of `state`.
    fn regions(&self, m: &Machine, state: usize) -> impl Iterator<Item = usize> + use<> {
        let last = (0..m.states.len())
            .filter(|&c| m.states[c].parent == Some(state))
            .map(|c| m.states[c].region)
            .max();
        0..last.map_or(0, |r| r + 1)
    }

    /// A join that was left forgets which ways arrived, so the next stay
    /// waits for them again.
    fn forget_join(&mut self, m: &Machine, state: usize) {
        if m.states[state].kind != Kind::Join {
            return;
        }
        self.arrived.retain(|&u| m.transitions[u].to != state);
        self.ready.retain(|&j| j != state);
    }

    /// Starts one region of `state` at its start.
    fn start_region(
        &mut self,
        m: &Machine,
        state: usize,
        region: usize,
        steps: &mut Vec<Taken>,
        clock: &mut Clock,
        beat: u32,
    ) {
        if let Some(start) = start_of(m, state, region) {
            self.leave_start(m, start, steps, clock, beat);
        }
    }

    fn leave_start(
        &mut self,
        m: &Machine,
        start: usize,
        steps: &mut Vec<Taken>,
        clock: &mut Clock,
        beat: u32,
    ) {
        for t in (0..m.transitions.len()).filter(|&t| m.transitions[t].from == start) {
            self.step(m, t, steps, clock, beat);
        }
    }
}

/// The start of `region` of `state`.
fn start_of(m: &Machine, state: usize, region: usize) -> Option<usize> {
    (0..m.states.len()).find(|&c| {
        m.states[c].parent == Some(state)
            && m.states[c].region == region
            && m.states[c].kind == Kind::Start
    })
}

#[cfg(test)]
mod tests {
    use omagram_core::Language;

    use super::*;

    fn machine(body: &str) -> Machine {
        Machine::read(&format!("@startuml\n{body}\n@enduml\n"), Language::PlantUml).unwrap()
    }

    fn names(m: &Machine, states: &[usize]) -> Vec<String> {
        states
            .iter()
            .map(|&s| match m.states[s].kind {
                Kind::End => "[*]".to_owned(),
                _ => m.states[s].name.clone(),
            })
            .collect()
    }

    fn labels(m: &Machine, options: &[usize]) -> Vec<String> {
        options
            .iter()
            .map(|&t| {
                let tr = &m.transitions[t];
                let mut label = tr.trigger.events.join(", ");
                if let Some(g) = &tr.trigger.guard {
                    label.push_str(&format!(" [{g}]"));
                }
                label.trim().to_owned()
            })
            .collect()
    }

    fn take(run: &mut Run, m: &Machine, label: &str) -> Vec<Taken> {
        let options = run.options(m);
        let at = labels(m, &options)
            .iter()
            .position(|l| l == label)
            .unwrap_or_else(|| panic!("{label} not in {:?}", labels(m, &options)));
        run.take(m, options[at])
    }

    #[test]
    fn starts_offers_and_moves() {
        let m = machine(
            "[*] --> Closed\nClosed --> Open : open [unlocked]\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock",
        );
        let (mut run, steps) = Run::start(&m).unwrap();
        assert_eq!(names(&m, run.active()), ["Closed"]);
        assert_eq!(steps.len(), 1);
        assert_eq!(labels(&m, &run.options(&m)), ["open [unlocked]", "lock"]);
        let steps = take(&mut run, &m, "lock");
        assert_eq!(names(&m, &steps[0].left), ["Closed"]);
        assert_eq!(names(&m, &steps[0].entered), ["Locked"]);
        assert_eq!(labels(&m, &run.options(&m)), ["unlock"]);
    }

    #[test]
    fn composite_states_start_and_complete() {
        let m = machine(
            "[*] --> Session\nstate Session {\n  [*] --> Login\n  Login --> Work : ok\n  Work --> [*] : quit\n}\nSession --> Gone\nSession --> Gone : kill\nGone --> [*]",
        );
        let (mut run, steps) = Run::start(&m).unwrap();
        // Into the session, and on to its start's target.
        assert_eq!(steps.len(), 2);
        assert_eq!(names(&m, run.active()), ["Login"]);
        // The outer `kill` can be taken from inside.
        assert_eq!(labels(&m, &run.options(&m)), ["ok", "kill"]);
        take(&mut run, &m, "ok");
        let steps = take(&mut run, &m, "quit");
        // The session's end completes it, which leads on by itself, to the
        // end of the machine.
        assert_eq!(steps.len(), 3, "{steps:?}");
        assert!(run.ended(&m));
    }

    #[test]
    fn regions_fork_and_join() {
        let m = machine(
            "state f <<fork>>\nstate j <<join>>\n[*] --> f\nf --> A\nf --> B\nA --> j : a\nB --> j : b\nj --> Done\nDone --> [*]",
        );
        let (mut run, _) = Run::start(&m).unwrap();
        assert_eq!(names(&m, run.active()), ["A", "B"]);
        take(&mut run, &m, "a");
        // The join waits for the other way.
        assert!(names(&m, run.active()).contains(&"j".to_owned()));
        assert_eq!(labels(&m, &run.options(&m)), ["b"]);
        take(&mut run, &m, "b");
        assert!(run.ended(&m), "{:?}", names(&m, run.active()));
    }

    #[test]
    fn concurrent_regions_move_on_their_own() {
        let m = machine(
            "[*] --> Both\nstate Both {\n  [*] --> A1\n  A1 --> A2 : a\n  --\n  [*] --> B1\n  B1 --> B2 : b\n}",
        );
        let (mut run, _) = Run::start(&m).unwrap();
        assert_eq!(names(&m, run.active()), ["A1", "B1"]);
        take(&mut run, &m, "b");
        assert_eq!(names(&m, run.active()), ["A1", "B2"]);
    }

    #[test]
    fn a_choice_waits_for_the_writer() {
        let m = machine(
            "state c <<choice>>\n[*] --> Idle\nIdle --> c : check\nc --> Ok : [fine]\nc --> Fail : [else]",
        );
        let (mut run, _) = Run::start(&m).unwrap();
        take(&mut run, &m, "check");
        assert_eq!(names(&m, run.active()), ["c"]);
        assert_eq!(labels(&m, &run.options(&m)), ["[fine]", "[else]"]);
        take(&mut run, &m, "[else]");
        assert_eq!(names(&m, run.active()), ["Fail"]);
    }

    #[test]
    fn the_machine_finds_itself_on_the_map() {
        let code = "@startuml\n[*] --> Idle\nIdle --> Busy : go\nstate Busy {\n  [*] --> Work\n}\nBusy --> [*]\n@enduml\n";
        let m = Machine::read(code, Language::PlantUml).unwrap();
        let (_, map) = omagram_uml::render_mapped(code, None).unwrap();
        let placed = m.place(&map);
        assert!(placed.states.iter().all(Option::is_some), "{placed:?}");
        assert!(placed.routes.iter().all(Option::is_some), "{placed:?}");
        let work = m.states.iter().position(|s| s.name == "Work").unwrap();
        let busy = m.states.iter().position(|s| s.name == "Busy").unwrap();
        let (w, b) = (
            placed.states[work].as_ref().unwrap().rect,
            placed.states[busy].as_ref().unwrap().rect,
        );
        // `Work` sits inside `Busy`.
        assert!(w.x > b.x && w.y > b.y && w.x + w.w < b.x + b.w && w.y + w.h < b.y + b.h);
    }

    #[test]
    fn plantuml_s_own_example_plays() {
        // The composite state example of PlantUML's documentation, scaled.
        let code = "@startuml\nscale 600 width\n\n[*] -> State1\nState1 --> State2 : Succeeded\nState1 --> [*] : Aborted\nState2 --> State3 : Succeeded\nState2 --> [*] : Aborted\nstate State3 {\n  state \"Accumulate Enough Data\\nLong State Name\" as long1\n  long1 : Just a test\n  [*] --> long1\n  long1 --> long1 : New Data\n  long1 --> ProcessData : Enough Data\n}\nState3 --> State3 : Failed\nState3 --> [*] : Succeeded / Save Result\nState3 --> [*] : Aborted\n\n@enduml\n";
        let m = Machine::read(code, Language::PlantUml).unwrap();
        let (_, map) = omagram_uml::render_mapped(code, None).unwrap();
        let placed = m.place(&map);
        assert!(placed.states.iter().all(Option::is_some), "{placed:?}");
        assert!(placed.routes.iter().all(Option::is_some), "{placed:?}");
        let (mut run, _) = Run::start(&m).unwrap();
        assert_eq!(names(&m, run.active()), ["State1"]);
        take(&mut run, &m, "Succeeded");
        take(&mut run, &m, "Succeeded");
        // Into State3, on to its start's target.
        assert_eq!(names(&m, run.active()), ["long1"]);
        assert_eq!(
            labels(&m, &run.options(&m)),
            ["New Data", "Enough Data", "Failed", "Succeeded", "Aborted"]
        );
        take(&mut run, &m, "Enough Data");
        assert_eq!(names(&m, run.active()), ["ProcessData"]);
        take(&mut run, &m, "Aborted");
        assert!(run.ended(&m));
    }

    #[test]
    fn a_join_forgets_a_stay_it_left() {
        let m = machine(
            "[*] --> P\nstate P {\n  state f <<fork>>\n  state j <<join>>\n  [*] --> f\n  f --> A\n  f --> B\n  A --> j : a\n  B --> j : b\n  j --> Done\n}\nP --> P : again",
        );
        let (mut run, _) = Run::start(&m).unwrap();
        assert_eq!(names(&m, run.active()), ["A", "B"]);
        take(&mut run, &m, "a");
        take(&mut run, &m, "again");
        assert_eq!(names(&m, run.active()), ["A", "B"]);
        take(&mut run, &m, "b");
        let at = names(&m, run.active());
        assert!(
            at.contains(&"j".to_owned()) && at.contains(&"A".to_owned()),
            "{at:?}"
        );
        assert!(!at.contains(&"Done".to_owned()), "{at:?}");
    }

    #[test]
    fn a_fork_into_a_composite_is_one_beat() {
        let m = machine(
            "state f <<fork>>\nstate Comp {\n  [*] --> Inner\n}\n[*] --> f\nf --> Comp\nf --> Other",
        );
        let (_, steps) = Run::start(&m).unwrap();
        let beat = |from: Kind, to: &str| {
            steps
                .iter()
                .find(|s| {
                    let tr = &m.transitions[s.transition];
                    m.states[tr.from].kind == from && m.states[tr.to].name == to
                })
                .unwrap_or_else(|| panic!("no {from:?} --> {to} in {steps:?}"))
                .beat
        };
        let arms = beat(Kind::Fork, "Comp");
        assert_eq!(arms, beat(Kind::Fork, "Other"));
        assert!(beat(Kind::Start, "Inner") > arms);
    }

    #[test]
    fn transitions_without_events_cannot_hang() {
        let m = machine("[*] --> A\nA --> B\nB --> A");
        let (run, steps) = Run::start(&m).unwrap();
        assert!(steps.len() <= MAX_CHAIN + 1);
        assert_eq!(run.active().len(), 1);
    }

    /// Where each option leads, by name.
    fn targets(m: &Machine, options: &[usize]) -> Vec<String> {
        let to: Vec<usize> = options.iter().map(|&t| m.transitions[t].to).collect();
        names(m, &to)
    }

    #[test]
    fn several_ways_without_an_event_are_offered() {
        let m = machine("[*] --> A\nA --> [*]\nA -> B\nB --> [*]");
        let (mut run, _) = Run::start(&m).unwrap();
        assert_eq!(names(&m, run.active()), ["A"]);
        let options = run.options(&m);
        assert_eq!(targets(&m, &options), ["[*]", "B"]);
        // B has one way out: it goes on by itself.
        run.take(&m, options[1]);
        assert!(run.ended(&m));
    }

    #[test]
    fn ways_out_of_a_composite_without_an_event_wait_until_it_is_done() {
        let m = machine(
            "[*] --> C\nstate C {\n  [*] --> Work\n  Work --> [*] : finish\n}\nC --> D\nC --> E",
        );
        let (mut run, _) = Run::start(&m).unwrap();
        assert_eq!(labels(&m, &run.options(&m)), ["finish"]);
        take(&mut run, &m, "finish");
        assert_eq!(targets(&m, &run.options(&m)), ["D", "E"]);
    }

    #[test]
    fn a_composite_without_a_start_is_entered_as_a_whole() {
        let m = machine(
            "[*] --> Idle\nIdle --> Preview : show\nstate Preview {\n  A -> B\n}\nPreview --> Idle : back",
        );
        let (mut run, _) = Run::start(&m).unwrap();
        let steps = take(&mut run, &m, "show");
        assert_eq!(names(&m, run.active()), ["Preview"]);
        assert_eq!(names(&m, &steps[0].entered), ["Preview"]);
        assert_eq!(labels(&m, &run.options(&m)), ["back"]);
        take(&mut run, &m, "back");
        assert_eq!(names(&m, run.active()), ["Idle"]);
    }

    const HISTORY: &str = "[*] --> Off\nOff --> On : power\nstate On {\n  [*] --> Low\n  Low --> High : up\n  High --> [*] : done\n  state High {\n    [*] --> Warm\n    Warm --> Hot : heat\n  }\n}\nOn --> Off : pause\nOff --> On[H] : resume\nOff --> On[H*] : deep";

    #[test]
    fn a_history_begins_at_the_start_the_first_time() {
        let m = machine(HISTORY);
        let (mut run, _) = Run::start(&m).unwrap();
        take(&mut run, &m, "resume");
        assert_eq!(names(&m, run.active()), ["Low"]);
    }

    #[test]
    fn a_history_brings_back_the_state_it_left() {
        let m = machine(HISTORY);
        let (mut run, _) = Run::start(&m).unwrap();
        for label in ["power", "up", "heat", "pause"] {
            take(&mut run, &m, label);
        }
        let paused_hot = run.clone();
        // Shallow: back into High, which starts afresh.
        take(&mut run, &m, "resume");
        assert_eq!(names(&m, run.active()), ["Warm"]);
        take(&mut run, &m, "pause");
        // Going back to an earlier run brings back its memory too.
        run = paused_hot;
        // Deep: back to Hot itself.
        let steps = take(&mut run, &m, "deep");
        assert_eq!(names(&m, run.active()), ["Hot"]);
        assert_eq!(names(&m, &steps[0].entered), ["Hot"]);
    }

    #[test]
    fn a_history_of_a_region_that_ended_begins_again() {
        let m = machine(HISTORY);
        let (mut run, _) = Run::start(&m).unwrap();
        for label in ["power", "up", "done", "pause"] {
            take(&mut run, &m, label);
        }
        take(&mut run, &m, "resume");
        assert_eq!(names(&m, run.active()), ["Low"]);
    }
}
