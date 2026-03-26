//! LLVM IR code generation from panproto expressions.
//!
//! Compiles `panproto_expr::Expr` ASTs to LLVM IR via inkwell, then
//! JIT-compiles them to native code for accelerated evaluation.

#[cfg(feature = "inkwell-jit")]
mod inner {
    use std::collections::HashMap;

    use inkwell::OptimizationLevel;
    use inkwell::builder::Builder;
    use inkwell::context::Context;
    use inkwell::execution_engine::{ExecutionEngine, JitFunction};
    use inkwell::values::{BasicValueEnum, FloatValue, IntValue};

    use panproto_expr::{BuiltinOp, Expr, Literal};

    use crate::error::JitError;

    /// A JIT-compiled expression that can be called with integer arguments.
    ///
    /// The compiled function takes no arguments and returns an i64.
    /// For expressions that reference variables, the variables must be
    /// bound in the environment before compilation.
    pub struct CompiledExpr {
        /// The LLVM execution engine (must be kept alive while the function is callable).
        _engine: ExecutionEngine<'static>,
        /// The JIT-compiled function pointer.
        func: JitFunction<'static, unsafe extern "C" fn() -> i64>,
    }

    impl CompiledExpr {
        /// Call the compiled expression, returning the result as an i64.
        ///
        /// # Safety
        ///
        /// The execution engine must still be alive and the compiled code valid.
        #[must_use]
        pub fn call(&self) -> i64 {
            // SAFETY: The execution engine is held by this struct, so the function
            // pointer remains valid for the lifetime of CompiledExpr.
            unsafe { self.func.call() }
        }
    }

    /// JIT compiler for panproto expressions.
    ///
    /// Compiles closed expressions (no free variables) to native code
    /// via LLVM's ORC JIT.
    pub struct JitCompiler {
        context: &'static Context,
    }

    // SAFETY: The Context is allocated on the heap and lives for 'static.
    // LLVM contexts are thread-safe when not shared.
    unsafe impl Send for JitCompiler {}
    unsafe impl Sync for JitCompiler {}

    impl JitCompiler {
        /// Create a new JIT compiler.
        ///
        /// Allocates a new LLVM context that lives for the duration of the compiler.
        #[must_use]
        pub fn new() -> Self {
            // Leak a boxed Context to get a 'static reference.
            // This is intentional: LLVM contexts are heavyweight and should live
            // for the duration of the JIT compiler.
            let context = Box::leak(Box::new(Context::create()));
            Self { context }
        }

        /// Compile a closed expression to native code.
        ///
        /// The expression must not contain free variables (all variables must
        /// be bound by enclosing `Let` or `Lam` expressions). For expressions
        /// with free variables, use `compile_with_env`.
        ///
        /// # Errors
        ///
        /// Returns [`JitError`] if code generation or JIT compilation fails.
        pub fn compile(&self, expr: &Expr) -> Result<CompiledExpr, JitError> {
            let module = self.context.create_module("panproto_jit");
            let builder = self.context.create_builder();

            // Create the function: () -> i64
            let i64_type = self.context.i64_type();
            let fn_type = i64_type.fn_type(&[], false);
            let function = module.add_function("__panproto_eval", fn_type, None);
            let entry = self.context.append_basic_block(function, "entry");
            builder.position_at_end(entry);

            // Compile the expression body.
            let mut env = HashMap::new();
            let result = self.compile_expr(expr, &builder, &mut env)?;

            // Convert result to i64 for the return type.
            let ret_val = self.coerce_to_i64(&builder, result)?;
            builder
                .build_return(Some(&ret_val))
                .map_err(|e| JitError::CodegenFailed {
                    reason: format!("build_return: {e}"),
                })?;

            // Create the execution engine and JIT-compile.
            let engine = module
                .create_jit_execution_engine(OptimizationLevel::Default)
                .map_err(|e| JitError::CompilationFailed {
                    reason: e.to_string(),
                })?;

            // SAFETY: The function signature matches our fn_type declaration.
            let func = unsafe {
                engine
                    .get_function::<unsafe extern "C" fn() -> i64>("__panproto_eval")
                    .map_err(|e| JitError::CompilationFailed {
                        reason: format!("get_function: {e}"),
                    })?
            };

            Ok(CompiledExpr {
                _engine: engine,
                func,
            })
        }

