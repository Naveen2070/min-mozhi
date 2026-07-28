use super::funcs::substitute_expr;
use super::*;

impl Emitter<'_> {
    pub(super) fn sized_field_expr(&mut self, e: &Expr, fty: &Type) -> String {
        if let ExprKind::Int { value, .. } = &e.kind {
            match fty {
                Type::Bit => {
                    // 1-bit field: emit as 1'b0 / 1'b1.
                    let lsb = match value {
                        crate::bits::Bits::Small(v) => v & 1,
                        crate::bits::Bits::Wide(limbs) => {
                            (limbs.first().copied().unwrap_or(0) & 1) as u128
                        }
                    };
                    return format!("1'b{lsb}");
                }
                Type::Bits(w_expr) => {
                    if let Ok(w) = consteval::eval(w_expr, &self.env) {
                        let w = w.to_i128_saturating() as u128;
                        let display_width = crate::bits::natural_width(value).max(1);
                        let v = crate::bits::bits_to_decimal_string(value, display_width, false);
                        return format!("{w}'d{v}");
                    }
                }
                Type::Signed(w_expr) => {
                    if let Ok(w) = consteval::eval(w_expr, &self.env) {
                        let w = w.to_i128_saturating() as u128;
                        let display_width = crate::bits::natural_width(value).max(1);
                        let v = crate::bits::bits_to_decimal_string(value, display_width, false);
                        return format!("{w}'sd{v}");
                    }
                }
                _ => {}
            }
        }
        self.expr(e)
    }

    /// Resolve a bundle type to its fields with concrete types, substituting
    /// any bundle parameters. Returns `Vec<(field_name, resolved_type)>`.
    /// Args in the `Type::Bundle` override bundle defaults; remaining params
    /// fold using the current env.
    ///
    /// A param whose arg forwards an identifier NOT in `self.env` (the
    /// common case: a module's own `parameter`, which stays a genuine
    /// symbolic Verilog parameter and is deliberately never folded to a
    /// literal — see `module()`'s header emission) is kept symbolic rather
    /// than silently falling back to the bundle's own unrelated default.
    /// Concretely: `module Child(W: int) { in req: Handshake(W: W) }` must
    /// emit `[(W)-1:0]` (tracking Child's own parameter through Verilog's
    /// own elaboration), not a literal folded from `Handshake`'s default.
    ///
    /// Use this at a module's OWN declaration (header, wire decls) — where
    /// no per-instance argument map exists and an unresolved param must
    /// stay symbolic. At an INSTANTIATION site, use
    /// [`Self::resolve_bundle_fields_for_instance`] instead, which also
    /// resolves a forwarded param against that instance's own concrete
    /// arguments (the `Ident("W")` in `Handshake(W: W)` means something
    /// different in the parent's scope than in the child's).
    pub(in crate::emit_verilog) fn resolve_bundle_fields(
        &self,
        bname: &QualIdent,
        args: &[NamedArg],
    ) -> Vec<(String, Type)> {
        self.resolve_bundle_fields_inner(bname, args, None)
    }

    /// Render one OR-mux operand's value for field `fname`. `??` chains
    /// left-associatively (`x ?? y ?? z` parses as `(x ?? y) ?? z`), so an
    /// operand here can itself be a nested `Coalesce` — in which case this
    /// recurses into [`Self::coalesce_field_expr`] to extract the same
    /// field from that sub-chain, rather than rendering the (bundle-typed)
    /// sub-expression as a plain signal and bolting `_fname` onto it.
    fn coalesce_operand_field(&mut self, operand: &Expr, fname: &str) -> String {
        if let ExprKind::Binary {
            op: BinOp::Coalesce,
            lhs,
            rhs,
        } = &operand.kind
        {
            self.coalesce_field_expr(lhs, rhs, fname)
        } else {
            let s = self.expr(operand);
            format!("{s}_{fname}")
        }
    }

    /// Render the per-field expression for extracting field `fname` from a
    /// `??` OR-mux expression (`lhs ?? rhs`, both bundle-typed): the
    /// `valid` field becomes `lhs_valid ? 1'b1 : rhs_valid`, every other
    /// field becomes `lhs_valid ? lhs_fname : rhs_fname`. `lhs`/`rhs` may
    /// themselves be nested `Coalesce` chains (`??` is left-associative and
    /// chains) — [`Self::coalesce_operand_field`] recurses through those
    /// rather than treating a bundle-typed operand as a plain signal.
    pub(in crate::emit_verilog) fn coalesce_field_expr(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        fname: &str,
    ) -> String {
        let l_valid = self.coalesce_operand_field(lhs, "valid");
        if fname == "valid" {
            let r_valid = self.coalesce_operand_field(rhs, "valid");
            format!("({l_valid} ? 1'b1 : {r_valid})")
        } else {
            let l = self.coalesce_operand_field(lhs, fname);
            let r = self.coalesce_operand_field(rhs, fname);
            format!("({l_valid} ? {l} : {r})")
        }
    }

    /// Like [`Self::resolve_bundle_fields`], but for a bundle-typed port at
    /// an instantiation site: `inst_args` is the SAME child-parameter
    /// substitution map `instance()` already builds for non-bundle port
    /// widths (see `width_subst`'s callers) — a param forwarding the
    /// child's own parameter (e.g. `Handshake(W: W)`) resolves against
    /// THIS instance's concrete argument for it, not `self.env` (the
    /// PARENT's scope, where the child's bare parameter name means
    /// nothing) and not symbolically (there is no such identifier in the
    /// parent's Verilog scope to reference).
    pub(super) fn resolve_bundle_fields_for_instance(
        &self,
        bname: &QualIdent,
        args: &[NamedArg],
        inst_args: &HashMap<&str, &Expr>,
    ) -> Vec<(String, Type)> {
        self.resolve_bundle_fields_inner(bname, args, Some(inst_args))
    }

    fn resolve_bundle_fields_inner(
        &self,
        bname: &QualIdent,
        args: &[NamedArg],
        inst_args: Option<&HashMap<&str, &Expr>>,
    ) -> Vec<(String, Type)> {
        let Some(bdecl) = self.project.resolve_bundle(bname) else {
            return vec![];
        };
        // Owned copy of inst_args's expressions, for use as a `substitute_expr`
        // symbol table (which needs owned `Expr`s, not borrowed `&Expr`s).
        let inst_args_owned: HashMap<String, Expr> = inst_args
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.to_string(), (*v).clone()))
                    .collect()
            })
            .unwrap_or_default();
        // Build the param bindings: each param is either a concrete literal
        // (`param_env`) or, when its arg forwards a symbol neither `self.env`
        // nor (at an instantiation site) `inst_args` can resolve, the
        // caller's own raw expression (`param_symbolic`) — never silently
        // defaulted when an arg was given.
        let mut param_env: HashMap<String, consteval::ConstVal> = HashMap::new();
        let mut param_symbolic: HashMap<String, Expr> = HashMap::new();
        for p in &bdecl.params {
            let arg = args.iter().find(|a| a.name.name == p.name.name);
            if let Some(a) = arg {
                if let Ok(v) = consteval::eval(&a.value, &self.env) {
                    param_env.insert(p.name.name.clone(), v);
                    continue;
                }
                if inst_args.is_some() {
                    let substituted = substitute_expr(&a.value, &self.env, &inst_args_owned);
                    if let Ok(v) = consteval::eval(&substituted, &self.env) {
                        param_env.insert(p.name.name.clone(), v);
                        continue;
                    }
                }
                param_symbolic.insert(p.name.name.clone(), a.value.clone());
            } else if let Some(default) = &p.default
                && let Ok(v) = consteval::eval(default, &self.env)
            {
                param_env.insert(p.name.name.clone(), v);
            }
        }
        // Merge param_env into env for field-type expression evaluation.
        // We do this by building a temporary Env that extends self.env.
        let mut merged_env = self.env.clone();
        for (k, v) in &param_env {
            merged_env.insert(k.clone(), v.clone());
        }
        // Resolve each field's type: evaluate width expressions fully to
        // integer literals using the merged env (bundle params + module
        // consts) when possible — this produces `[7:0]` rather than
        // `[(8)-1:0]` for clean Verilog output. When a param is symbolic
        // (forwards the enclosing module's own parameter), the width stays
        // symbolic too — `width_resolved`/`width_subst` already render a
        // non-literal `Type::Bits`/`Type::Signed` correctly.
        bdecl
            .fields
            .iter()
            .map(|f| {
                let resolved_ty = match &f.ty {
                    Type::Bit => Type::Bit,
                    Type::Bits(e) => Type::Bits(Box::new(self.resolve_field_width(
                        e,
                        &merged_env,
                        &param_symbolic,
                    ))),
                    Type::Signed(e) => Type::Signed(Box::new(self.resolve_field_width(
                        e,
                        &merged_env,
                        &param_symbolic,
                    ))),
                    // Enums and nested bundles: leave as-is (checker validates).
                    other => other.clone(),
                };
                (f.name.name.clone(), resolved_ty)
            })
            .collect()
    }

    /// One bundle field's width expression, resolved as far as it can be:
    /// a literal if every identifier in it is known (env or symbolic
    /// substitution), otherwise the substituted-but-still-symbolic
    /// expression (e.g. `Ident("W")`, referencing the enclosing module's
    /// own Verilog parameter) — never a hardcoded fallback to `1`.
    fn resolve_field_width(
        &self,
        e: &Expr,
        merged_env: &consteval::Env,
        param_symbolic: &HashMap<String, Expr>,
    ) -> Expr {
        if let Ok(w) = consteval::eval(e, merged_env) {
            return Expr {
                kind: ExprKind::Int {
                    value: w.bits.clone(),
                    raw: w.to_string(),
                },
                span: e.span,
            };
        }
        let substituted = substitute_expr(e, merged_env, param_symbolic);
        match consteval::eval(&substituted, merged_env) {
            Ok(w) => Expr {
                kind: ExprKind::Int {
                    value: w.bits.clone(),
                    raw: w.to_string(),
                },
                span: e.span,
            },
            Err(_) => substituted,
        }
    }
}

/// Raw bit-width of a scalar element `Type` (`Bit`/`Bits`/`Signed` — the
/// only shapes an array/mem ELEMENT type can be, per the checker). Used to
/// backfill `LocalLet::inferred_width` for the synthetic `var` binding
/// `ast::lower_foreach_fn`'s Elements form produces: the checker validates
/// that binding's uses via `cx.sigs` injection instead of ever
/// constructing this node (see `checker/widths/mod.rs`'s `FnStmt::ForEach`
/// arm doc comment), so nothing else ever sets this Cell for it.
pub(super) fn elem_width(ty: &Type, env: &Env) -> u32 {
    match ty {
        Type::Bit => 1,
        Type::Bits(e) | Type::Signed(e) => consteval::eval(e, env)
            .expect("checker already validated this array's element width")
            .to_i128_saturating() as u32,
        _ => 1, // unreachable: array/mem elements are never Array/Bundle/Named
    }
}
