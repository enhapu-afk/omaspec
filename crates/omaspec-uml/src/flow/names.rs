//! The elements of a flow diagram's picture by name, from the source
//! (docs/adr/0037): a port of the diagram by its side and its name
//! (`input-x`, `output-y`), a block by its name (`block-Filter`, one
//! inside another as well), a wire by what it joins and says, a block's
//! port as `Block.port` (`link-x-Filter.in`). A name given twice takes
//! `-2`.

use std::collections::HashMap;

use super::{Block, Diagram, End, Wire};
use crate::svg::{Given, element, link_name};

#[derive(Debug, Default)]
pub(super) struct Names {
    inputs: Vec<String>,
    outputs: Vec<String>,
    blocks: HashMap<Vec<usize>, String>,
    /// By the path of the block that holds them, the diagram's at `[]`.
    wires: HashMap<(Vec<usize>, usize), String>,
}

impl Names {
    pub(super) fn of(diagram: &Diagram) -> Names {
        let mut given = Given::default();
        let mut names = Names {
            inputs: diagram
                .inputs
                .iter()
                .map(|p| given.unique(element("input", &p.name)))
                .collect(),
            outputs: diagram
                .outputs
                .iter()
                .map(|p| given.unique(element("output", &p.name)))
                .collect(),
            ..Names::default()
        };
        names.blocks_of(&diagram.blocks, &mut Vec::new(), &mut given);
        names.wires_of(&diagram.wires, &[], &mut given);
        names.inner_wires(&diagram.blocks, &mut Vec::new(), &mut given);
        names
    }

    pub(super) fn input(&self, at: usize) -> &str {
        self.inputs.get(at).map_or("", String::as_str)
    }

    pub(super) fn output(&self, at: usize) -> &str {
        self.outputs.get(at).map_or("", String::as_str)
    }

    /// The block at `path`, down the blocks inside blocks.
    pub(super) fn block(&self, path: &[usize]) -> &str {
        self.blocks.get(path).map_or("", String::as_str)
    }

    /// Wire `at` of the block at `path`, the diagram's at `[]`.
    pub(super) fn wire(&self, path: &[usize], at: usize) -> &str {
        self.wires
            .get(&(path.to_vec(), at))
            .map_or("", String::as_str)
    }

    fn blocks_of(&mut self, blocks: &[Block], path: &mut Vec<usize>, given: &mut Given) {
        for (index, block) in blocks.iter().enumerate() {
            path.push(index);
            self.blocks
                .insert(path.clone(), given.unique(element("block", &block.name)));
            self.blocks_of(&block.blocks, path, given);
            path.pop();
        }
    }

    fn wires_of(&mut self, wires: &[Wire], path: &[usize], given: &mut Given) {
        for (at, wire) in wires.iter().enumerate() {
            let said = wire.label.as_deref().unwrap_or_default();
            let name = link_name(&end(&wire.from), &end(&wire.to), said);
            self.wires.insert((path.to_vec(), at), given.unique(name));
        }
    }

    fn inner_wires(&mut self, blocks: &[Block], path: &mut Vec<usize>, given: &mut Given) {
        for (index, block) in blocks.iter().enumerate() {
            path.push(index);
            self.wires_of(&block.wires, path, given);
            self.inner_wires(&block.blocks, path, given);
            path.pop();
        }
    }
}

/// An end of a wire as its name holds it: a port of the diagram by its
/// name, a block's port as `Block.port`.
fn end(end: &End) -> String {
    match end {
        End::Boundary(name) => name.clone(),
        End::Port { block, port } => format!("{block}.{port}"),
    }
}
