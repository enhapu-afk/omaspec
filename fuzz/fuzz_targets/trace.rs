//! A recorded trace (JSON Lines) of any text against a state machine: the
//! objects found, each followed, and the coverage, without a panic; every
//! step names a line of the trace.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omaspec_core::Language;
use omaspec_model::{Spec, coverage, follow_trace, trace_objects};

const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock [key]\nLocked --> Closed : unlock\nstate Open {\n  [*] --> Wide\n  Wide --> Ajar : push\n}\n@enduml\n";

fuzz_target!(|trace: &str| {
    let Some(spec) = Spec::read(DOOR, Language::PlantUml, "door.puml") else {
        return;
    };
    let lines = trace.split('\n').count();
    for object in trace_objects(trace, &spec) {
        let followed = follow_trace(trace, &spec, &object);
        for step in &followed.steps {
            assert!(step.line < lines, "{step:?}");
        }
    }
    let _ = coverage(&spec, &[trace]);
});