        /// Compile an expression to an LLVM value.
        fn compile_expr<'ctx>(
            &self,
            expr: &Expr,
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            match expr {
                Expr::Lit(lit) => self.compile_literal(lit),

                Expr::Var(name) => {
                    env.get(name.as_ref())
                        .copied()
                        .ok_or_else(|| JitError::CodegenFailed {
                            reason: format!("unbound variable: {name}"),
                        })
                }

                Expr::Let { name, value, body } => {
                    let val = self.compile_expr(value, builder, env)?;
                    env.insert(name.to_string(), val);
                    let result = self.compile_expr(body, builder, env)?;
                    env.remove(name.as_ref());
                    Ok(result)
                }

                Expr::Builtin(op, args) => self.compile_builtin(*op, args, builder, env),

                Expr::Match { scrutinee, arms } => {
                    self.compile_match(scrutinee, arms, builder, env)
                }

                Expr::Record(_) | Expr::List(_) | Expr::Field(_, _) | Expr::Index(_, _) => {
                    Err(JitError::Unsupported {
                        reason: "compound types (record, list, field access, indexing) require \
                             heap-allocated runtime representations not available in the JIT; \
                             use the interpreter for expressions containing these constructs"
                            .to_owned(),
                    })
                }

                Expr::App(_, _) | Expr::Lam(_, _) => Err(JitError::Unsupported {
                    reason: "lambda/application requires closure compilation \
                                 (captures, function pointers) not available in the JIT; \
                                 use the interpreter for higher-order expressions"
                        .to_owned(),
                }),
            }
        }

        /// Compile a literal to an LLVM constant.
        fn compile_literal<'ctx>(&self, lit: &Literal) -> Result<BasicValueEnum<'ctx>, JitError> {
            match lit {
                Literal::Int(n) => {
                    // Reinterpret i64 bits as u64 for LLVM's const_int (sign_extend=true
                    // tells LLVM to treat the value as signed).
                    let bits = u64::from_ne_bytes(n.to_ne_bytes());
                    Ok(self.context.i64_type().const_int(bits, true).into())
                }
                Literal::Float(f) => Ok(self.context.f64_type().const_float(*f).into()),
                Literal::Bool(b) => Ok(self
                    .context
                    .bool_type()
                    .const_int(u64::from(*b), false)
                    .into()),
                Literal::Null => Ok(self.context.i64_type().const_int(0, false).into()),
                _ => Err(JitError::Unsupported {
                    reason: format!("literal type not supported in JIT: {lit:?}"),
                }),
            }
        }

        /// Helper to convert inkwell builder errors to [`JitError`].
        fn cg_err(e: impl std::fmt::Display) -> JitError {
            JitError::CodegenFailed {
                reason: e.to_string(),
            }
        }

