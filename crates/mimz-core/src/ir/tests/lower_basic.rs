use crate::elaborate::{Design, Signal};
use crate::ir::lower;
use std::collections::BTreeMap;

fn empty_design(name: &str) -> Design {
    Design {
        module: name.to_string(),
        consts: BTreeMap::new(),
        inputs: vec![Signal {
            name: "a".to_string(),
            width: crate::elaborate::Width {
                bits: 8,
                signed: false,
            },
        }],
        outputs: vec![],
        wires: vec![],
        regs: vec![],
        mems: vec![],
        comb: BTreeMap::new(),
        procs: vec![],
        clocks: vec![],
        resets: vec![],
        funcs: Default::default(),
        unknown_signals: Default::default(),
        extern_instances: vec![],
        asserts: vec![],
        covers: vec![],
    }
}

#[test]
fn lowers_a_single_input_port_to_named_nets() {
    let design = empty_design("top");
    let module = lower(&design);
    assert_eq!(module.name, "top");
    assert_eq!(module.ports.len(), 1);
    let (name, bits, dir) = &module.ports[0];
    assert_eq!(name, "a");
    assert_eq!(bits.0.len(), 8);
    assert_eq!(*dir, crate::ast::Dir::In);
    for (i, net_id) in bits.0.iter().enumerate() {
        assert_eq!(module.nets[net_id.0 as usize].name.as_deref(), Some("a"));
        let _ = i;
    }
}
