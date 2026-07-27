use super::*;

#[test]
fn extern_module_parses_with_params_doc_and_ports() {
    let f = parse_ok(
        "extern module Pll(MULT: int = 2) {\n  \
         doc: \"50MHz input, 100MHz output\"\n  \
         in clk_in: bit\n  out clk_out: bit\n  out locked: bit\n}\n",
    );
    let TopItem::ExternModule(em) = &f.items[0] else {
        panic!("expected ExternModule, got {:?}", f.items[0]);
    };
    assert_eq!(em.name.name, "Pll");
    assert_eq!(em.verilog_name, None);
    assert_eq!(em.params.len(), 1);
    assert_eq!(em.params[0].name.name, "MULT");
    assert_eq!(em.doc.as_deref(), Some("50MHz input, 100MHz output"));
    assert_eq!(em.items.len(), 3);
}

#[test]
fn extern_module_parses_with_alias_and_no_params_or_doc() {
    let f = parse_ok("extern module Pll = \"PLL_HARD_IP_v2\" {\n  in clk_in: bit\n}\n");
    let TopItem::ExternModule(em) = &f.items[0] else {
        panic!("expected ExternModule, got {:?}", f.items[0]);
    };
    assert_eq!(em.name.name, "Pll");
    assert_eq!(em.verilog_name.as_deref(), Some("PLL_HARD_IP_v2"));
    assert!(em.params.is_empty());
    assert!(em.doc.is_none());
    assert_eq!(em.items.len(), 1);
}

#[test]
fn extern_module_body_rejects_wire_declarations() {
    let src = "extern module Pll {\n  in clk_in: bit\n  wire w: bit = clk_in\n}\n";
    parse_err(src);
}

#[test]
fn sync_double_flop_call_parses_as_a_builtin_call() {
    let src = "module M {\n\
                 clock clk_src\n\
                 clock clk_dst\n\
                 in fast_bit: bit\n\
                 reg slow_bit: bit = 0\n\
                 reset rst\n\
                 on rise(clk_dst) {\n\
                     slow_bit <- sync.double_flop(fast_bit, clk_src, clk_dst)\n\
                 }\n\
               }";
    let file = parse_ok(src);
    let TopItem::Module(m) = &file.items[0] else {
        panic!("expected a module")
    };
    let on = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::On(on) => Some(on),
            _ => None,
        })
        .expect("expected an on-block");
    let crate::ast::SeqStmt::Assign { rhs, .. } = &on.body[0] else {
        panic!("expected an assign statement")
    };
    let ExprKind::Call { func, args } = &rhs.kind else {
        panic!("expected a Call expression, got {:?}", rhs.kind)
    };
    assert_eq!(*func, Builtin::SyncDoubleFlop);
    assert_eq!(args.len(), 3);
}

#[test]
fn sync_pulse_call_parses_as_a_builtin_call() {
    let src = "module M {\n\
                 clock clk_src\n\
                 clock clk_dst\n\
                 in src_pulse: bit\n\
                 wire dst_pulse: bit = sync.pulse(src_pulse, clk_src, clk_dst)\n\
                 out o: bit\n\
                 o = dst_pulse\n\
               }";
    let file = parse_ok(src);
    let TopItem::Module(m) = &file.items[0] else {
        panic!("expected a module")
    };
    let init = m
        .items
        .iter()
        .find_map(|it| match it {
            ModuleItem::Wire { init, .. } => Some(init),
            _ => None,
        })
        .expect("expected a wire");
    let ExprKind::Call { func, args } = &init.kind else {
        panic!("expected a Call expression, got {:?}", init.kind)
    };
    assert_eq!(*func, Builtin::SyncPulse);
    assert_eq!(args.len(), 3);
}

#[test]
fn sync_dot_with_unknown_method_is_a_clean_parse_error() {
    let src = "module M {\n\
                 clock clk\n\
                 out o: bit\n\
                 o = sync.nonsense(1, clk, clk)\n\
               }";
    let result = parse(lex(src).expect("lex error"));
    // Verify it produces a clean error with the correct code, never a panic.
    match result {
        Err(diags) => {
            assert!(!diags.is_empty(), "expected at least one diagnostic");
            let has_e1116 = diags.iter().any(|d| d.code == Some("E1116"));
            assert!(
                has_e1116,
                "expected E1116 diagnostic, got: {:?}",
                diags.iter().map(|d| d.code).collect::<Vec<_>>()
            );
        }
        Ok(_) => panic!("expected parse error for unknown sync.nonsense method"),
    }
}