        /// Compile a binary integer operation.
        fn compile_int_binop<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
            build_fn: fn(
                &Builder<'ctx>,
                IntValue<'ctx>,
                IntValue<'ctx>,
                &str,
            ) -> Result<IntValue<'ctx>, inkwell::builder::BuilderError>,
            name: &str,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let (lhs, rhs) = self.compile_binary_args(args, builder, env)?;
            let (l, r) = self.coerce_both_to_i64(builder, lhs, rhs)?;
            Ok(build_fn(builder, l, r, name).map_err(Self::cg_err)?.into())
        }

        /// Compile a rounding operation (floor or ceil).
        fn compile_round<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
            pred: inkwell::FloatPredicate,
            adjust_fn: fn(
                &Builder<'ctx>,
                IntValue<'ctx>,
                IntValue<'ctx>,
                &str,
            ) -> Result<IntValue<'ctx>, inkwell::builder::BuilderError>,
            name: &str,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let arg = self.compile_unary_arg(args, builder, env)?;
            let val = self.coerce_to_f64(builder, arg)?;
            let truncated = builder
                .build_float_to_signed_int(val, self.context.i64_type(), "trunc")
                .map_err(Self::cg_err)?;
            let back = builder
                .build_signed_int_to_float(truncated, self.context.f64_type(), "back")
                .map_err(Self::cg_err)?;
            let needs_adjust = builder
                .build_float_compare(pred, back, val, &format!("needs_{name}_adjust"))
                .map_err(Self::cg_err)?;
            let one = self.context.i64_type().const_int(1, false);
            let adjusted = adjust_fn(builder, truncated, one, &format!("{name}_adjusted"))
                .map_err(Self::cg_err)?;
            Ok(builder
                .build_select(needs_adjust, adjusted, truncated, name)
                .map_err(Self::cg_err)?
                .into_int_value()
                .into())
        }

        /// Compile a builtin operation.
        fn compile_builtin<'ctx>(
            &self,
            op: BuiltinOp,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            match op {
                BuiltinOp::Add => {
                    self.compile_int_binop(args, builder, env, Builder::build_int_add, "add")
                }
                BuiltinOp::Sub => {
                    self.compile_int_binop(args, builder, env, Builder::build_int_sub, "sub")
                }
                BuiltinOp::Mul => {
                    self.compile_int_binop(args, builder, env, Builder::build_int_mul, "mul")
                }
                BuiltinOp::Div => {
                    self.compile_int_binop(args, builder, env, Builder::build_int_signed_div, "div")
                }
                BuiltinOp::Mod => {
                    self.compile_int_binop(args, builder, env, Builder::build_int_signed_rem, "mod")
                }
                BuiltinOp::Neg => self.compile_neg(args, builder, env),
                BuiltinOp::Abs => self.compile_abs(args, builder, env),

                BuiltinOp::Eq => {
                    self.compile_int_cmp(inkwell::IntPredicate::EQ, args, builder, env)
                }
                BuiltinOp::Neq => {
                    self.compile_int_cmp(inkwell::IntPredicate::NE, args, builder, env)
                }
                BuiltinOp::Lt => {
                    self.compile_int_cmp(inkwell::IntPredicate::SLT, args, builder, env)
                }
                BuiltinOp::Lte => {
                    self.compile_int_cmp(inkwell::IntPredicate::SLE, args, builder, env)
                }
                BuiltinOp::Gt => {
                    self.compile_int_cmp(inkwell::IntPredicate::SGT, args, builder, env)
                }
                BuiltinOp::Gte => {
                    self.compile_int_cmp(inkwell::IntPredicate::SGE, args, builder, env)
                }
                BuiltinOp::And => {
                    self.compile_int_binop(args, builder, env, Builder::build_and, "and")
                }
                BuiltinOp::Or => {
                    self.compile_int_binop(args, builder, env, Builder::build_or, "or")
                }
                BuiltinOp::Not => self.compile_not(args, builder, env),
                BuiltinOp::IntToFloat => self.compile_int_to_float(args, builder, env),
                BuiltinOp::FloatToInt => self.compile_float_to_int(args, builder, env),

                BuiltinOp::Floor => self.compile_round(
                    args,
                    builder,
                    env,
                    inkwell::FloatPredicate::OGT,
                    Builder::build_int_sub,
                    "floor",
                ),
                BuiltinOp::Ceil => self.compile_round(
                    args,
                    builder,
                    env,
                    inkwell::FloatPredicate::OLT,
                    Builder::build_int_add,
                    "ceil",
                ),

                other => Err(JitError::Unsupported {
                    reason: format!(
                        "builtin {other:?} requires runtime support functions (string, list, record, graph ops)"
                    ),
                }),
            }
        }

        /// Compile integer negation.
        fn compile_neg<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let arg = self.compile_unary_arg(args, builder, env)?;
            let val = self.coerce_to_i64(builder, arg)?;
            Ok(builder
                .build_int_neg(val, "neg")
                .map_err(Self::cg_err)?
                .into())
        }

        /// Compile absolute value: `abs(x) = x >= 0 ? x : -x`.
        fn compile_abs<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let arg = self.compile_unary_arg(args, builder, env)?;
            let val = self.coerce_to_i64(builder, arg)?;
            let zero = self.context.i64_type().const_int(0, false);
            let is_neg = builder
                .build_int_compare(inkwell::IntPredicate::SLT, val, zero, "is_neg")
                .map_err(Self::cg_err)?;
            let neg_val = builder.build_int_neg(val, "neg").map_err(Self::cg_err)?;
            Ok(builder
                .build_select(is_neg, neg_val, val, "abs")
                .map_err(Self::cg_err)?
                .into_int_value()
                .into())
        }

        /// Compile bitwise NOT.
        fn compile_not<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let arg = self.compile_unary_arg(args, builder, env)?;
            let val = self.coerce_to_i64(builder, arg)?;
            Ok(builder.build_not(val, "not").map_err(Self::cg_err)?.into())
        }

        /// Compile integer to float conversion.
        fn compile_int_to_float<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let arg = self.compile_unary_arg(args, builder, env)?;
            let val = self.coerce_to_i64(builder, arg)?;
            Ok(builder
                .build_signed_int_to_float(val, self.context.f64_type(), "itof")
                .map_err(Self::cg_err)?
                .into())
        }

        /// Compile float to integer conversion.
        fn compile_float_to_int<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let arg = self.compile_unary_arg(args, builder, env)?;
            let val = self.coerce_to_f64(builder, arg)?;
            Ok(builder
                .build_float_to_signed_int(val, self.context.i64_type(), "ftoi")
                .map_err(Self::cg_err)?
                .into())
        }

        /// Compile a match expression.
        fn compile_match<'ctx>(
            &self,
            scrutinee: &Expr,
            arms: &[(panproto_expr::Pattern, Expr)],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let scrut_val = self.compile_expr(scrutinee, builder, env)?;

            // For simple literal pattern matching, use cascading comparisons.
            let function = builder
                .get_insert_block()
                .and_then(inkwell::basic_block::BasicBlock::get_parent)
                .ok_or_else(|| JitError::CodegenFailed {
                    reason: "no current function".to_owned(),
                })?;

            let merge_bb = self.context.append_basic_block(function, "match_merge");
            let i64_type = self.context.i64_type();
            let mut results: Vec<(BasicValueEnum<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)> =
                Vec::new();

            for (i, (pattern, body)) in arms.iter().enumerate() {
                match pattern {
                    panproto_expr::Pattern::Wildcard | panproto_expr::Pattern::Var(_) => {
                        // Wildcard/var: always matches. Bind if var.
                        if let panproto_expr::Pattern::Var(name) = pattern {
                            env.insert(name.to_string(), scrut_val);
                        }
                        let result = self.compile_expr(body, builder, env)?;
                        let current_bb =
                            builder
                                .get_insert_block()
                                .ok_or_else(|| JitError::CodegenFailed {
                                    reason: "no insert block".to_owned(),
                                })?;
                        results.push((result, current_bb));
                        builder
                            .build_unconditional_branch(merge_bb)
                            .map_err(Self::cg_err)?;
                        break; // Wildcard is always last.
                    }
                    panproto_expr::Pattern::Lit(lit) => {
                        let lit_val = self.compile_literal(lit)?;
                        let scrut_i64 = self.coerce_to_i64(builder, scrut_val)?;
                        let lit_i64 = self.coerce_to_i64(builder, lit_val)?;
                        let cmp = builder
                            .build_int_compare(
                                inkwell::IntPredicate::EQ,
                                scrut_i64,
                                lit_i64,
                                &format!("cmp_{i}"),
                            )
                            .map_err(Self::cg_err)?;

                        let then_bb = self
                            .context
                            .append_basic_block(function, &format!("arm_{i}"));
                        let else_bb = self
                            .context
                            .append_basic_block(function, &format!("next_{i}"));
                        builder
                            .build_conditional_branch(cmp, then_bb, else_bb)
                            .map_err(Self::cg_err)?;

                        builder.position_at_end(then_bb);
                        let result = self.compile_expr(body, builder, env)?;
                        let then_end =
                            builder
                                .get_insert_block()
                                .ok_or_else(|| JitError::CodegenFailed {
                                    reason: "no insert block".to_owned(),
                                })?;
                        results.push((result, then_end));
                        builder
                            .build_unconditional_branch(merge_bb)
                            .map_err(Self::cg_err)?;

                        builder.position_at_end(else_bb);
                    }
                    _ => {
                        return Err(JitError::Unsupported {
                            reason: format!("pattern type not supported in JIT: {pattern:?}"),
                        });
                    }
                }
            }

            // If we fell through all arms without matching, return 0.
            let current_bb = builder
                .get_insert_block()
                .ok_or_else(|| JitError::CodegenFailed {
                    reason: "no insert block".to_owned(),
                })?;
            let default_val = i64_type.const_int(0, false);
            results.push((default_val.into(), current_bb));
            builder
                .build_unconditional_branch(merge_bb)
                .map_err(Self::cg_err)?;

            // Build phi node in merge block.
            builder.position_at_end(merge_bb);
            let phi = builder
                .build_phi(i64_type, "match_result")
                .map_err(Self::cg_err)?;
            for (val, bb) in &results {
                let i64_val = self.coerce_to_i64(builder, *val)?;
                phi.add_incoming(&[(&i64_val, *bb)]);
            }

            Ok(phi.as_basic_value())
        }

        // ── helpers ────────────────────────────────────────────────────

        fn compile_binary_args<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<(BasicValueEnum<'ctx>, BasicValueEnum<'ctx>), JitError> {
            if args.len() != 2 {
                return Err(JitError::CodegenFailed {
                    reason: format!("expected 2 args, got {}", args.len()),
                });
            }
            let lhs = self.compile_expr(&args[0], builder, env)?;
            let rhs = self.compile_expr(&args[1], builder, env)?;
            Ok((lhs, rhs))
        }

        fn compile_unary_arg<'ctx>(
            &self,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            if args.is_empty() {
                return Err(JitError::CodegenFailed {
                    reason: "expected 1 arg, got 0".to_owned(),
                });
            }
            self.compile_expr(&args[0], builder, env)
        }

        fn compile_int_cmp<'ctx>(
            &self,
            pred: inkwell::IntPredicate,
            args: &[Expr],
            builder: &Builder<'ctx>,
            env: &mut HashMap<String, BasicValueEnum<'ctx>>,
        ) -> Result<BasicValueEnum<'ctx>, JitError> {
            let (lhs, rhs) = self.compile_binary_args(args, builder, env)?;
            let (l, r) = self.coerce_both_to_i64(builder, lhs, rhs)?;
            let cmp = builder
                .build_int_compare(pred, l, r, "cmp")
                .map_err(Self::cg_err)?;
            // Extend bool to i64.
            Ok(builder
                .build_int_z_extend(cmp, self.context.i64_type(), "ext")
                .map_err(Self::cg_err)?
                .into())
        }

        fn coerce_to_i64<'ctx>(
            &self,
            builder: &Builder<'ctx>,
            val: BasicValueEnum<'ctx>,
        ) -> Result<IntValue<'ctx>, JitError> {
            match val {
                BasicValueEnum::IntValue(i) => {
                    if i.get_type().get_bit_width() == 64 {
                        Ok(i)
                    } else {
                        Ok(builder
                            .build_int_z_extend(i, self.context.i64_type(), "ext")
                            .map_err(Self::cg_err)?)
                    }
                }
                BasicValueEnum::FloatValue(f) => Ok(builder
                    .build_float_to_signed_int(f, self.context.i64_type(), "ftoi")
                    .map_err(Self::cg_err)?),
                _ => Err(JitError::CodegenFailed {
                    reason: format!("cannot coerce {val:?} to i64"),
                }),
            }
        }

        fn coerce_to_f64<'ctx>(
            &self,
            builder: &Builder<'ctx>,
            val: BasicValueEnum<'ctx>,
        ) -> Result<FloatValue<'ctx>, JitError> {
            match val {
                BasicValueEnum::FloatValue(f) => Ok(f),
                BasicValueEnum::IntValue(i) => Ok(builder
                    .build_signed_int_to_float(i, self.context.f64_type(), "itof")
                    .map_err(Self::cg_err)?),
                _ => Err(JitError::CodegenFailed {
                    reason: format!("cannot coerce {val:?} to f64"),
                }),
            }
        }

        fn coerce_both_to_i64<'ctx>(
            &self,
            builder: &Builder<'ctx>,
            a: BasicValueEnum<'ctx>,
            b: BasicValueEnum<'ctx>,
        ) -> Result<(IntValue<'ctx>, IntValue<'ctx>), JitError> {
            Ok((
                self.coerce_to_i64(builder, a)?,
                self.coerce_to_i64(builder, b)?,
            ))
        }
    }

    impl Default for JitCompiler {
        fn default() -> Self {
            Self::new()
        }
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod tests {
        use super::*;

        #[test]
        fn jit_literal_int() {
            let jit = JitCompiler::new();
            let expr = Expr::Lit(Literal::Int(42));
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 42);
        }

        #[test]
        fn jit_add() {
            let jit = JitCompiler::new();
            let expr = Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Lit(Literal::Int(10)), Expr::Lit(Literal::Int(32))],
            );
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 42);
        }

        #[test]
        fn jit_arithmetic_chain() {
            let jit = JitCompiler::new();
            // (10 + 5) * 3 - 1 = 44
            let expr = Expr::Builtin(
                BuiltinOp::Sub,
                vec![
                    Expr::Builtin(
                        BuiltinOp::Mul,
                        vec![
                            Expr::Builtin(
                                BuiltinOp::Add,
                                vec![Expr::Lit(Literal::Int(10)), Expr::Lit(Literal::Int(5))],
                            ),
                            Expr::Lit(Literal::Int(3)),
                        ],
                    ),
                    Expr::Lit(Literal::Int(1)),
                ],
            );
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 44);
        }

        #[test]
        fn jit_comparison() {
            let jit = JitCompiler::new();
            // 5 < 10 → 1 (true)
            let expr = Expr::Builtin(
                BuiltinOp::Lt,
                vec![Expr::Lit(Literal::Int(5)), Expr::Lit(Literal::Int(10))],
            );
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 1);

            // 10 < 5 → 0 (false)
            let expr2 = Expr::Builtin(
                BuiltinOp::Lt,
                vec![Expr::Lit(Literal::Int(10)), Expr::Lit(Literal::Int(5))],
            );
            let result2 = jit.compile(&expr2).unwrap();
            assert_eq!(result2.call(), 0);
        }

        #[test]
        fn jit_negation() {
            let jit = JitCompiler::new();
            let expr = Expr::Builtin(BuiltinOp::Neg, vec![Expr::Lit(Literal::Int(42))]);
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), -42);
        }

        #[test]
        fn jit_abs() {
            let jit = JitCompiler::new();
            let expr = Expr::Builtin(BuiltinOp::Abs, vec![Expr::Lit(Literal::Int(-42))]);
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 42);
        }

        #[test]
        fn jit_let_binding() {
            let jit = JitCompiler::new();
            // let x = 10 in let y = 32 in x + y
            let expr = Expr::Let {
                name: "x".into(),
                value: Box::new(Expr::Lit(Literal::Int(10))),
                body: Box::new(Expr::Let {
                    name: "y".into(),
                    value: Box::new(Expr::Lit(Literal::Int(32))),
                    body: Box::new(Expr::Builtin(
                        BuiltinOp::Add,
                        vec![Expr::Var("x".into()), Expr::Var("y".into())],
                    )),
                }),
            };
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 42);
        }

        #[test]
        fn jit_match_literal() {
            let jit = JitCompiler::new();
            // match 2 { 1 => 10, 2 => 20, _ => 0 }
            let expr = Expr::Match {
                scrutinee: Box::new(Expr::Lit(Literal::Int(2))),
                arms: vec![
                    (
                        panproto_expr::Pattern::Lit(Literal::Int(1)),
                        Expr::Lit(Literal::Int(10)),
                    ),
                    (
                        panproto_expr::Pattern::Lit(Literal::Int(2)),
                        Expr::Lit(Literal::Int(20)),
                    ),
                    (panproto_expr::Pattern::Wildcard, Expr::Lit(Literal::Int(0))),
                ],
            };
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 20);
        }

        #[test]
        fn jit_boolean_logic() {
            let jit = JitCompiler::new();
            // (1 AND 1) OR 0 = 1
            let expr = Expr::Builtin(
                BuiltinOp::Or,
                vec![
                    Expr::Builtin(
                        BuiltinOp::And,
                        vec![Expr::Lit(Literal::Int(1)), Expr::Lit(Literal::Int(1))],
                    ),
                    Expr::Lit(Literal::Int(0)),
                ],
            );
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 1);
        }

        #[test]
        fn jit_mod() {
            let jit = JitCompiler::new();
            // 17 % 5 = 2
            let expr = Expr::Builtin(
                BuiltinOp::Mod,
                vec![Expr::Lit(Literal::Int(17)), Expr::Lit(Literal::Int(5))],
            );
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 2);
        }

        #[test]
        fn jit_complex_expression() {
            let jit = JitCompiler::new();
            // let a = 100 in let b = 58 in a - b
            let expr = Expr::Let {
                name: "a".into(),
                value: Box::new(Expr::Lit(Literal::Int(100))),
                body: Box::new(Expr::Let {
                    name: "b".into(),
                    value: Box::new(Expr::Lit(Literal::Int(58))),
                    body: Box::new(Expr::Builtin(
                        BuiltinOp::Sub,
                        vec![Expr::Var("a".into()), Expr::Var("b".into())],
                    )),
                }),
            };
            let result = jit.compile(&expr).unwrap();
            assert_eq!(result.call(), 42);
        }
    }
}

#[cfg(feature = "inkwell-jit")]
pub use inner::{CompiledExpr, JitCompiler};
