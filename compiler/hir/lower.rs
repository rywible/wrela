use crate::hir::*;
use crate::parser::ast::{self, AstNode};
use crate::parser::kind::SyntaxKind;
use crate::parser::{SyntaxNode, SyntaxToken};
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};

pub fn lower(root: ast::Root) -> Module {
    let mut ctx = LoweringContext::default();
    ctx.lower_module(root)
}

pub fn lower_root_body(root: ast::Root) -> Option<Body> {
    let mut body_ctx = BodyLoweringContext::new();
    let mut has_stmt = false;
    for stmt in root.statements() {
        match stmt {
            ast::Stmt::FuncDef(_)
            | ast::Stmt::KernelDef(_)
            | ast::Stmt::SystemDef(_)
            | ast::Stmt::FieldDecl(_)
            | ast::Stmt::RegionDecl(_)
            | ast::Stmt::DomainDecl(_)
            | ast::Stmt::RenderDecl(_)
            | ast::Stmt::RadianceDecl(_)
            | ast::Stmt::VolumeDecl(_)
            | ast::Stmt::MaterialDecl(_)
            | ast::Stmt::ShapeDecl(_)
            | ast::Stmt::ClassDef(_)
            | ast::Stmt::ValueDef(_)
            | ast::Stmt::EnumDef(_)
            | ast::Stmt::UseStmt(_)
            | ast::Stmt::PrivateBlock(_) => {}
            other => {
                let s = body_ctx.lower_stmt(other);
                body_ctx.body.root_stmts.push(s);
                has_stmt = true;
            }
        }
    }
    if has_stmt { Some(body_ctx.body) } else { None }
}

#[derive(Default)]
struct LoweringContext {
    module: Module,
}

impl Module {
    fn new() -> Self {
        Self {
            functions: Arena::new(),
            classes: Arena::new(),
            enums: Arena::new(),
            interfaces: Arena::new(),
            shapes: Arena::new(),
            uses: Vec::new(),
        }
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

impl LoweringContext {
    fn finalize_implicit_return(body: &mut Body, ret_type: Option<&TypeRef>) {
        let expects_value = ret_type.is_some_and(|ret| ret.name != "Nothing");
        if !expects_value {
            return;
        }
        let Some(last_stmt) = body.root_stmts.last().copied() else {
            return;
        };
        if let Stmt::Expr(expr) = body.stmts[last_stmt] {
            body.stmts[last_stmt] = Stmt::Return(Some(expr));
        }
    }

    fn lower_module(&mut self, root: ast::Root) -> Module {
        for stmt in root.statements() {
            match stmt {
                ast::Stmt::FuncDef(f) => {
                    let func = self.lower_func(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::KernelDef(f) => {
                    let func = self.lower_kernel_def(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::SystemDef(f) => {
                    let func = self.lower_system_def(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::FieldDecl(f) => {
                    let func = self.lower_field_decl(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::RegionDecl(r) => {
                    let func = self.lower_region_decl(r);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::DomainDecl(d) => {
                    let func = self.lower_domain_decl(d);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::RenderDecl(r) => {
                    let func = self.lower_render_decl(r);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::RadianceDecl(f) => {
                    let func = self.lower_radiance_decl(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::VolumeDecl(f) => {
                    let func = self.lower_volume_decl(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::MaterialDecl(f) => {
                    let func = self.lower_material_decl(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::ShapeDecl(s) => {
                    let shape = self.lower_shape_decl(s);
                    self.module.shapes.alloc(shape);
                }
                ast::Stmt::ClassDef(c) => {
                    if self.class_is_interface(&c) {
                        let interface = self.lower_interface_from_class(c);
                        self.module.interfaces.alloc(interface);
                    } else {
                        let class = self.lower_class(c);
                        self.module.classes.alloc(class);
                    }
                }
                ast::Stmt::ResourceDef(c) => {
                    let class = self.lower_class_like(c, ClassRole::Resource);
                    self.module.classes.alloc(class);
                }
                ast::Stmt::ValueDef(c) => {
                    let class = self.lower_class_like(c, ClassRole::Value);
                    self.module.classes.alloc(class);
                }
                ast::Stmt::EnumDef(e) => {
                    let en = self.lower_enum(e);
                    self.module.enums.alloc(en);
                }
                ast::Stmt::PrivateBlock(block) => {
                    for stmt in block.statements() {
                        match stmt {
                            ast::Stmt::FuncDef(f) => {
                                let func = self.lower_func(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::KernelDef(f) => {
                                let func = self.lower_kernel_def(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::SystemDef(f) => {
                                let func = self.lower_system_def(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::FieldDecl(f) => {
                                let func = self.lower_field_decl(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::RegionDecl(r) => {
                                let func = self.lower_region_decl(r);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::DomainDecl(d) => {
                                let func = self.lower_domain_decl(d);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::RenderDecl(r) => {
                                let func = self.lower_render_decl(r);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::RadianceDecl(f) => {
                                let func = self.lower_radiance_decl(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::VolumeDecl(f) => {
                                let func = self.lower_volume_decl(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::MaterialDecl(f) => {
                                let func = self.lower_material_decl(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::ShapeDecl(s) => {
                                let shape = self.lower_shape_decl(s);
                                self.module.shapes.alloc(shape);
                            }
                            ast::Stmt::ClassDef(c) => {
                                if self.class_is_interface(&c) {
                                    let interface = self.lower_interface_from_class(c);
                                    self.module.interfaces.alloc(interface);
                                } else {
                                    let class = self.lower_class(c);
                                    self.module.classes.alloc(class);
                                }
                            }
                            ast::Stmt::ResourceDef(c) => {
                                let class = self.lower_class_like(c, ClassRole::Resource);
                                self.module.classes.alloc(class);
                            }
                            ast::Stmt::ValueDef(c) => {
                                let class = self.lower_class_like(c, ClassRole::Value);
                                self.module.classes.alloc(class);
                            }
                            ast::Stmt::EnumDef(e) => {
                                let en = self.lower_enum(e);
                                self.module.enums.alloc(en);
                            }
                            _ => {}
                        }
                    }
                }
                ast::Stmt::UseStmt(u) => {
                    let (names, module, module_span) = parse_use_stmt(&u);
                    self.module.uses.push(UseStmt {
                        names,
                        module,
                        module_span,
                        span: u.syntax().text_range(),
                    });
                }
                _ => {
                    // Top-level executable statements are rejected; entrypoint is `run`.
                }
            }
        }
        self.finalize_field_metadata();
        std::mem::take(&mut self.module)
    }

    fn finalize_field_metadata(&mut self) {
        let mut field_cache = HashMap::new();
        let mut shape_cache = HashMap::new();
        let mut visiting_fields = HashSet::new();
        let mut visiting_shapes = HashSet::new();

        for idx in 0..self.module.functions.len() {
            let func_idx = Idx::new(idx);
            let Some(field) = self.module.functions[func_idx].field.clone() else {
                continue;
            };
            let Some(graph) = self.module.functions[func_idx].field_graph.clone() else {
                continue;
            };
            let trace = self.field_graph_trace_metadata(
                &graph.root,
                self.field_point_param_name(func_idx).as_ref(),
                &mut field_cache,
                &mut shape_cache,
                &mut visiting_fields,
                &mut visiting_shapes,
            );
            let trace = match field.class {
                FieldClass::Exact => trace,
                FieldClass::Conservative => GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    ..trace
                },
            };
            let execution_trace = self.apply_authored_field_support(trace, &field);
            self.module.functions[func_idx].field = Some(FieldMetadata {
                support: execution_trace.support,
                bounds: execution_trace.bounds,
                trace: execution_trace,
                ..field
            });
            self.module.functions[func_idx].field_graph = Some(FieldGraph {
                root: graph.root,
                trace,
            });
        }

        for idx in 0..self.module.shapes.len() {
            let shape_idx = Idx::new(idx);
            let Some(graph) = self.module.shapes[shape_idx].graph.clone() else {
                continue;
            };
            let trace = self.shape_graph_trace_metadata(
                &graph.root,
                &mut field_cache,
                &mut shape_cache,
                &mut visiting_fields,
                &mut visiting_shapes,
            );
            self.module.shapes[shape_idx].graph = Some(ShapeGraph {
                root: graph.root,
                provenance: graph.provenance,
                trace,
            });
        }
    }

    fn field_graph_trace_metadata(
        &self,
        expr: &FieldExpr,
        point_param: Option<&SmolStr>,
        field_cache: &mut HashMap<SmolStr, GraphTraceMetadata>,
        shape_cache: &mut HashMap<SmolStr, GraphTraceMetadata>,
        visiting_fields: &mut HashSet<SmolStr>,
        visiting_shapes: &mut HashSet<SmolStr>,
    ) -> GraphTraceMetadata {
        match expr {
            FieldExpr::Use { target } => self.field_trace_for_target(
                target,
                None,
                field_cache,
                shape_cache,
                visiting_fields,
                visiting_shapes,
            ),
            FieldExpr::Primitive { primitive, .. } => {
                self.field_primitive_trace_metadata(*primitive)
            }
            FieldExpr::Union { items } | FieldExpr::Intersection { items } => self
                .combine_trace_metadata(items.iter().map(|item| {
                    self.field_graph_trace_metadata(
                        item,
                        point_param,
                        field_cache,
                        shape_cache,
                        visiting_fields,
                        visiting_shapes,
                    )
                })),
            FieldExpr::Subtract { left, right } => {
                let left = self.field_graph_trace_metadata(
                    left,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                let right = self.field_graph_trace_metadata(
                    right,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                GraphTraceMetadata {
                    class: left.combine_class(right),
                    support: left.support,
                    bounds: left.bounds,
                    can_coarse_support_pruning: left.can_coarse_support_pruning,
                    smooth_op_count: left.smooth_op_count + right.smooth_op_count,
                    deform_op_count: left.deform_op_count + right.deform_op_count,
                }
            }
            FieldExpr::Translate { translate, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(translate, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                if self.field_wrapper_body_returns_named_call(translate, "vec3") {
                    GraphTraceMetadata {
                        class: body.class,
                        ..body
                    }
                } else {
                    GraphTraceMetadata {
                        class: FieldClass::Conservative,
                        support: body.support,
                        bounds: body.bounds,
                        can_coarse_support_pruning: false,
                        smooth_op_count: body.smooth_op_count,
                        deform_op_count: body.deform_op_count,
                    }
                }
            }
            FieldExpr::Rotate { rotate, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(rotate, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                if self.field_wrapper_body_returns_named_call(rotate, "vec3") {
                    GraphTraceMetadata {
                        class: body.class,
                        ..body
                    }
                } else {
                    GraphTraceMetadata {
                        class: FieldClass::Conservative,
                        support: body.support,
                        bounds: body.bounds,
                        can_coarse_support_pruning: false,
                        smooth_op_count: body.smooth_op_count,
                        deform_op_count: body.deform_op_count,
                    }
                }
            }
            FieldExpr::UniformScale { scale, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(scale, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                if self
                    .field_wrapper_body_numeric_literal(scale)
                    .is_some_and(|value| value > 0.0)
                {
                    GraphTraceMetadata {
                        class: body.class,
                        ..body
                    }
                } else {
                    GraphTraceMetadata {
                        class: FieldClass::Conservative,
                        support: body.support,
                        bounds: body.bounds,
                        can_coarse_support_pruning: false,
                        smooth_op_count: body.smooth_op_count,
                        deform_op_count: body.deform_op_count,
                    }
                }
            }
            FieldExpr::AffineTransform { transform, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(transform, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: body.support,
                    bounds: body.bounds,
                    can_coarse_support_pruning: false,
                    smooth_op_count: body.smooth_op_count,
                    deform_op_count: body.deform_op_count + 1,
                }
            }
            FieldExpr::Warp { warp, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(warp, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: body.support,
                    bounds: body.bounds,
                    can_coarse_support_pruning: false,
                    smooth_op_count: body.smooth_op_count,
                    deform_op_count: body.deform_op_count + 1,
                }
            }
            FieldExpr::RepeatLinear { repeat, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(repeat, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                if self.field_wrapper_body_returns_named_call(repeat, "vec3") {
                    GraphTraceMetadata {
                        class: body.class,
                        support: FieldSupport::Periodic,
                        bounds: FieldBounds::Unbounded,
                        can_coarse_support_pruning: body.can_coarse_support_pruning,
                        smooth_op_count: body.smooth_op_count,
                        deform_op_count: body.deform_op_count,
                    }
                } else {
                    GraphTraceMetadata {
                        class: FieldClass::Conservative,
                        support: FieldSupport::Periodic,
                        bounds: FieldBounds::Unbounded,
                        can_coarse_support_pruning: false,
                        smooth_op_count: body.smooth_op_count,
                        deform_op_count: body.deform_op_count,
                    }
                }
            }
            FieldExpr::RepeatGrid { repeat, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(repeat, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                if self.field_wrapper_body_returns_named_call(repeat, "vec3") {
                    GraphTraceMetadata {
                        class: body.class,
                        support: FieldSupport::Periodic,
                        bounds: FieldBounds::Unbounded,
                        can_coarse_support_pruning: body.can_coarse_support_pruning,
                        smooth_op_count: body.smooth_op_count,
                        deform_op_count: body.deform_op_count,
                    }
                } else {
                    GraphTraceMetadata {
                        class: FieldClass::Conservative,
                        support: FieldSupport::Periodic,
                        bounds: FieldBounds::Unbounded,
                        can_coarse_support_pruning: false,
                        smooth_op_count: body.smooth_op_count,
                        deform_op_count: body.deform_op_count,
                    }
                }
            }
            FieldExpr::RadialRepeat { radial, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(radial, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: FieldSupport::Periodic,
                    bounds: FieldBounds::Unbounded,
                    can_coarse_support_pruning: false,
                    smooth_op_count: body.smooth_op_count,
                    deform_op_count: body.deform_op_count + 1,
                }
            }
            FieldExpr::MirrorArray { mirror, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(mirror, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                if self.field_wrapper_body_returns_named_call(mirror, "vec3") {
                    GraphTraceMetadata {
                        class: body.class,
                        ..body
                    }
                } else {
                    GraphTraceMetadata {
                        class: FieldClass::Conservative,
                        support: body.support,
                        bounds: body.bounds,
                        can_coarse_support_pruning: false,
                        smooth_op_count: body.smooth_op_count,
                        deform_op_count: body.deform_op_count,
                    }
                }
            }
            FieldExpr::InstanceArray { instance, body } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(instance, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: body.support,
                    bounds: body.bounds,
                    can_coarse_support_pruning: false,
                    smooth_op_count: body.smooth_op_count,
                    deform_op_count: body.deform_op_count + 1,
                }
            }
            FieldExpr::SmoothUnion { smoothing, items } => {
                let body = self.combine_trace_metadata(items.iter().map(|item| {
                    self.field_graph_trace_metadata(
                        item,
                        point_param,
                        field_cache,
                        shape_cache,
                        visiting_fields,
                        visiting_shapes,
                    )
                }));
                if point_param.is_some_and(|name| self.body_references_variable(smoothing, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: body.support,
                    bounds: body.bounds,
                    can_coarse_support_pruning: body.can_coarse_support_pruning,
                    smooth_op_count: body.smooth_op_count + 1,
                    deform_op_count: body.deform_op_count,
                }
            }
            FieldExpr::SmoothIntersection { smoothing, items } => {
                let body = self.combine_trace_metadata(items.iter().map(|item| {
                    self.field_graph_trace_metadata(
                        item,
                        point_param,
                        field_cache,
                        shape_cache,
                        visiting_fields,
                        visiting_shapes,
                    )
                }));
                if point_param.is_some_and(|name| self.body_references_variable(smoothing, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: body.support,
                    bounds: body.bounds,
                    can_coarse_support_pruning: body.can_coarse_support_pruning,
                    smooth_op_count: body.smooth_op_count + 1,
                    deform_op_count: body.deform_op_count,
                }
            }
            FieldExpr::SmoothSubtract {
                smoothing,
                left,
                right,
            } => {
                let left = self.field_graph_trace_metadata(
                    left,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                let right = self.field_graph_trace_metadata(
                    right,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(smoothing, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: left.support,
                    bounds: left.bounds,
                    can_coarse_support_pruning: left.can_coarse_support_pruning,
                    smooth_op_count: left.smooth_op_count + right.smooth_op_count + 1,
                    deform_op_count: left.deform_op_count + right.deform_op_count,
                }
            }
            FieldExpr::Bend { bend, body }
            | FieldExpr::Twist { twist: bend, body }
            | FieldExpr::Taper { taper: bend, body }
            | FieldExpr::Displace {
                displace: bend,
                body,
            } => {
                let body = self.field_graph_trace_metadata(
                    body,
                    point_param,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                if point_param.is_some_and(|name| self.body_references_variable(bend, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: body.support,
                    bounds: body.bounds,
                    can_coarse_support_pruning: false,
                    smooth_op_count: body.smooth_op_count,
                    deform_op_count: body.deform_op_count + 1,
                }
            }
            FieldExpr::Extrude { height, profile } => {
                if point_param.is_some_and(|name| self.body_references_variable(height, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                let profile = self.profile_trace_metadata(profile);
                GraphTraceMetadata {
                    class: if self
                        .field_wrapper_body_numeric_literal(height)
                        .is_some_and(|value| value > 0.0)
                    {
                        profile.class
                    } else {
                        FieldClass::Conservative
                    },
                    support: FieldSupport::Bounded,
                    bounds: FieldBounds::Bounded,
                    can_coarse_support_pruning: true,
                    smooth_op_count: profile.smooth_op_count,
                    deform_op_count: profile.deform_op_count,
                }
            }
            FieldExpr::Revolve { profile } => {
                let profile = self.profile_trace_metadata(profile);
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: FieldSupport::Bounded,
                    bounds: FieldBounds::Bounded,
                    can_coarse_support_pruning: true,
                    smooth_op_count: profile.smooth_op_count,
                    deform_op_count: profile.deform_op_count,
                }
            }
            FieldExpr::Sweep { path, profile } => {
                if point_param.is_some_and(|name| self.body_references_variable(path, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                let profile = self.profile_trace_metadata(profile);
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: FieldSupport::Bounded,
                    bounds: FieldBounds::Bounded,
                    can_coarse_support_pruning: true,
                    smooth_op_count: profile.smooth_op_count,
                    deform_op_count: profile.deform_op_count,
                }
            }
            FieldExpr::Loft { height, from, to } => {
                if point_param.is_some_and(|name| self.body_references_variable(height, name)) {
                    return GraphTraceMetadata::pessimistic();
                }
                let from = self.profile_trace_metadata(from);
                let to = self.profile_trace_metadata(to);
                GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: FieldSupport::Bounded,
                    bounds: FieldBounds::Bounded,
                    can_coarse_support_pruning: true,
                    smooth_op_count: from.smooth_op_count + to.smooth_op_count,
                    deform_op_count: from.deform_op_count + to.deform_op_count,
                }
            }
            FieldExpr::Custom { .. } => GraphTraceMetadata::pessimistic(),
        }
    }

    fn profile_trace_metadata(&self, expr: &ProfileExpr) -> GraphTraceMetadata {
        match expr {
            ProfileExpr::Primitive { .. } => {
                GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true)
            }
        }
    }

    fn field_primitive_trace_metadata(&self, primitive: FieldPrimitive) -> GraphTraceMetadata {
        match primitive {
            FieldPrimitive::Plane => {
                GraphTraceMetadata::exact(FieldSupport::Unbounded, FieldBounds::Unbounded, false)
            }
            FieldPrimitive::Sphere
            | FieldPrimitive::Box
            | FieldPrimitive::Capsule
            | FieldPrimitive::Cylinder
            | FieldPrimitive::Torus
            | FieldPrimitive::RoundedBox
            | FieldPrimitive::CappedCone
            | FieldPrimitive::BoxFrame
            | FieldPrimitive::TrianglePrism
            | FieldPrimitive::HexPrism => {
                GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true)
            }
            FieldPrimitive::Ellipsoid => {
                GraphTraceMetadata::conservative(FieldSupport::Bounded, FieldBounds::Bounded, true)
            }
            FieldPrimitive::Cone | FieldPrimitive::Slab => {
                GraphTraceMetadata::exact(FieldSupport::Unbounded, FieldBounds::Unbounded, false)
            }
        }
    }

    fn field_wrapper_body_returns_named_call(&self, body: &Body, name: &str) -> bool {
        let Some(expr) = self.field_wrapper_body_terminal_expr(body) else {
            return false;
        };
        let Expr::Call { callee, .. } = &body.exprs[expr] else {
            return false;
        };
        matches!(&body.exprs[*callee], Expr::Variable(callee_name) if callee_name == name)
    }

    fn field_wrapper_body_terminal_expr(&self, body: &Body) -> Option<Idx<Expr>> {
        let stmt = *body.root_stmts.last()?;
        match &body.stmts[stmt] {
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => Some(*expr),
            _ => None,
        }
    }

    fn field_wrapper_body_numeric_literal(&self, body: &Body) -> Option<f64> {
        let expr_id = self.field_wrapper_body_terminal_expr(body)?;
        self.field_wrapper_numeric_literal(body, expr_id)
    }

    fn field_wrapper_numeric_literal(&self, body: &Body, expr_id: Idx<Expr>) -> Option<f64> {
        match &body.exprs[expr_id] {
            Expr::Literal(Literal::Integer(value)) => Some(*value as f64),
            Expr::Literal(Literal::Float(value)) => Some(*value),
            Expr::Unary {
                op: UnaryOp::Neg,
                expr,
                ..
            } => self
                .field_wrapper_numeric_literal(body, *expr)
                .map(|value| -value),
            Expr::Call { callee, args, .. } => {
                let Expr::Variable(name) = &body.exprs[*callee] else {
                    return None;
                };
                if name.as_str() != "f32" && name.as_str() != "to_f32" {
                    return None;
                }
                let [arg] = args.as_slice() else {
                    return None;
                };
                match arg {
                    Arg::Positional { value, .. } | Arg::Named { value, .. } => {
                        self.field_wrapper_numeric_literal(body, *value)
                    }
                }
            }
            _ => None,
        }
    }

    fn shape_graph_trace_metadata(
        &self,
        expr: &ShapeExpr,
        field_cache: &mut HashMap<SmolStr, GraphTraceMetadata>,
        shape_cache: &mut HashMap<SmolStr, GraphTraceMetadata>,
        visiting_fields: &mut HashSet<SmolStr>,
        visiting_shapes: &mut HashSet<SmolStr>,
    ) -> GraphTraceMetadata {
        match expr {
            ShapeExpr::Use { target } => self.shape_trace_for_target(
                target,
                field_cache,
                shape_cache,
                visiting_fields,
                visiting_shapes,
            ),
            ShapeExpr::Union { items, .. } | ShapeExpr::Intersection { items, .. } => self
                .combine_trace_metadata(items.iter().map(|item| {
                    self.shape_graph_trace_metadata(
                        item,
                        field_cache,
                        shape_cache,
                        visiting_fields,
                        visiting_shapes,
                    )
                })),
            ShapeExpr::Subtract { left, right, .. } => {
                let left = self.shape_graph_trace_metadata(
                    left,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                let right = self.shape_graph_trace_metadata(
                    right,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                GraphTraceMetadata {
                    class: left.combine_class(right),
                    support: left.support,
                    bounds: left.bounds,
                    can_coarse_support_pruning: left.can_coarse_support_pruning,
                    smooth_op_count: left.smooth_op_count + right.smooth_op_count,
                    deform_op_count: left.deform_op_count + right.deform_op_count,
                }
            }
            ShapeExpr::Leaf(leaf) => self.field_trace_for_target(
                &leaf.field,
                None,
                field_cache,
                shape_cache,
                visiting_fields,
                visiting_shapes,
            ),
        }
    }

    fn field_trace_for_target(
        &self,
        target: &SmolStr,
        _point_param: Option<&SmolStr>,
        field_cache: &mut HashMap<SmolStr, GraphTraceMetadata>,
        shape_cache: &mut HashMap<SmolStr, GraphTraceMetadata>,
        visiting_fields: &mut HashSet<SmolStr>,
        visiting_shapes: &mut HashSet<SmolStr>,
    ) -> GraphTraceMetadata {
        if let Some(metadata) = field_cache.get(target).copied() {
            return metadata;
        }
        if !visiting_fields.insert(target.clone()) {
            return GraphTraceMetadata::pessimistic();
        }
        let metadata = self
            .module
            .functions
            .iter()
            .find(|(_, func)| func.name == *target && matches!(func.role, FunctionRole::Field))
            .and_then(|(idx, func)| {
                let graph = func.field_graph.as_ref()?;
                let graph_trace = self.field_graph_trace_metadata(
                    &graph.root,
                    self.field_point_param_name(idx).as_ref(),
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                );
                let class = func
                    .field
                    .as_ref()
                    .map(|field| field.class)
                    .unwrap_or(FieldClass::Conservative);
                Some(match class {
                    FieldClass::Exact => graph_trace,
                    FieldClass::Conservative => GraphTraceMetadata {
                        class: FieldClass::Conservative,
                        ..graph_trace
                    },
                })
            })
            .or_else(|| {
                Self::field_primitive_from_name(target)
                    .map(|primitive| self.field_primitive_trace_metadata(primitive))
            })
            .unwrap_or_else(GraphTraceMetadata::pessimistic);
        visiting_fields.remove(target);
        field_cache.insert(target.clone(), metadata);
        metadata
    }

    fn field_point_param_name(&self, func_idx: Idx<Function>) -> Option<SmolStr> {
        self.module.functions[func_idx]
            .params
            .first()
            .map(|param| param.name.clone())
    }

    fn body_references_variable(&self, body: &Body, name: &SmolStr) -> bool {
        body.exprs
            .iter()
            .any(|(_, expr)| matches!(expr, Expr::Variable(found) if found == name))
    }

    fn shape_trace_for_target(
        &self,
        target: &SmolStr,
        field_cache: &mut HashMap<SmolStr, GraphTraceMetadata>,
        shape_cache: &mut HashMap<SmolStr, GraphTraceMetadata>,
        visiting_fields: &mut HashSet<SmolStr>,
        visiting_shapes: &mut HashSet<SmolStr>,
    ) -> GraphTraceMetadata {
        if let Some(metadata) = shape_cache.get(target).copied() {
            return metadata;
        }
        if !visiting_shapes.insert(target.clone()) {
            return GraphTraceMetadata::pessimistic();
        }
        let metadata = self
            .module
            .shapes
            .iter()
            .find(|(_, shape)| shape.name == *target)
            .and_then(|(_, shape)| shape.graph.as_ref())
            .map(|graph| {
                self.shape_graph_trace_metadata(
                    &graph.root,
                    field_cache,
                    shape_cache,
                    visiting_fields,
                    visiting_shapes,
                )
            })
            .unwrap_or_else(GraphTraceMetadata::pessimistic);
        visiting_shapes.remove(target);
        shape_cache.insert(target.clone(), metadata);
        metadata
    }

    fn combine_trace_metadata(
        &self,
        values: impl IntoIterator<Item = GraphTraceMetadata>,
    ) -> GraphTraceMetadata {
        let mut support_unknown = false;
        let mut support_unbounded = false;
        let mut support_periodic = false;
        let mut support_bounded = false;
        let mut bounds_unknown = false;
        let mut bounds_unbounded = false;
        let mut bounds_bounded = false;
        let mut saw_value = false;
        let mut exact = true;
        let mut smooth_op_count = 0;
        let mut deform_op_count = 0;
        for value in values {
            saw_value = true;
            smooth_op_count += value.smooth_op_count;
            deform_op_count += value.deform_op_count;
            match value.support {
                FieldSupport::Unknown => support_unknown = true,
                FieldSupport::Bounded => support_bounded = true,
                FieldSupport::Periodic => support_periodic = true,
                FieldSupport::Unbounded => support_unbounded = true,
            }
            match value.bounds {
                FieldBounds::Unknown => bounds_unknown = true,
                FieldBounds::Bounded => bounds_bounded = true,
                FieldBounds::Unbounded => bounds_unbounded = true,
            }
            exact &= matches!(value.class, FieldClass::Exact);
        }
        if saw_value {
            let support = if support_unknown {
                FieldSupport::Unknown
            } else if support_unbounded {
                FieldSupport::Unbounded
            } else if support_periodic {
                FieldSupport::Periodic
            } else if support_bounded {
                FieldSupport::Bounded
            } else {
                FieldSupport::Unknown
            };
            let bounds = if bounds_unknown {
                FieldBounds::Unknown
            } else if bounds_unbounded {
                FieldBounds::Unbounded
            } else if bounds_bounded {
                FieldBounds::Bounded
            } else {
                FieldBounds::Unknown
            };
            let can_coarse_support_pruning = !matches!(support, FieldSupport::Unknown)
                && (matches!(bounds, FieldBounds::Bounded | FieldBounds::Unbounded)
                    || matches!(support, FieldSupport::Periodic));
            GraphTraceMetadata {
                class: if exact {
                    FieldClass::Exact
                } else {
                    FieldClass::Conservative
                },
                support,
                bounds,
                can_coarse_support_pruning,
                smooth_op_count,
                deform_op_count,
            }
        } else {
            GraphTraceMetadata::pessimistic()
        }
    }

    fn apply_authored_field_support(
        &self,
        trace: GraphTraceMetadata,
        field: &FieldMetadata,
    ) -> GraphTraceMetadata {
        if field.authored_support.is_none() && field.authored_bounds.is_none() {
            return trace;
        }
        match trace.support {
            FieldSupport::Unknown | FieldSupport::Bounded => GraphTraceMetadata {
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                can_coarse_support_pruning: true,
                ..trace
            },
            FieldSupport::Periodic | FieldSupport::Unbounded => trace,
        }
    }

    fn lower_func(&mut self, f: ast::FuncDef) -> Function {
        let attributes = lower_attributes(f.attributes());
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let type_params = lower_func_type_params(f.syntax());
        let params: Vec<_> = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in f.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());

        Function {
            name,
            name_span,
            attributes,
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Function,
            field: None,
            region: None,
            domain: None,
            render: None,
            field_graph: None,
            system_metadata: None,
            type_params,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_system_def(&mut self, f: ast::SystemDef) -> Function {
        let attributes = lower_attributes(f.attributes());
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let type_params = lower_func_type_params(f.syntax());
        let params = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in f.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());

        Function {
            name,
            name_span,
            attributes,
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::System,
            field: None,
            region: None,
            domain: None,
            render: None,
            field_graph: None,
            system_metadata: parse_system_metadata(f.syntax()),
            type_params,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_kernel_def(&mut self, f: ast::KernelDef) -> Function {
        let attributes = lower_attributes(f.attributes());
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let type_params = lower_func_type_params(f.syntax());
        let params = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in f.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());

        Function {
            name,
            name_span,
            attributes,
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Kernel,
            field: None,
            region: None,
            domain: None,
            render: None,
            field_graph: None,
            system_metadata: None,
            type_params,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_field_decl(&mut self, f: ast::FieldDecl) -> Function {
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let params: Vec<_> = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));
        let authored_support = f
            .support_clause()
            .and_then(|clause| clause.value())
            .map(|expr| self.lower_shape_payload(expr));
        let authored_bounds = f
            .bounds_clause()
            .and_then(|clause| clause.value())
            .map(|expr| self.lower_shape_payload(expr));
        let field = Some(FieldMetadata {
            class: match f.field_class().unwrap_or(ast::FieldClass::Exact) {
                ast::FieldClass::Exact => FieldClass::Exact,
                ast::FieldClass::Conservative => FieldClass::Conservative,
            },
            kind: match f.field_kind().unwrap_or(ast::FieldKind::Distance) {
                ast::FieldKind::Distance => FieldKind::Distance,
            },
            support: FieldSupport::Unknown,
            bounds: FieldBounds::Unknown,
            trace: GraphTraceMetadata::pessimistic(),
            authored_support,
            authored_bounds,
        });
        let semantic_expr = f.semantic_expr();
        let mut body_ctx = BodyLoweringContext::new();
        let (field_graph, body) = if let Some(expr) = semantic_expr {
            let graph = FieldGraph {
                root: self.lower_field_expr(&mut body_ctx, expr),
                trace: GraphTraceMetadata::pessimistic(),
            };
            let point_name = params
                .first()
                .map(|param| param.name.clone())
                .unwrap_or_else(|| SmolStr::new("p"));
            let span = body_ctx.empty_span();
            let point_expr = body_ctx.alloc_expr(Expr::Variable(point_name), span);
            let value = self.lower_field_graph_to_expr(&mut body_ctx, &graph.root, point_expr);
            let stmt = body_ctx.alloc_stmt(Stmt::Return(Some(value)), span);
            body_ctx.body.root_stmts.push(stmt);
            (Some(graph), body_ctx.body)
        } else {
            for stmt in f.statements() {
                let s = body_ctx.lower_stmt(stmt);
                body_ctx.body.root_stmts.push(s);
            }
            Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());
            let body = body_ctx.body;
            let field_graph = Some(FieldGraph {
                root: FieldExpr::Custom { body: body.clone() },
                trace: GraphTraceMetadata::pessimistic(),
            });
            (field_graph, body)
        };

        Function {
            name,
            name_span,
            attributes: Vec::new(),
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Field,
            field,
            region: None,
            domain: None,
            render: None,
            field_graph,
            system_metadata: None,
            type_params: Vec::new(),
            params,
            ret_type,
            body: Some(body),
        }
    }

    fn lower_region_decl(&mut self, r: ast::RegionDecl) -> Function {
        let name = r.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = r.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(r.syntax());
        let params = r.params().map(|p| self.lower_param(p)).collect();
        let items: Vec<_> = r.items().filter_map(|item| self.lower_region_item(item)).collect();
        let layers = collect_region_layers(&items);

        Function {
            name,
            name_span,
            attributes: Vec::new(),
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Region,
            field: None,
            region: Some(RegionMetadata { layers, items }),
            domain: None,
            render: None,
            field_graph: None,
            system_metadata: None,
            type_params: Vec::new(),
            params,
            ret_type: Some(TypeRef {
                name: SmolStr::new("RegionCapture"),
                name_span: None,
                args: Vec::new(),
            }),
            body: Some(Self::empty_body()),
        }
    }

    fn lower_domain_decl(&mut self, d: ast::DomainDecl) -> Function {
        let name = d.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = d.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(d.syntax());
        let params = d.params().map(|p| self.lower_param(p)).collect();
        let stmts: Vec<_> = d.statements().collect();
        let metadata = self.lower_domain_metadata(&stmts);
        let body = self.lower_world_body(stmts);

        Function {
            name,
            name_span,
            attributes: Vec::new(),
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Domain,
            field: None,
            region: None,
            domain: Some(metadata),
            render: None,
            field_graph: None,
            system_metadata: None,
            type_params: Vec::new(),
            params,
            ret_type: Some(TypeRef {
                name: SmolStr::new("SceneDomain"),
                name_span: None,
                args: Vec::new(),
            }),
            body: Some(body),
        }
    }

    fn lower_render_decl(&mut self, r: ast::RenderDecl) -> Function {
        let name = r.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = r.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(r.syntax());
        let params = r.params().map(|p| self.lower_param(p)).collect();
        let stmts: Vec<_> = r.statements().collect();
        let metadata = self.lower_render_metadata(&stmts);
        let render_ret_type = TypeRef {
            name: SmolStr::new("String"),
            name_span: None,
            args: Vec::new(),
        };
        let body = self.lower_world_body(stmts);

        Function {
            name,
            name_span,
            attributes: Vec::new(),
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Render,
            field: None,
            region: None,
            domain: None,
            render: Some(metadata),
            field_graph: None,
            system_metadata: None,
            type_params: Vec::new(),
            params,
            ret_type: Some(render_ret_type),
            body: Some(body),
        }
    }

    fn lower_radiance_decl(&mut self, f: ast::RadianceDecl) -> Function {
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let params = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in f.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());

        Function {
            name,
            name_span,
            attributes: Vec::new(),
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Radiance,
            field: None,
            region: None,
            domain: None,
            render: None,
            field_graph: None,
            system_metadata: None,
            type_params: Vec::new(),
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_volume_decl(&mut self, f: ast::VolumeDecl) -> Function {
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let params = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in f.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());

        Function {
            name,
            name_span,
            attributes: Vec::new(),
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Volume,
            field: None,
            region: None,
            domain: None,
            render: None,
            field_graph: None,
            system_metadata: None,
            type_params: Vec::new(),
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_material_decl(&mut self, f: ast::MaterialDecl) -> Function {
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let params = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in f.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());

        Function {
            name,
            name_span,
            attributes: Vec::new(),
            visibility,
            kind: FunctionKind::Function,
            role: FunctionRole::Material,
            field: None,
            region: None,
            domain: None,
            render: None,
            field_graph: None,
            system_metadata: None,
            type_params: Vec::new(),
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_region_item(&mut self, item: ast::RegionItem) -> Option<RegionItemMetadata> {
        match item {
            ast::RegionItem::Place(stmt) => self.lower_region_assignment(
                stmt.name(),
                stmt.value(),
                RegionComposeKind::Place,
            ),
            ast::RegionItem::Overlay(stmt) => self.lower_region_assignment(
                stmt.name(),
                stmt.value(),
                RegionComposeKind::Overlay,
            ),
            ast::RegionItem::Replace(stmt) => self.lower_region_assignment(
                stmt.name(),
                stmt.value(),
                RegionComposeKind::Replace,
            ),
            ast::RegionItem::Scatter(stmt) => {
                let name = stmt.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let name_span = stmt.name().map(|t| t.text_range());
                let items = stmt
                    .items()
                    .filter_map(|item| self.lower_region_item(item))
                    .collect();
                Some(RegionItemMetadata::Scatter {
                    name,
                    name_span,
                    items,
                })
            }
            ast::RegionItem::If(stmt) => {
                let condition = stmt.condition().map(|expr| self.lower_shape_payload(expr))?;
                let then_items = stmt
                    .then_block()
                    .into_iter()
                    .flat_map(|block| block.region_items().collect::<Vec<_>>())
                    .filter_map(|item| self.lower_region_item(item))
                    .collect();
                let else_items = stmt
                    .else_block()
                    .into_iter()
                    .flat_map(|block| block.region_items().collect::<Vec<_>>())
                    .filter_map(|item| self.lower_region_item(item))
                    .collect();
                Some(RegionItemMetadata::Conditional {
                    condition,
                    then_items,
                    else_items,
                })
            }
        }
    }

    fn lower_region_assignment(
        &mut self,
        name: Option<SyntaxToken>,
        value: Option<ast::Expr>,
        kind: RegionComposeKind,
    ) -> Option<RegionItemMetadata> {
        let name_token = name?;
        let value_expr = value?;
        let shape = self.lower_shape_name_expr(&value_expr);
        let name = SmolStr::new(name_token.text());
        let detail = region_detail_level_for_name(name.as_str());
        Some(RegionItemMetadata::Compose {
            kind,
            name,
            name_span: Some(name_token.text_range()),
            shape_span: Some(value_expr.syntax().text_range()),
            shape,
            detail,
        })
    }

    fn lower_domain_metadata(&mut self, stmts: &[ast::Stmt]) -> DomainMetadata {
        let mut metadata = DomainMetadata {
            geometry_detail: DomainGeometryDetail::Fine,
            material: true,
            radiance: true,
            media: true,
            max_distance: None,
            min_step: None,
            hit_epsilon: None,
            max_steps: None,
        };
        for stmt in stmts {
            let ast::Stmt::VarAssign(assign) = stmt else {
                continue;
            };
            let Some(name) = assign.name().map(|tok| SmolStr::new(tok.text())) else {
                continue;
            };
            let Some(value) = assign.value() else {
                continue;
            };
            match name.as_str() {
                "geometry" | "geometry_detail" => {
                    metadata.geometry_detail = match value.syntax().text().to_string().as_str() {
                        "coarse" => DomainGeometryDetail::Coarse,
                        _ => DomainGeometryDetail::Fine,
                    };
                }
                "material" => metadata.material = lower_bool_config_expr(&value).unwrap_or(true),
                "radiance" => metadata.radiance = lower_bool_config_expr(&value).unwrap_or(true),
                "media" => metadata.media = lower_bool_config_expr(&value).unwrap_or(true),
                "max_distance" => metadata.max_distance = Some(self.lower_shape_payload(value)),
                "min_step" => metadata.min_step = Some(self.lower_shape_payload(value)),
                "hit_epsilon" => metadata.hit_epsilon = Some(self.lower_shape_payload(value)),
                "max_steps" => metadata.max_steps = Some(self.lower_shape_payload(value)),
                _ => {}
            }
        }
        metadata
    }

    fn lower_render_metadata(&mut self, stmts: &[ast::Stmt]) -> RenderMetadata {
        let mut metadata = RenderMetadata {
            domain: None,
            light: None,
            lights: None,
            width: None,
            height: None,
            world_up: None,
            view_scale: None,
            fill_dir: None,
        };
        for stmt in stmts {
            let ast::Stmt::VarAssign(assign) = stmt else {
                continue;
            };
            let Some(name) = assign.name().map(|tok| SmolStr::new(tok.text())) else {
                continue;
            };
            let Some(value) = assign.value() else {
                continue;
            };
            match name.as_str() {
                "domain" => metadata.domain = Some(self.lower_shape_payload(value)),
                "light" => metadata.light = Some(self.lower_shape_payload(value)),
                "lights" => metadata.lights = Some(self.lower_shape_payload(value)),
                "width" => metadata.width = Some(self.lower_shape_payload(value)),
                "height" => metadata.height = Some(self.lower_shape_payload(value)),
                "world_up" => metadata.world_up = Some(self.lower_shape_payload(value)),
                "view_scale" => metadata.view_scale = Some(self.lower_shape_payload(value)),
                "fill_dir" => metadata.fill_dir = Some(self.lower_shape_payload(value)),
                _ => {}
            }
        }
        metadata
    }

    fn lower_world_body(&mut self, stmts: Vec<ast::Stmt>) -> Body {
        let mut body_ctx = BodyLoweringContext::new();
        for stmt in stmts {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        body_ctx.body
    }

    fn lower_shape_decl(&mut self, s: ast::ShapeDecl) -> Shape {
        let name = s.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = s.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(s.syntax());
        let graph = s.semantic_expr().map(|expr| {
            let mut body_ctx = BodyLoweringContext::new();
            let mut feature_path = vec![name.clone()];
            let provenance = self.lower_shape_provenance_expr(&expr);
            ShapeGraph {
                root: self.lower_shape_expr(&mut body_ctx, expr, &mut feature_path),
                provenance,
                trace: GraphTraceMetadata::pessimistic(),
            }
        });

        Shape {
            name,
            name_span,
            visibility,
            graph,
        }
    }

    fn lower_shape_expr(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        expr: ast::ShapeExpr,
        feature_path: &mut Vec<SmolStr>,
    ) -> ShapeExpr {
        match expr {
            ast::ShapeExpr::Use(use_expr) => ShapeExpr::Use {
                target: use_expr
                    .name()
                    .map(|tok| SmolStr::new(tok.text()))
                    .unwrap_or_default(),
            },
            ast::ShapeExpr::Union(union_expr) => ShapeExpr::Union {
                items: union_expr
                    .items()
                    .enumerate()
                    .map(|(idx, item)| {
                        feature_path.push(SmolStr::new(format!("union[{idx}]")));
                        let lowered = self.lower_shape_expr(body_ctx, item, feature_path);
                        feature_path.pop();
                        lowered
                    })
                    .collect(),
            },
            ast::ShapeExpr::Intersection(intersection_expr) => ShapeExpr::Intersection {
                items: intersection_expr
                    .items()
                    .enumerate()
                    .map(|(idx, item)| {
                        feature_path.push(SmolStr::new(format!("intersection[{idx}]")));
                        let lowered = self.lower_shape_expr(body_ctx, item, feature_path);
                        feature_path.pop();
                        lowered
                    })
                    .collect(),
            },
            ast::ShapeExpr::Subtract(subtract_expr) => {
                let mut items = subtract_expr.items();
                let left = items
                    .next()
                    .map(|item| {
                        feature_path.push(SmolStr::new("subtract[left]"));
                        let lowered = self.lower_shape_expr(body_ctx, item, feature_path);
                        feature_path.pop();
                        lowered
                    })
                    .unwrap_or_else(|| ShapeExpr::Use {
                        target: SmolStr::new(""),
                    });
                let right = items
                    .next()
                    .map(|item| {
                        feature_path.push(SmolStr::new("subtract[right]"));
                        let lowered = self.lower_shape_expr(body_ctx, item, feature_path);
                        feature_path.pop();
                        lowered
                    })
                    .unwrap_or_else(|| ShapeExpr::Use {
                        target: SmolStr::new(""),
                    });
                ShapeExpr::Subtract {
                    left: Box::new(left),
                    right: Box::new(right),
                }
            }
            ast::ShapeExpr::Leaf(leaf_expr) => {
                let field = leaf_expr
                    .field()
                    .and_then(|binding| binding.value())
                    .map(|expr| self.lower_shape_name_expr(&expr))
                    .unwrap_or_default();
                let material = leaf_expr
                    .material()
                    .and_then(|binding| binding.value())
                    .map(|expr| self.lower_shape_name_expr(&expr))
                    .unwrap_or_default();
                let radiance = leaf_expr
                    .radiance()
                    .and_then(|binding| binding.value())
                    .map(|expr| self.lower_shape_name_expr(&expr));
                let volume = leaf_expr
                    .volume()
                    .and_then(|binding| binding.value())
                    .map(|expr| self.lower_shape_name_expr(&expr));
                let payload = leaf_expr
                    .payload()
                    .and_then(|binding| binding.value())
                    .map(|expr| self.lower_shape_payload(expr))
                    .unwrap_or_else(Self::empty_body);
                let feature_id = Self::shape_feature_id(feature_path);

                ShapeExpr::Leaf(ShapeLeaf {
                    field,
                    material,
                    radiance,
                    volume,
                    payload,
                    feature_id,
                })
            }
        }
    }

    fn lower_shape_provenance_expr(&self, expr: &ast::ShapeExpr) -> Option<ShapeProvenanceExpr> {
        match expr {
            ast::ShapeExpr::Use(use_expr) => Some(ShapeProvenanceExpr::Use {
                target: use_expr
                    .name()
                    .map(|tok| SmolStr::new(tok.text()))
                    .unwrap_or_default(),
            }),
            ast::ShapeExpr::Union(union_expr) => Some(ShapeProvenanceExpr::Union {
                provenance: Self::lower_shape_merge_provenance_policy(
                    union_expr.provenance_policy(),
                ),
                items: union_expr
                    .items()
                    .filter_map(|item| self.lower_shape_provenance_expr(&item))
                    .collect(),
            }),
            ast::ShapeExpr::Intersection(intersection_expr) => {
                Some(ShapeProvenanceExpr::Intersection {
                    provenance: Self::lower_shape_merge_provenance_policy(
                        intersection_expr.provenance_policy(),
                    ),
                    items: intersection_expr
                        .items()
                        .filter_map(|item| self.lower_shape_provenance_expr(&item))
                        .collect(),
                })
            }
            ast::ShapeExpr::Subtract(subtract_expr) => {
                let mut items = subtract_expr.items();
                let left = items
                    .next()
                    .and_then(|item| self.lower_shape_provenance_expr(&item))
                    .unwrap_or(ShapeProvenanceExpr::Leaf);
                let right = items
                    .next()
                    .and_then(|item| self.lower_shape_provenance_expr(&item))
                    .unwrap_or(ShapeProvenanceExpr::Leaf);
                Some(ShapeProvenanceExpr::Subtract {
                    provenance: Self::lower_shape_subtract_provenance_policy(
                        subtract_expr.provenance_policy(),
                    ),
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            ast::ShapeExpr::Leaf(_) => Some(ShapeProvenanceExpr::Leaf),
        }
    }

    fn shape_feature_id(path: &[SmolStr]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;

        let mut hash = FNV_OFFSET;
        for part in path {
            for byte in part.as_bytes() {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= b'/' as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash & (i64::MAX as u64)
    }

    fn lower_shape_merge_provenance_policy(
        policy: Option<SyntaxToken>,
    ) -> ShapeMergeProvenancePolicy {
        match policy.as_ref().map(|token| token.text()) {
            Some("ordered") => ShapeMergeProvenancePolicy::Ordered,
            _ => ShapeMergeProvenancePolicy::Nearest,
        }
    }

    fn lower_shape_subtract_provenance_policy(
        policy: Option<SyntaxToken>,
    ) -> ShapeSubtractProvenancePolicy {
        match policy.as_ref().map(|token| token.text()) {
            Some("right") => ShapeSubtractProvenancePolicy::Right,
            _ => ShapeSubtractProvenancePolicy::Left,
        }
    }

    fn lower_shape_name_expr(&self, expr: &ast::Expr) -> SmolStr {
        match expr {
            ast::Expr::Ident(ident) => ident
                .name()
                .map(|tok| SmolStr::new(tok.text()))
                .unwrap_or_default(),
            _ => SmolStr::new(expr.syntax().text().to_string()),
        }
    }

    fn lower_shape_payload(&mut self, expr: ast::Expr) -> Body {
        let mut body_ctx = BodyLoweringContext::new();
        let value = match body_ctx.lower_expr(expr) {
            Some(value) => value,
            None => body_ctx.alloc_expr(Expr::Literal(Literal::Nil), body_ctx.empty_span()),
        };
        let span = body_ctx.empty_span();
        let stmt = body_ctx.alloc_stmt(Stmt::Expr(value), span);
        body_ctx.body.root_stmts.push(stmt);
        body_ctx.body
    }

    fn lower_wrapped_field_body(&mut self, expr: ast::Expr) -> Body {
        self.lower_shape_payload(expr)
    }

    fn wrapped_body_value_expr(body: &Body) -> Option<Idx<Expr>> {
        let stmt = *body.root_stmts.last()?;
        match &body.stmts[stmt] {
            Stmt::Expr(expr) => Some(*expr),
            Stmt::Return(Some(expr)) => Some(*expr),
            _ => None,
        }
    }

    fn clone_wrapped_body_expr(
        &mut self,
        source: &Body,
        expr_id: Idx<Expr>,
        body_ctx: &mut BodyLoweringContext,
    ) -> Idx<Expr> {
        let span = source.expr_span(expr_id);
        let cloned = match &source.exprs[expr_id] {
            Expr::Literal(literal) => Expr::Literal(literal.clone()),
            Expr::Variable(name) => Expr::Variable(name.clone()),
            Expr::Detach {
                target,
                size,
                objective,
            } => Expr::Detach {
                target: self.clone_wrapped_body_expr(source, *target, body_ctx),
                size: *size,
                objective: *objective,
            },
            Expr::Binary {
                lhs,
                op,
                rhs,
                op_span,
            } => Expr::Binary {
                lhs: self.clone_wrapped_body_expr(source, *lhs, body_ctx),
                op: *op,
                rhs: self.clone_wrapped_body_expr(source, *rhs, body_ctx),
                op_span: *op_span,
            },
            Expr::Unary { op, expr, op_span } => Expr::Unary {
                op: *op,
                expr: self.clone_wrapped_body_expr(source, *expr, body_ctx),
                op_span: *op_span,
            },
            Expr::TypeApply { callee, type_args } => Expr::TypeApply {
                callee: self.clone_wrapped_body_expr(source, *callee, body_ctx),
                type_args: type_args.clone(),
            },
            Expr::Crash { expr } => Expr::Crash {
                expr: self.clone_wrapped_body_expr(source, *expr, body_ctx),
            },
            Expr::Call {
                callee,
                args,
                type_args,
            } => Expr::Call {
                callee: self.clone_wrapped_body_expr(source, *callee, body_ctx),
                args: args
                    .iter()
                    .map(|arg| match arg {
                        Arg::Positional { value, span } => Arg::Positional {
                            value: self.clone_wrapped_body_expr(source, *value, body_ctx),
                            span: *span,
                        },
                        Arg::Named {
                            name,
                            value,
                            span,
                            name_span,
                        } => Arg::Named {
                            name: name.clone(),
                            value: self.clone_wrapped_body_expr(source, *value, body_ctx),
                            span: *span,
                            name_span: *name_span,
                        },
                    })
                    .collect(),
                type_args: type_args.clone(),
            },
            Expr::Member {
                object,
                member,
                member_span,
            } => Expr::Member {
                object: self.clone_wrapped_body_expr(source, *object, body_ctx),
                member: member.clone(),
                member_span: *member_span,
            },
            Expr::Index {
                object,
                index,
                index_span,
            } => Expr::Index {
                object: self.clone_wrapped_body_expr(source, *object, body_ctx),
                index: self.clone_wrapped_body_expr(source, *index, body_ctx),
                index_span: *index_span,
            },
            Expr::List(items) => Expr::List(
                items
                    .iter()
                    .map(|item| self.clone_wrapped_body_expr(source, *item, body_ctx))
                    .collect(),
            ),
            Expr::Map(items) => Expr::Map(
                items
                    .iter()
                    .map(|(key, value)| {
                        (
                            self.clone_wrapped_body_expr(source, *key, body_ctx),
                            self.clone_wrapped_body_expr(source, *value, body_ctx),
                        )
                    })
                    .collect(),
            ),
            Expr::StringInterp(parts) => Expr::StringInterp(
                parts
                    .iter()
                    .map(|part| match part {
                        StringPart::Literal(text) => StringPart::Literal(text.clone()),
                        StringPart::Expr(expr) => {
                            StringPart::Expr(self.clone_wrapped_body_expr(source, *expr, body_ctx))
                        }
                    })
                    .collect(),
            ),
            Expr::Closure { params, body } => Expr::Closure {
                params: params.clone(),
                body: self.clone_wrapped_body_expr(source, *body, body_ctx),
            },
        };
        body_ctx.alloc_expr(cloned, span)
    }

    fn lower_field_wrapper_point(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        helper_name: &str,
        wrapper_param: &str,
        wrapper_body: &Body,
        point_expr: Idx<Expr>,
    ) -> Idx<Expr> {
        let Some(wrapper_expr) = Self::wrapped_body_value_expr(wrapper_body) else {
            return point_expr;
        };
        let span = body_ctx.empty_span();
        let wrapper_value = self.clone_wrapped_body_expr(wrapper_body, wrapper_expr, body_ctx);
        let callee = body_ctx.alloc_expr(Expr::Variable(SmolStr::new(helper_name)), span);
        body_ctx.alloc_expr(
            Expr::Call {
                callee,
                type_args: Vec::new(),
                args: vec![
                    Arg::Named {
                        name: SmolStr::new(wrapper_param),
                        value: wrapper_value,
                        span,
                        name_span: span,
                    },
                    Arg::Named {
                        name: SmolStr::new("point"),
                        value: point_expr,
                        span,
                        name_span: span,
                    },
                ],
            },
            span,
        )
    }

    fn empty_body() -> Body {
        Body {
            exprs: Arena::new(),
            stmts: Arena::new(),
            root_stmts: Vec::new(),
            expr_spans: Vec::new(),
            stmt_spans: Vec::new(),
        }
    }

    fn lower_field_expr(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        expr: ast::FieldExpr,
    ) -> FieldExpr {
        match expr {
            ast::FieldExpr::Use(use_expr) => FieldExpr::Use {
                target: use_expr
                    .name()
                    .map(|tok| SmolStr::new(tok.text()))
                    .unwrap_or_default(),
            },
            ast::FieldExpr::Primitive(primitive_expr) => {
                let primitive_name = primitive_expr
                    .name()
                    .map(|tok| tok.text().to_string())
                    .unwrap_or_default();
                if Self::is_profile_primitive_name(&primitive_name) {
                    return FieldExpr::Custom {
                        body: Self::empty_body(),
                    };
                }
                let primitive = Self::field_primitive_from_name(&primitive_name)
                    .unwrap_or(FieldPrimitive::Sphere);
                let args = primitive_expr
                    .args()
                    .filter_map(|arg| body_ctx.lower_arg(arg))
                    .collect();
                FieldExpr::Primitive { primitive, args }
            }
            ast::FieldExpr::Union(union_expr) => FieldExpr::Union {
                items: union_expr
                    .items()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .collect(),
            },
            ast::FieldExpr::Intersection(intersection_expr) => FieldExpr::Intersection {
                items: intersection_expr
                    .items()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .collect(),
            },
            ast::FieldExpr::Subtract(subtract_expr) => {
                let mut items = subtract_expr.items();
                let left = items
                    .next()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Body {
                            exprs: Arena::new(),
                            stmts: Arena::new(),
                            root_stmts: Vec::new(),
                            expr_spans: Vec::new(),
                            stmt_spans: Vec::new(),
                        },
                    });
                let right = items
                    .next()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Body {
                            exprs: Arena::new(),
                            stmts: Arena::new(),
                            root_stmts: Vec::new(),
                            expr_spans: Vec::new(),
                            stmt_spans: Vec::new(),
                        },
                    });
                FieldExpr::Subtract {
                    left: Box::new(left),
                    right: Box::new(right),
                }
            }
            ast::FieldExpr::SmoothUnion(expr) => FieldExpr::SmoothUnion {
                smoothing: expr
                    .smoothing()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body),
                items: expr
                    .items()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .collect(),
            },
            ast::FieldExpr::SmoothIntersection(expr) => FieldExpr::SmoothIntersection {
                smoothing: expr
                    .smoothing()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body),
                items: expr
                    .items()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .collect(),
            },
            ast::FieldExpr::SmoothSubtract(expr) => {
                let mut items = expr.items();
                let left = items
                    .next()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                let right = items
                    .next()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::SmoothSubtract {
                    smoothing: expr
                        .smoothing()
                        .map(|value| self.lower_wrapped_field_body(value))
                        .unwrap_or_else(Self::empty_body),
                    left: Box::new(left),
                    right: Box::new(right),
                }
            }
            ast::FieldExpr::Translate(expr) => {
                let translate = expr
                    .translate()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::Translate {
                    translate,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::Rotate(expr) => {
                let rotate = expr
                    .rotate()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::Rotate {
                    rotate,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::UniformScale(expr) => {
                let scale = expr
                    .uniform_scale()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::UniformScale {
                    scale,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::AffineTransform(expr) => {
                let transform = expr
                    .affine_transform()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::AffineTransform {
                    transform,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::Warp(expr) => {
                let warp = expr
                    .warp()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::Warp {
                    warp,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::RepeatLinear(expr) => {
                let repeat = expr
                    .repeat_linear()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::RepeatLinear {
                    repeat,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::RepeatGrid(expr) => {
                let repeat = expr
                    .repeat_grid()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::RepeatGrid {
                    repeat,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::RadialRepeat(expr) => {
                let radial = expr
                    .radial_repeat()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::RadialRepeat {
                    radial,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::MirrorArray(expr) => {
                let mirror = expr
                    .mirror_array()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::MirrorArray {
                    mirror,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::InstanceArray(expr) => {
                let instance = expr
                    .instance_array()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::InstanceArray {
                    instance,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::Bend(expr) => {
                let bend = expr
                    .bend()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::Bend {
                    bend,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::Twist(expr) => {
                let twist = expr
                    .twist()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::Twist {
                    twist,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::Taper(expr) => {
                let taper = expr
                    .taper()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::Taper {
                    taper,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::Displace(expr) => {
                let displace = expr
                    .displace()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body);
                let body = expr
                    .body()
                    .map(|item| self.lower_field_expr(body_ctx, item))
                    .unwrap_or(FieldExpr::Custom {
                        body: Self::empty_body(),
                    });
                FieldExpr::Displace {
                    displace,
                    body: Box::new(body),
                }
            }
            ast::FieldExpr::Extrude(expr) => FieldExpr::Extrude {
                height: expr
                    .height()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body),
                profile: expr
                    .profile()
                    .map(|profile| self.lower_profile_expr(body_ctx, profile))
                    .unwrap_or(ProfileExpr::Primitive {
                        primitive: ProfilePrimitive::Circle2,
                        args: Vec::new(),
                    }),
            },
            ast::FieldExpr::Revolve(expr) => FieldExpr::Revolve {
                profile: expr
                    .profile()
                    .map(|profile| self.lower_profile_expr(body_ctx, profile))
                    .unwrap_or(ProfileExpr::Primitive {
                        primitive: ProfilePrimitive::Circle2,
                        args: Vec::new(),
                    }),
            },
            ast::FieldExpr::Sweep(expr) => FieldExpr::Sweep {
                path: expr
                    .path()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body),
                profile: expr
                    .profile()
                    .map(|profile| self.lower_profile_expr(body_ctx, profile))
                    .unwrap_or(ProfileExpr::Primitive {
                        primitive: ProfilePrimitive::Circle2,
                        args: Vec::new(),
                    }),
            },
            ast::FieldExpr::Loft(expr) => FieldExpr::Loft {
                height: expr
                    .height()
                    .map(|value| self.lower_wrapped_field_body(value))
                    .unwrap_or_else(Self::empty_body),
                from: expr
                    .from_profile()
                    .map(|profile| self.lower_profile_expr(body_ctx, profile))
                    .unwrap_or(ProfileExpr::Primitive {
                        primitive: ProfilePrimitive::Circle2,
                        args: Vec::new(),
                    }),
                to: expr
                    .to_profile()
                    .map(|profile| self.lower_profile_expr(body_ctx, profile))
                    .unwrap_or(ProfileExpr::Primitive {
                        primitive: ProfilePrimitive::Circle2,
                        args: Vec::new(),
                    }),
            },
        }
    }

    fn lower_profile_expr(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        expr: ast::ProfileExpr,
    ) -> ProfileExpr {
        match expr {
            ast::ProfileExpr::Primitive(primitive_expr) => {
                let primitive_name = primitive_expr
                    .name()
                    .map(|tok| tok.text().to_string())
                    .unwrap_or_default();
                let primitive = Self::profile_primitive_from_name(&primitive_name)
                    .unwrap_or(ProfilePrimitive::Circle2);
                let args = primitive_expr
                    .args()
                    .filter_map(|arg| body_ctx.lower_arg(arg))
                    .collect();
                ProfileExpr::Primitive { primitive, args }
            }
        }
    }

    fn lower_field_graph_to_expr(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        expr: &FieldExpr,
        point_expr: Idx<Expr>,
    ) -> Idx<Expr> {
        let span = body_ctx.empty_span();
        match expr {
            FieldExpr::Use { target } => {
                let callee = body_ctx.alloc_expr(Expr::Variable(target.clone()), span);
                body_ctx.alloc_expr(
                    Expr::Call {
                        callee,
                        type_args: Vec::new(),
                        args: vec![Arg::Named {
                            name: SmolStr::new("p"),
                            value: point_expr,
                            span,
                            name_span: span,
                        }],
                    },
                    span,
                )
            }
            FieldExpr::Primitive { primitive, args } => {
                let callee = body_ctx.alloc_expr(
                    Expr::Variable(SmolStr::new(Self::primitive_callee_name(*primitive))),
                    span,
                );
                let mut call_args = vec![Arg::Named {
                    name: SmolStr::new("p"),
                    value: point_expr,
                    span,
                    name_span: span,
                }];
                call_args.extend(args.iter().cloned().filter(|arg| match arg {
                    Arg::Named { name, .. } => name.as_str() != "p",
                    _ => true,
                }));
                body_ctx.alloc_expr(
                    Expr::Call {
                        callee,
                        type_args: Vec::new(),
                        args: call_args,
                    },
                    span,
                )
            }
            FieldExpr::Union { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return body_ctx.alloc_expr(Expr::Literal(Literal::Nil), span);
                };
                let mut current = self.lower_field_graph_to_expr(body_ctx, first, point_expr);
                for item in iter {
                    let rhs = self.lower_field_graph_to_expr(body_ctx, item, point_expr);
                    let callee =
                        body_ctx.alloc_expr(Expr::Variable(SmolStr::new("field_union")), span);
                    current = body_ctx.alloc_expr(
                        Expr::Call {
                            callee,
                            type_args: Vec::new(),
                            args: vec![
                                Arg::Named {
                                    name: SmolStr::new("left"),
                                    value: current,
                                    span,
                                    name_span: span,
                                },
                                Arg::Named {
                                    name: SmolStr::new("right"),
                                    value: rhs,
                                    span,
                                    name_span: span,
                                },
                            ],
                        },
                        span,
                    );
                }
                current
            }
            FieldExpr::Intersection { items } => {
                let mut iter = items.iter();
                let Some(first) = iter.next() else {
                    return body_ctx.alloc_expr(Expr::Literal(Literal::Nil), span);
                };
                let mut current = self.lower_field_graph_to_expr(body_ctx, first, point_expr);
                for item in iter {
                    let rhs = self.lower_field_graph_to_expr(body_ctx, item, point_expr);
                    let callee = body_ctx
                        .alloc_expr(Expr::Variable(SmolStr::new("field_intersection")), span);
                    current = body_ctx.alloc_expr(
                        Expr::Call {
                            callee,
                            type_args: Vec::new(),
                            args: vec![
                                Arg::Named {
                                    name: SmolStr::new("left"),
                                    value: current,
                                    span,
                                    name_span: span,
                                },
                                Arg::Named {
                                    name: SmolStr::new("right"),
                                    value: rhs,
                                    span,
                                    name_span: span,
                                },
                            ],
                        },
                        span,
                    );
                }
                current
            }
            FieldExpr::Subtract { left, right } => {
                let left = self.lower_field_graph_to_expr(body_ctx, left, point_expr);
                let right = self.lower_field_graph_to_expr(body_ctx, right, point_expr);
                let callee =
                    body_ctx.alloc_expr(Expr::Variable(SmolStr::new("field_subtract")), span);
                body_ctx.alloc_expr(
                    Expr::Call {
                        callee,
                        type_args: Vec::new(),
                        args: vec![
                            Arg::Named {
                                name: SmolStr::new("left"),
                                value: left,
                                span,
                                name_span: span,
                            },
                            Arg::Named {
                                name: SmolStr::new("right"),
                                value: right,
                                span,
                                name_span: span,
                            },
                        ],
                    },
                    span,
                )
            }
            FieldExpr::Translate { translate, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_translate_point",
                    "translate",
                    translate,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::Rotate { rotate, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_rotate_point",
                    "rotate",
                    rotate,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::UniformScale { scale, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_uniform_scale_point",
                    "scale",
                    scale,
                    point_expr,
                );
                let scaled = self.lower_field_graph_to_expr(body_ctx, body, local_point);
                let scale_expr =
                    Self::wrapped_body_value_expr(scale).expect("uniform scale expression");
                let scale_value = self.clone_wrapped_body_expr(scale, scale_expr, body_ctx);
                let callee = body_ctx.alloc_expr(Expr::Variable(SmolStr::new("abs")), span);
                let abs_scale = body_ctx.alloc_expr(
                    Expr::Call {
                        callee,
                        type_args: Vec::new(),
                        args: vec![Arg::Named {
                            name: SmolStr::new("value"),
                            value: scale_value,
                            span,
                            name_span: span,
                        }],
                    },
                    span,
                );
                body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: scaled,
                        op: BinaryOp::Mul,
                        rhs: abs_scale,
                        op_span: span,
                    },
                    span,
                )
            }
            FieldExpr::AffineTransform { transform, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_affine_transform_point",
                    "transform",
                    transform,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::Warp { warp, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_warp_point",
                    "warp",
                    warp,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::RepeatLinear { repeat, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_repeat_linear_point",
                    "repeat",
                    repeat,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::RepeatGrid { repeat, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_repeat_grid_point",
                    "repeat",
                    repeat,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::RadialRepeat { radial, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_radial_repeat_point",
                    "radial",
                    radial,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::MirrorArray { mirror, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_mirror_array_point",
                    "mirror",
                    mirror,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::InstanceArray { instance, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_instance_array_point",
                    "instance",
                    instance,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::SmoothUnion { smoothing, items } => {
                let Some(first) = items.first() else {
                    return body_ctx.alloc_expr(Expr::Literal(Literal::Nil), span);
                };
                let smoothing_expr = Self::wrapped_body_value_expr(smoothing)
                    .expect("smooth union smoothing expression");
                let smoothing_value =
                    self.clone_wrapped_body_expr(smoothing, smoothing_expr, body_ctx);
                let mut current = self.lower_field_graph_to_expr(body_ctx, first, point_expr);
                for item in items.iter().skip(1) {
                    let rhs = self.lower_field_graph_to_expr(body_ctx, item, point_expr);
                    let callee = body_ctx
                        .alloc_expr(Expr::Variable(SmolStr::new("field_smooth_union")), span);
                    current = body_ctx.alloc_expr(
                        Expr::Call {
                            callee,
                            type_args: Vec::new(),
                            args: vec![
                                Arg::Named {
                                    name: SmolStr::new("smoothing"),
                                    value: smoothing_value,
                                    span,
                                    name_span: span,
                                },
                                Arg::Named {
                                    name: SmolStr::new("left"),
                                    value: current,
                                    span,
                                    name_span: span,
                                },
                                Arg::Named {
                                    name: SmolStr::new("right"),
                                    value: rhs,
                                    span,
                                    name_span: span,
                                },
                            ],
                        },
                        span,
                    );
                }
                current
            }
            FieldExpr::SmoothIntersection { smoothing, items } => {
                let Some(first) = items.first() else {
                    return body_ctx.alloc_expr(Expr::Literal(Literal::Nil), span);
                };
                let smoothing_expr = Self::wrapped_body_value_expr(smoothing)
                    .expect("smooth intersection smoothing expression");
                let smoothing_value =
                    self.clone_wrapped_body_expr(smoothing, smoothing_expr, body_ctx);
                let mut current = self.lower_field_graph_to_expr(body_ctx, first, point_expr);
                for item in items.iter().skip(1) {
                    let rhs = self.lower_field_graph_to_expr(body_ctx, item, point_expr);
                    let callee = body_ctx.alloc_expr(
                        Expr::Variable(SmolStr::new("field_smooth_intersection")),
                        span,
                    );
                    current = body_ctx.alloc_expr(
                        Expr::Call {
                            callee,
                            type_args: Vec::new(),
                            args: vec![
                                Arg::Named {
                                    name: SmolStr::new("smoothing"),
                                    value: smoothing_value,
                                    span,
                                    name_span: span,
                                },
                                Arg::Named {
                                    name: SmolStr::new("left"),
                                    value: current,
                                    span,
                                    name_span: span,
                                },
                                Arg::Named {
                                    name: SmolStr::new("right"),
                                    value: rhs,
                                    span,
                                    name_span: span,
                                },
                            ],
                        },
                        span,
                    );
                }
                current
            }
            FieldExpr::SmoothSubtract {
                smoothing,
                left,
                right,
            } => {
                let smoothing_expr = Self::wrapped_body_value_expr(smoothing)
                    .expect("smooth subtract smoothing expression");
                let smoothing_value =
                    self.clone_wrapped_body_expr(smoothing, smoothing_expr, body_ctx);
                let left = self.lower_field_graph_to_expr(body_ctx, left, point_expr);
                let right = self.lower_field_graph_to_expr(body_ctx, right, point_expr);
                let callee = body_ctx
                    .alloc_expr(Expr::Variable(SmolStr::new("field_smooth_subtract")), span);
                body_ctx.alloc_expr(
                    Expr::Call {
                        callee,
                        type_args: Vec::new(),
                        args: vec![
                            Arg::Named {
                                name: SmolStr::new("smoothing"),
                                value: smoothing_value,
                                span,
                                name_span: span,
                            },
                            Arg::Named {
                                name: SmolStr::new("left"),
                                value: left,
                                span,
                                name_span: span,
                            },
                            Arg::Named {
                                name: SmolStr::new("right"),
                                value: right,
                                span,
                                name_span: span,
                            },
                        ],
                    },
                    span,
                )
            }
            FieldExpr::Bend { bend, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_bend_point",
                    "bend",
                    bend,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::Twist { twist, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_twist_point",
                    "twist",
                    twist,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::Taper { taper, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_taper_point",
                    "taper",
                    taper,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::Displace { displace, body } => {
                let local_point = self.lower_field_wrapper_point(
                    body_ctx,
                    "field_displace_point",
                    "displace",
                    displace,
                    point_expr,
                );
                self.lower_field_graph_to_expr(body_ctx, body, local_point)
            }
            FieldExpr::Extrude { height, profile } => {
                let height_expr =
                    Self::wrapped_body_value_expr(height).expect("extrude height expression");
                let height_value = self.clone_wrapped_body_expr(height, height_expr, body_ctx);
                let y = self.lower_member_expr(body_ctx, point_expr, "y");
                let point_x = self.lower_member_expr(body_ctx, point_expr, "x");
                let point_z = self.lower_member_expr(body_ctx, point_expr, "z");
                let profile_point = self.lower_vec2_expr(body_ctx, point_x, point_z);
                let profile_distance =
                    self.lower_profile_expr_to_distance(body_ctx, profile, profile_point);
                let abs_height = self.lower_scalar_call(body_ctx, "abs", "value", height_value);
                let half_height_value = self.lower_float_literal(body_ctx, 0.5);
                let half_height = body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: abs_height,
                        op: BinaryOp::Mul,
                        rhs: half_height_value,
                        op_span: span,
                    },
                    span,
                );
                let abs_y = self.lower_scalar_call(body_ctx, "abs", "value", y);
                let axial = body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: abs_y,
                        op: BinaryOp::Sub,
                        rhs: half_height,
                        op_span: span,
                    },
                    span,
                );
                self.lower_profile_cap_distance(body_ctx, profile_distance, axial)
            }
            FieldExpr::Revolve { profile } => {
                let point_x = self.lower_member_expr(body_ctx, point_expr, "x");
                let point_z = self.lower_member_expr(body_ctx, point_expr, "z");
                let radial_point = self.lower_vec2_expr(body_ctx, point_x, point_z);
                let radial = self.lower_scalar_call(body_ctx, "length", "value", radial_point);
                let point_y = self.lower_member_expr(body_ctx, point_expr, "y");
                let profile_point = self.lower_vec2_expr(body_ctx, radial, point_y);
                self.lower_profile_expr_to_distance(body_ctx, profile, profile_point)
            }
            FieldExpr::Sweep { path, profile } => {
                let path_expr = Self::wrapped_body_value_expr(path).expect("sweep path expression");
                let path_value = self.clone_wrapped_body_expr(path, path_expr, body_ctx);
                let coords = self.lower_named_call_expr(
                    body_ctx,
                    "field_sweep_coords",
                    vec![("path", path_value), ("point", point_expr)],
                );
                let coords_x = self.lower_member_expr(body_ctx, coords, "x");
                let coords_y = self.lower_member_expr(body_ctx, coords, "y");
                let profile_point = self.lower_vec2_expr(body_ctx, coords_x, coords_y);
                let profile_distance =
                    self.lower_profile_expr_to_distance(body_ctx, profile, profile_point);
                let coords_z = self.lower_member_expr(body_ctx, coords, "z");
                let path_length = self.lower_scalar_call(body_ctx, "length", "value", path_value);
                let half_factor = self.lower_float_literal(body_ctx, 0.5);
                let half_length = body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: path_length,
                        op: BinaryOp::Mul,
                        rhs: half_factor,
                        op_span: span,
                    },
                    span,
                );
                let abs_z = self.lower_scalar_call(body_ctx, "abs", "value", coords_z);
                let axial = body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: abs_z,
                        op: BinaryOp::Sub,
                        rhs: half_length,
                        op_span: span,
                    },
                    span,
                );
                self.lower_profile_cap_distance(body_ctx, profile_distance, axial)
            }
            FieldExpr::Loft { height, from, to } => {
                let height_expr =
                    Self::wrapped_body_value_expr(height).expect("loft height expression");
                let height_value = self.clone_wrapped_body_expr(height, height_expr, body_ctx);
                let abs_height = self.lower_scalar_call(body_ctx, "abs", "value", height_value);
                let half_height_factor = self.lower_float_literal(body_ctx, 0.5);
                let half_height = body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: abs_height,
                        op: BinaryOp::Mul,
                        rhs: half_height_factor,
                        op_span: span,
                    },
                    span,
                );
                let min_safe_height = self.lower_float_literal(body_ctx, 0.0001);
                let safe_height = self.lower_binary_call(
                    body_ctx,
                    "max",
                    ("left", abs_height),
                    ("right", min_safe_height),
                );
                let y = self.lower_member_expr(body_ctx, point_expr, "y");
                let point_x = self.lower_member_expr(body_ctx, point_expr, "x");
                let point_z = self.lower_member_expr(body_ctx, point_expr, "z");
                let profile_point = self.lower_vec2_expr(body_ctx, point_x, point_z);
                let from_distance =
                    self.lower_profile_expr_to_distance(body_ctx, from, profile_point);
                let to_distance = self.lower_profile_expr_to_distance(body_ctx, to, profile_point);
                let y_plus_half = body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: y,
                        op: BinaryOp::Add,
                        rhs: half_height,
                        op_span: span,
                    },
                    span,
                );
                let unclamped_t = body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: y_plus_half,
                        op: BinaryOp::Div,
                        rhs: safe_height,
                        op_span: span,
                    },
                    span,
                );
                let t_min = self.lower_float_literal(body_ctx, 0.0);
                let t_max = self.lower_float_literal(body_ctx, 1.0);
                let t = self.lower_ternary_call(
                    body_ctx,
                    "clamp",
                    ("value", unclamped_t),
                    ("min", t_min),
                    ("max", t_max),
                );
                let mixed = self.lower_ternary_call(
                    body_ctx,
                    "mix",
                    ("value", from_distance),
                    ("other", to_distance),
                    ("t", t),
                );
                let abs_y = self.lower_scalar_call(body_ctx, "abs", "value", y);
                let axial = body_ctx.alloc_expr(
                    Expr::Binary {
                        lhs: abs_y,
                        op: BinaryOp::Sub,
                        rhs: half_height,
                        op_span: span,
                    },
                    span,
                );
                self.lower_profile_cap_distance(body_ctx, mixed, axial)
            }
            FieldExpr::Custom { body } => {
                let _ = body;
                body_ctx.alloc_expr(Expr::Literal(Literal::Nil), span)
            }
        }
    }

    fn lower_profile_expr_to_distance(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        expr: &ProfileExpr,
        point_expr: Idx<Expr>,
    ) -> Idx<Expr> {
        match expr {
            ProfileExpr::Primitive { primitive, args } => {
                let callee = body_ctx.alloc_expr(
                    Expr::Variable(SmolStr::new(Self::profile_primitive_callee_name(
                        *primitive,
                    ))),
                    body_ctx.empty_span(),
                );
                let mut call_args = vec![Arg::Named {
                    name: SmolStr::new("p"),
                    value: point_expr,
                    span: body_ctx.empty_span(),
                    name_span: body_ctx.empty_span(),
                }];
                call_args.extend(args.iter().cloned().filter(|arg| match arg {
                    Arg::Named { name, .. } => name.as_str() != "p",
                    _ => true,
                }));
                body_ctx.alloc_expr(
                    Expr::Call {
                        callee,
                        type_args: Vec::new(),
                        args: call_args,
                    },
                    body_ctx.empty_span(),
                )
            }
        }
    }

    fn lower_profile_cap_distance(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        profile_distance: Idx<Expr>,
        axial_distance: Idx<Expr>,
    ) -> Idx<Expr> {
        let span = body_ctx.empty_span();
        let d = self.lower_vec2_expr(body_ctx, profile_distance, axial_distance);
        let zero_x = self.lower_float_literal(body_ctx, 0.0);
        let zero_y = self.lower_float_literal(body_ctx, 0.0);
        let zero = self.lower_vec2_expr(body_ctx, zero_x, zero_y);
        let outside = self.lower_binary_call(body_ctx, "max", ("left", d), ("right", zero));
        let d_x = self.lower_member_expr(body_ctx, d, "x");
        let d_y = self.lower_member_expr(body_ctx, d, "y");
        let max_xy = self.lower_binary_call(body_ctx, "max", ("left", d_x), ("right", d_y));
        let inside_cap = self.lower_float_literal(body_ctx, 0.0);
        let inside =
            self.lower_binary_call(body_ctx, "min", ("left", max_xy), ("right", inside_cap));
        let outside_len = self.lower_scalar_call(body_ctx, "length", "value", outside);
        body_ctx.alloc_expr(
            Expr::Binary {
                lhs: inside,
                op: BinaryOp::Add,
                rhs: outside_len,
                op_span: span,
            },
            span,
        )
    }

    fn lower_named_call_expr(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        callee_name: &str,
        args: Vec<(&str, Idx<Expr>)>,
    ) -> Idx<Expr> {
        let span = body_ctx.empty_span();
        let callee = body_ctx.alloc_expr(Expr::Variable(SmolStr::new(callee_name)), span);
        body_ctx.alloc_expr(
            Expr::Call {
                callee,
                type_args: Vec::new(),
                args: args
                    .into_iter()
                    .map(|(name, value)| Arg::Named {
                        name: SmolStr::new(name),
                        value,
                        span,
                        name_span: span,
                    })
                    .collect(),
            },
            span,
        )
    }

    fn lower_scalar_call(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        callee_name: &str,
        arg_name: &str,
        value: Idx<Expr>,
    ) -> Idx<Expr> {
        self.lower_named_call_expr(body_ctx, callee_name, vec![(arg_name, value)])
    }

    fn lower_binary_call(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        callee_name: &str,
        a: (&str, Idx<Expr>),
        b: (&str, Idx<Expr>),
    ) -> Idx<Expr> {
        self.lower_named_call_expr(body_ctx, callee_name, vec![a, b])
    }

    fn lower_ternary_call(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        callee_name: &str,
        a: (&str, Idx<Expr>),
        b: (&str, Idx<Expr>),
        c: (&str, Idx<Expr>),
    ) -> Idx<Expr> {
        self.lower_named_call_expr(body_ctx, callee_name, vec![a, b, c])
    }

    fn lower_member_expr(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        object: Idx<Expr>,
        member: &str,
    ) -> Idx<Expr> {
        let span = body_ctx.empty_span();
        body_ctx.alloc_expr(
            Expr::Member {
                object,
                member: SmolStr::new(member),
                member_span: span,
            },
            span,
        )
    }

    fn lower_vec2_expr(
        &mut self,
        body_ctx: &mut BodyLoweringContext,
        x: Idx<Expr>,
        y: Idx<Expr>,
    ) -> Idx<Expr> {
        self.lower_named_call_expr(body_ctx, "vec2", vec![("x", x), ("y", y)])
    }

    fn lower_float_literal(&mut self, body_ctx: &mut BodyLoweringContext, value: f64) -> Idx<Expr> {
        body_ctx.alloc_expr(Expr::Literal(Literal::Float(value)), body_ctx.empty_span())
    }

    fn field_primitive_from_name(name: &str) -> Option<FieldPrimitive> {
        match name {
            "sphere" => Some(FieldPrimitive::Sphere),
            "box" => Some(FieldPrimitive::Box),
            "capsule" => Some(FieldPrimitive::Capsule),
            "cylinder" => Some(FieldPrimitive::Cylinder),
            "plane" => Some(FieldPrimitive::Plane),
            "torus" => Some(FieldPrimitive::Torus),
            "rounded_box" => Some(FieldPrimitive::RoundedBox),
            "ellipsoid" => Some(FieldPrimitive::Ellipsoid),
            "cone" => Some(FieldPrimitive::Cone),
            "capped_cone" => Some(FieldPrimitive::CappedCone),
            "box_frame" => Some(FieldPrimitive::BoxFrame),
            "slab" => Some(FieldPrimitive::Slab),
            "triangle_prism" => Some(FieldPrimitive::TrianglePrism),
            "hex_prism" => Some(FieldPrimitive::HexPrism),
            _ => None,
        }
    }

    fn is_profile_primitive_name(name: &str) -> bool {
        matches!(
            name,
            "circle2"
                | "rect2"
                | "rounded_rect2"
                | "capsule2"
                | "segment2"
                | "polygon2"
                | "polyline2"
        )
    }

    fn profile_primitive_from_name(name: &str) -> Option<ProfilePrimitive> {
        match name {
            "circle2" => Some(ProfilePrimitive::Circle2),
            "rect2" => Some(ProfilePrimitive::Rect2),
            "rounded_rect2" => Some(ProfilePrimitive::RoundedRect2),
            "capsule2" => Some(ProfilePrimitive::Capsule2),
            "segment2" => Some(ProfilePrimitive::Segment2),
            "polygon2" => Some(ProfilePrimitive::Polygon2),
            "polyline2" => Some(ProfilePrimitive::Polyline2),
            _ => None,
        }
    }

    fn primitive_callee_name(primitive: FieldPrimitive) -> &'static str {
        match primitive {
            FieldPrimitive::Sphere => "sphere",
            FieldPrimitive::Box => "box",
            FieldPrimitive::Capsule => "capsule",
            FieldPrimitive::Cylinder => "cylinder",
            FieldPrimitive::Plane => "plane",
            FieldPrimitive::Torus => "torus",
            FieldPrimitive::RoundedBox => "rounded_box",
            FieldPrimitive::Ellipsoid => "ellipsoid",
            FieldPrimitive::Cone => "cone",
            FieldPrimitive::CappedCone => "capped_cone",
            FieldPrimitive::BoxFrame => "box_frame",
            FieldPrimitive::Slab => "slab",
            FieldPrimitive::TrianglePrism => "triangle_prism",
            FieldPrimitive::HexPrism => "hex_prism",
        }
    }

    fn profile_primitive_callee_name(primitive: ProfilePrimitive) -> &'static str {
        match primitive {
            ProfilePrimitive::Circle2 => "circle2",
            ProfilePrimitive::Rect2 => "rect2",
            ProfilePrimitive::RoundedRect2 => "rounded_rect2",
            ProfilePrimitive::Capsule2 => "capsule2",
            ProfilePrimitive::Segment2 => "segment2",
            ProfilePrimitive::Polygon2 => "polygon2",
            ProfilePrimitive::Polyline2 => "polyline2",
        }
    }

    fn lower_class(&mut self, c: ast::ClassDef) -> Class {
        let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = c.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(c.syntax());
        let type_params = c
            .type_params()
            .map(|t| TypeParam {
                name: SmolStr::new(t.text()),
                bounds: Vec::new(),
            })
            .collect();
        let implements = c
            .is_a()
            .map(|t| SmolStr::new(t.text()))
            .into_iter()
            .collect();
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        for field in c.fields() {
            fields.push(self.lower_field(field));
        }

        for method in c.methods() {
            let func = self.lower_method(method);
            let id = self.module.functions.alloc(func);
            methods.push(id);
        }

        for private_block in c.syntax().children().filter_map(ast::PrivateBlock::cast) {
            for child in private_block.syntax().children() {
                if let Some(field) = ast::FieldDef::cast(child.clone()) {
                    fields.push(self.lower_field(field));
                    continue;
                }
                if let Some(method) = ast::MethodDef::cast(child.clone()) {
                    let func = self.lower_method(method);
                    let id = self.module.functions.alloc(func);
                    methods.push(id);
                    continue;
                }
            }
        }

        Class {
            name,
            name_span,
            visibility,
            role: ClassRole::Class,
            type_params,
            fields,
            methods,
            implements,
        }
    }

    fn lower_class_like<T>(&mut self, c: T, role: ClassRole) -> Class
    where
        T: ClassLikeDef,
    {
        let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = c.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(c.syntax());
        let type_params = c
            .type_params()
            .map(|t| TypeParam {
                name: SmolStr::new(t.text()),
                bounds: Vec::new(),
            })
            .collect();
        let implements = c
            .is_a()
            .map(|t| SmolStr::new(t.text()))
            .into_iter()
            .collect();
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        for field in c.fields() {
            fields.push(self.lower_field(field));
        }

        for method in c.methods() {
            let func = self.lower_method(method);
            let id = self.module.functions.alloc(func);
            methods.push(id);
        }

        for private_block in c.syntax().children().filter_map(ast::PrivateBlock::cast) {
            for child in private_block.syntax().children() {
                if let Some(field) = ast::FieldDef::cast(child.clone()) {
                    fields.push(self.lower_field(field));
                    continue;
                }
                if let Some(method) = ast::MethodDef::cast(child.clone()) {
                    let func = self.lower_method(method);
                    let id = self.module.functions.alloc(func);
                    methods.push(id);
                    continue;
                }
            }
        }

        Class {
            name,
            name_span,
            visibility,
            role,
            type_params,
            fields,
            methods,
            implements,
        }
    }

    fn lower_enum(&mut self, e: ast::EnumDef) -> Enum {
        let name = e.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = e.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(e.syntax());
        let type_params = e
            .type_params()
            .map(|t| TypeParam {
                name: SmolStr::new(t.text()),
                bounds: Vec::new(),
            })
            .collect();
        let mut variants = Vec::new();
        for variant in e.variants() {
            let v_name = variant
                .name()
                .map(|t| SmolStr::new(t.text()))
                .unwrap_or_default();
            let v_name_span = variant.name().map(|t| t.text_range());
            let params = variant.params().map(|p| self.lower_param(p)).collect();
            variants.push(EnumVariant {
                name: v_name,
                name_span: v_name_span,
                params,
            });
        }
        Enum {
            name,
            name_span,
            visibility,
            type_params,
            variants,
        }
    }

    fn lower_interface_from_class(&mut self, c: ast::ClassDef) -> Interface {
        let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = c.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(c.syntax());
        let type_params = c
            .type_params()
            .map(|t| TypeParam {
                name: SmolStr::new(t.text()),
                bounds: Vec::new(),
            })
            .collect();
        let mut methods = Vec::new();
        for method in c.must_methods() {
            let m_name = method
                .name()
                .map(|t| SmolStr::new(t.text()))
                .unwrap_or_default();
            let m_name_span = method.name().map(|t| t.text_range());
            let params = method.params().map(|p| self.lower_param(p)).collect();
            let ret_type = method.ret_type().map(|t| self.lower_type_ref(t));
            methods.push(InterfaceMethod {
                name: m_name,
                name_span: m_name_span,
                params,
                ret_type,
                kind: if method.is_check() {
                    InterfaceMethodKind::Check
                } else {
                    InterfaceMethodKind::Method
                },
            });
        }
        Interface {
            name,
            name_span,
            visibility,
            type_params,
            methods,
        }
    }

    fn class_is_interface(&self, c: &ast::ClassDef) -> bool {
        c.must_methods().next().is_some()
    }

    fn lower_method(&mut self, m: ast::MethodDef) -> Function {
        let name = m.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = m.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(m.syntax());
        let params = m.params().map(|p| self.lower_param(p)).collect();
        let ret_type = m.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in m.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());

        Function {
            name,
            name_span,
            attributes: Vec::new(),
            visibility,
            kind: FunctionKind::Method,
            role: FunctionRole::Function,
            field: None,
            region: None,
            domain: None,
            render: None,
            field_graph: None,
            system_metadata: None,
            type_params: Vec::new(),
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_param(&mut self, p: ast::Param) -> Param {
        Param {
            name: p.name().map(|t| SmolStr::new(t.text())).unwrap_or_default(),
            name_span: p.name().map(|t| t.text_range()),
            ty: p.ty().map(|t| self.lower_type_ref(t)),
        }
    }

    fn lower_field(&mut self, f: ast::FieldDef) -> Field {
        Field {
            name: f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default(),
            name_span: f.name().map(|t| t.text_range()),
            visibility: visibility_for_node_default(f.syntax()),
            ty: f.ty().map(|t| self.lower_type_ref(t)),
            mutable: f.is_mutable(),
            default: f
                .default_expr()
                .and_then(|expr| self.lower_field_default(expr)),
        }
    }

    fn lower_field_default(&mut self, expr: ast::Expr) -> Option<FieldDefault> {
        match expr {
            ast::Expr::Literal(l) => {
                let token = first_non_trivia_token(l.syntax())?;
                let lit = match token.kind() {
                    SyntaxKind::IntNumber => Literal::Integer(token.text().parse().unwrap_or(0)),
                    SyntaxKind::FloatNumber => Literal::Float(token.text().parse().unwrap_or(0.0)),
                    SyntaxKind::StringLiteral => {
                        Literal::String(SmolStr::new(token.text().trim_matches('\"')))
                    }
                    SyntaxKind::TrueKw => Literal::Boolean(true),
                    SyntaxKind::FalseKw => Literal::Boolean(false),
                    SyntaxKind::NothingKw => Literal::Nil,
                    _ => return None,
                };
                Some(FieldDefault::Literal(lit))
            }
            ast::Expr::List(list) => {
                let mut items = Vec::new();
                for item in list.items() {
                    let lowered = self.lower_field_default(item)?;
                    items.push(lowered);
                }
                Some(FieldDefault::List(items))
            }
            ast::Expr::Map(map) => {
                let mut items = Vec::new();
                let mut iter = map.items();
                while let Some(key) = iter.next() {
                    let value = iter.next()?;
                    let key = self.lower_field_default(key)?;
                    let value = self.lower_field_default(value)?;
                    items.push((key, value));
                }
                Some(FieldDefault::Map(items))
            }
            ast::Expr::Paren(p) => {
                self.lower_field_default(p.syntax().children().filter_map(ast::Expr::cast).next()?)
            }
            _ => None,
        }
    }

    fn lower_type_ref(&mut self, t: ast::TypeRef) -> TypeRef {
        TypeRef {
            name: t
                .name()
                .map(|tok| SmolStr::new(tok.text()))
                .unwrap_or_default(),
            name_span: t.name().map(|tok| tok.text_range()),
            args: t
                .args()
                .into_iter()
                .map(|arg| self.lower_type_ref(arg))
                .collect(),
        }
    }
}

trait ClassLikeDef {
    fn syntax(&self) -> &SyntaxNode;
    fn name(&self) -> Option<SyntaxToken>;
    fn type_params(&self) -> Box<dyn Iterator<Item = SyntaxToken> + '_>;
    fn is_a(&self) -> Option<SyntaxToken>;
    fn fields(&self) -> Box<dyn Iterator<Item = ast::FieldDef> + '_>;
    fn methods(&self) -> Box<dyn Iterator<Item = ast::MethodDef> + '_>;
}

impl ClassLikeDef for ast::ResourceDef {
    fn syntax(&self) -> &SyntaxNode {
        <ast::ResourceDef as AstNode>::syntax(self)
    }

    fn name(&self) -> Option<SyntaxToken> {
        self.name()
    }

    fn type_params(&self) -> Box<dyn Iterator<Item = SyntaxToken> + '_> {
        Box::new(self.type_params())
    }

    fn is_a(&self) -> Option<SyntaxToken> {
        self.is_a()
    }

    fn fields(&self) -> Box<dyn Iterator<Item = ast::FieldDef> + '_> {
        Box::new(self.fields())
    }

    fn methods(&self) -> Box<dyn Iterator<Item = ast::MethodDef> + '_> {
        Box::new(self.methods())
    }
}

impl ClassLikeDef for ast::ValueDef {
    fn syntax(&self) -> &SyntaxNode {
        <ast::ValueDef as AstNode>::syntax(self)
    }

    fn name(&self) -> Option<SyntaxToken> {
        self.name()
    }

    fn type_params(&self) -> Box<dyn Iterator<Item = SyntaxToken> + '_> {
        Box::new(self.type_params())
    }

    fn is_a(&self) -> Option<SyntaxToken> {
        self.is_a()
    }

    fn fields(&self) -> Box<dyn Iterator<Item = ast::FieldDef> + '_> {
        Box::new(self.fields())
    }

    fn methods(&self) -> Box<dyn Iterator<Item = ast::MethodDef> + '_> {
        Box::new(self.methods())
    }
}

/// Parse type parameters with optional bounds from a syntax node.
/// Walks the TypeParamList child node, collecting (name, bounds) pairs.
/// In the TypeParamList, items are laid out as: Ident [Colon Ident] [Comma ...]*
fn lower_func_type_params(node: &SyntaxNode) -> Vec<TypeParam> {
    let Some(list_node) = node
        .children()
        .find(|it| it.kind() == SyntaxKind::TypeParamList)
    else {
        return Vec::new();
    };

    let tokens: Vec<SyntaxToken> = list_node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .filter(|it| !it.kind().is_trivia())
        .collect();

    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if tok.kind() == SyntaxKind::Comma {
            i += 1;
            continue;
        }
        if tok.kind() != SyntaxKind::Ident {
            i += 1;
            continue;
        }
        let param_name = SmolStr::new(tok.text());
        let mut bounds = Vec::new();
        // Check for optional bound: Colon Ident
        if i + 2 < tokens.len()
            && tokens[i + 1].kind() == SyntaxKind::Colon
            && tokens[i + 2].kind() == SyntaxKind::Ident
        {
            bounds.push(SmolStr::new(tokens[i + 2].text()));
            i += 3;
        } else {
            i += 1;
        }
        result.push(TypeParam {
            name: param_name,
            bounds,
        });
    }
    result
}

fn region_detail_level_for_name(name: &str) -> Option<RegionDetailLevel> {
    match name {
        "coarse" => Some(RegionDetailLevel::Coarse),
        "fine" => Some(RegionDetailLevel::Fine),
        _ => None,
    }
}

fn collect_region_layers(items: &[RegionItemMetadata]) -> Vec<RegionLayerBinding> {
    fn walk(items: &[RegionItemMetadata], out: &mut Vec<RegionLayerBinding>) {
        for item in items {
            match item {
                RegionItemMetadata::Compose {
                    detail: Some(detail),
                    shape,
                    shape_span,
                    ..
                } => out.push(RegionLayerBinding {
                    detail: *detail,
                    shape: shape.clone(),
                    shape_span: *shape_span,
                }),
                RegionItemMetadata::Scatter { items, .. } => walk(items, out),
                RegionItemMetadata::Conditional {
                    then_items,
                    else_items,
                    ..
                } => {
                    walk(then_items, out);
                    walk(else_items, out);
                }
                RegionItemMetadata::Compose { .. } => {}
            }
        }
    }

    let mut layers = Vec::new();
    walk(items, &mut layers);
    layers
}

fn lower_bool_config_expr(expr: &ast::Expr) -> Option<bool> {
    match expr.syntax().text().to_string().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn lower_attributes(attributes: impl Iterator<Item = ast::Attribute>) -> Vec<AttributeAnnotation> {
    attributes
        .filter_map(|attribute| {
            let name = attribute.name()?;
            let args = attribute
                .args()
                .into_iter()
                .map(|arg| {
                    let key = arg.key();
                    let value = arg.value();
                    AttributeArg {
                        key: SmolStr::new(key.text()),
                        key_span: Some(key.text_range()),
                        value: lower_attribute_arg_value(&value),
                        value_span: Some(value.text_range()),
                    }
                })
                .collect();
            Some(AttributeAnnotation {
                name: SmolStr::new(name.text()),
                name_span: Some(name.text_range()),
                args,
                span: attribute.syntax().text_range(),
            })
        })
        .collect()
}

fn lower_attribute_arg_value(value: &SyntaxToken) -> SmolStr {
    match value.kind() {
        SyntaxKind::StringLiteral => parse_string_literal(value.text()),
        _ => SmolStr::new(value.text()),
    }
}

fn parse_system_metadata(node: &SyntaxNode) -> Option<SystemMetadata> {
    let text = node.text().to_string();
    let start = text.find('[')?;
    let mut depth = 0usize;
    let mut end = None;
    for (idx, ch) in text.char_indices().skip(start) {
        if ch == '[' {
            depth += 1;
        } else if ch == ']' {
            if depth == 0 {
                break;
            }
            depth -= 1;
            if depth == 0 {
                end = Some(idx);
                break;
            }
        }
    }
    let end = end?;
    let body = &text[start + 1..end];
    let mut stage = None;
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut before = Vec::new();
    let mut after = Vec::new();
    for raw in split_top_level_commas(body) {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if key == "stage" {
                let value = value.trim().trim_matches('"');
                if !value.is_empty() {
                    stage = Some(SmolStr::new(value));
                }
                continue;
            }
            if key == "reads" {
                reads = parse_system_name_list(value);
                continue;
            }
            if key == "writes" {
                writes = parse_system_name_list(value);
                continue;
            }
            if key == "before" {
                before = parse_system_name_list(value);
                continue;
            }
            if key == "after" {
                after = parse_system_name_list(value);
                continue;
            }
        } else if let Some(value) = part.strip_prefix("stage=") {
            let value = value.trim().trim_matches('"');
            if !value.is_empty() {
                stage = Some(SmolStr::new(value));
            }
            continue;
        }
        if let Some(value) = part.strip_prefix("reads=") {
            reads = parse_system_name_list(value);
            continue;
        }
        if let Some(value) = part.strip_prefix("writes=") {
            writes = parse_system_name_list(value);
            continue;
        }
        if let Some(value) = part.strip_prefix("before=") {
            before = parse_system_name_list(value);
            continue;
        }
        if let Some(value) = part.strip_prefix("after=") {
            after = parse_system_name_list(value);
            continue;
        }
    }
    Some(SystemMetadata {
        stage,
        reads,
        writes,
        before,
        after,
    })
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(input[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start <= input.len() {
        parts.push(input[start..].trim());
    }
    parts
}

fn parse_system_name_list(raw: &str) -> Vec<SmolStr> {
    let trimmed = raw.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return Vec::new();
    }
    let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
    inner
        .split(',')
        .map(|item| item.trim().trim_matches('"'))
        .filter(|item| !item.is_empty())
        .map(SmolStr::new)
        .collect()
}

struct BodyLoweringContext {
    body: Body,
    scopes: Vec<HashSet<SmolStr>>,
}

impl BodyLoweringContext {
    fn new() -> Self {
        Self {
            body: Body {
                exprs: Arena::new(),
                stmts: Arena::new(),
                root_stmts: Vec::new(),
                expr_spans: Vec::new(),
                stmt_spans: Vec::new(),
            },
            scopes: vec![HashSet::new()],
        }
    }

    fn alloc_expr(&mut self, expr: Expr, span: TextRange) -> Idx<Expr> {
        let idx = self.body.exprs.alloc(expr);
        self.body.expr_spans.push(span);
        idx
    }

    fn alloc_stmt(&mut self, stmt: Stmt, span: TextRange) -> Idx<Stmt> {
        let idx = self.body.stmts.alloc(stmt);
        self.body.stmt_spans.push(span);
        idx
    }

    fn empty_span(&self) -> TextRange {
        TextRange::empty(TextSize::from(0))
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_name(&mut self, name: &SmolStr) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.clone());
        }
    }

    fn name_exists(&self, name: &SmolStr) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }

    fn lower_type_ref(&mut self, t: ast::TypeRef) -> TypeRef {
        TypeRef {
            name: t
                .name()
                .map(|tok| SmolStr::new(tok.text()))
                .unwrap_or_default(),
            name_span: t.name().map(|tok| tok.text_range()),
            args: t
                .args()
                .into_iter()
                .map(|arg| self.lower_type_ref(arg))
                .collect(),
        }
    }

    fn lower_stmt(&mut self, stmt: ast::Stmt) -> Idx<Stmt> {
        let stmt_span = stmt.syntax().text_range();
        let name_span = match &stmt {
            ast::Stmt::VarAssign(v) => v.name().map(|t| t.text_range()),
            _ => None,
        };
        let hir_stmt = match stmt {
            ast::Stmt::Expr(e) => {
                let expr = e.expr().and_then(|e| self.lower_expr(e));
                match expr {
                    Some(e) => Stmt::Expr(e),
                    None => Stmt::Break, // Error recovery or empty
                }
            }
            ast::Stmt::VarAssign(v) => {
                let name = v.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let visibility = visibility_for_node(v.syntax());
                let mutable = has_token(v.syntax(), SyntaxKind::MutableKw);
                let value = v
                    .value()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                match assign_op_for_node(v.syntax()) {
                    Some(AssignOp::Assign) | None => {
                        let exists = self.name_exists(&name);
                        if !mutable && exists {
                            Stmt::Assign {
                                name,
                                op: AssignOp::Assign,
                                value,
                                mutable: false,
                                visibility,
                            }
                        } else {
                            self.declare_name(&name);
                            Stmt::Let {
                                name,
                                value,
                                mutable,
                                visibility,
                            }
                        }
                    }
                    Some(op) => Stmt::Assign {
                        name,
                        op,
                        value,
                        mutable,
                        visibility,
                    },
                }
            }
            ast::Stmt::IfStmt(i) => {
                let nested = self.lower_if_stmt(i);
                return nested;
            }
            ast::Stmt::WhileStmt(w) => {
                let condition = w
                    .condition()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Boolean(false)), self.empty_span())
                    });
                let body = self.lower_block(w.body());
                Stmt::While { condition, body }
            }
            ast::Stmt::ForStmt(f) => {
                let value_name = f
                    .value_name()
                    .map(|t| SmolStr::new(t.text()))
                    .unwrap_or_default();
                let key_name = f.key_name().map(|t| SmolStr::new(t.text()));
                let index_name = f.index_name().map(|t| SmolStr::new(t.text()));
                let iterable = f
                    .iterable()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                let body = self.lower_block(f.body());
                Stmt::For {
                    value_name,
                    key_name,
                    index_name,
                    iterable,
                    body,
                }
            }
            ast::Stmt::MatchStmt(m) => {
                let subject = m
                    .subject()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                let mut cases = Vec::new();
                let mut otherwise = None;
                for case in m.cases() {
                    match case {
                        ast::MatchCaseItem::Case(c) => {
                            let labels = c.labels().map(|p| self.lower_pattern(p)).collect();
                            let guard = c.guard().and_then(|e| self.lower_expr(e));
                            let body = if let Some(block) = c.block() {
                                self.lower_block(Some(block))
                            } else {
                                c.statement()
                                    .map(|s| vec![self.lower_stmt(s)])
                                    .unwrap_or_default()
                            };
                            cases.push(MatchCase {
                                labels,
                                guard,
                                body,
                            });
                        }
                        ast::MatchCaseItem::Otherwise(c) => {
                            let body = if let Some(block) = c.block() {
                                self.lower_block(Some(block))
                            } else {
                                c.statement()
                                    .map(|s| vec![self.lower_stmt(s)])
                                    .unwrap_or_default()
                            };
                            otherwise = Some(body);
                        }
                    }
                }
                Stmt::Match {
                    subject,
                    cases,
                    otherwise,
                }
            }
            ast::Stmt::IgnoreResultStmt(d) => {
                let expr = d
                    .expr()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                Stmt::IgnoreResult { expr }
            }
            ast::Stmt::CaptureStmt(c) => {
                let name = c.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let value = c
                    .expr()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                Stmt::Capture { name, value }
            }
            ast::Stmt::DeferStmt(d) => {
                let expr = d
                    .expr()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Nil), self.empty_span())
                    });
                Stmt::Defer { expr }
            }
            ast::Stmt::AssertStmt(a) => {
                let expr = a
                    .expr()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Boolean(false)), self.empty_span())
                    });
                let kind = match a.mode() {
                    ast::AssertMode::Value => crate::hir::AssertKind::Value,
                    ast::AssertMode::Identity => crate::hir::AssertKind::Identity,
                    ast::AssertMode::Approx => crate::hir::AssertKind::Approx,
                };
                let rhs = a.rhs_expr().and_then(|e| self.lower_expr(e));
                let tolerance = a.tolerance_expr().and_then(|e| self.lower_expr(e));
                Stmt::Assert {
                    kind,
                    expr,
                    rhs,
                    tolerance,
                }
            }
            ast::Stmt::RequireStmt(r) => {
                let condition = r
                    .condition()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(Expr::Literal(Literal::Boolean(false)), self.empty_span())
                    });
                let message = r
                    .message()
                    .and_then(|e| self.lower_expr(e))
                    .unwrap_or_else(|| {
                        self.alloc_expr(
                            Expr::Literal(Literal::String(SmolStr::new(""))),
                            self.empty_span(),
                        )
                    });
                Stmt::Require { condition, message }
            }
            ast::Stmt::UseStmt(u) => {
                let (names, module, _module_span) = parse_use_stmt(&u);
                Stmt::Use { names, module }
            }
            ast::Stmt::ReturnStmt(r) => {
                let value = r.value().and_then(|e| self.lower_expr(e));
                Stmt::Return(value)
            }
            ast::Stmt::BreakStmt(_) => Stmt::Break,
            ast::Stmt::ContinueStmt(_) => Stmt::Continue,
            _ => Stmt::Break, // Error recovery or unsupported statement
        };
        let stmt_span = name_span.unwrap_or(stmt_span);
        self.alloc_stmt(hir_stmt, stmt_span)
    }

    fn lower_block(&mut self, block: Option<ast::Block>) -> Vec<Idx<Stmt>> {
        let mut stmts = Vec::new();
        if let Some(b) = block {
            self.enter_scope();
            for stmt in b.statements() {
                stmts.push(self.lower_stmt(stmt));
            }
            self.exit_scope();
        }
        stmts
    }

    fn lower_if_stmt(&mut self, i: ast::IfStmt) -> Idx<Stmt> {
        let condition = i
            .condition()
            .and_then(|e| self.lower_expr(e))
            .unwrap_or_else(|| {
                self.alloc_expr(Expr::Literal(Literal::Boolean(false)), self.empty_span())
            });
        let then_branch = self.lower_block(i.then_block());
        let else_branch = if let Some(block) = i.else_block() {
            Some(self.lower_block(Some(block)))
        } else if let Some(else_if) = i.else_if() {
            let stmt = self.lower_if_stmt(else_if);
            Some(vec![stmt])
        } else {
            None
        };
        let stmt = Stmt::If {
            condition,
            then_branch,
            else_branch,
        };
        let span = i.syntax().text_range();
        self.alloc_stmt(stmt, span)
    }

    fn lower_pattern(&mut self, pattern: ast::Pattern) -> Pattern {
        if let Some(token) = pattern.literals().next() {
            let lit = match token.kind() {
                SyntaxKind::StringLiteral => Literal::String(parse_string_literal(token.text())),
                SyntaxKind::IntNumber => Literal::Integer(parse_int_literal(token.text())),
                SyntaxKind::FloatNumber => Literal::Float(parse_float_literal(token.text())),
                SyntaxKind::TrueKw => Literal::Boolean(true),
                SyntaxKind::FalseKw => Literal::Boolean(false),
                SyntaxKind::NothingKw => Literal::Nil,
                _ => Literal::Nil,
            };
            return Pattern::Literal(lit);
        }

        let parts: Vec<SmolStr> = pattern
            .name_tokens()
            .map(|t| SmolStr::new(t.text()))
            .collect();
        let args: Vec<Pattern> = pattern.args().map(|p| self.lower_pattern(p)).collect();
        let fields: Vec<(SmolStr, Pattern)> = pattern
            .fields()
            .filter_map(|field| {
                let name = field.name().map(|token| SmolStr::new(token.text()))?;
                let lowered = field
                    .pattern()
                    .map(|p| self.lower_pattern(p))
                    .unwrap_or_else(|| Pattern::Binding(name.clone()));
                Some((name, lowered))
            })
            .collect();

        if !fields.is_empty() {
            return Pattern::Struct { parts, fields };
        }

        if parts.len() == 1 && args.is_empty() {
            let name = parts[0].clone();
            if name.as_str() == "_" {
                Pattern::Wildcard
            } else {
                Pattern::Binding(name)
            }
        } else {
            Pattern::Path { parts, args }
        }
    }

    fn lower_expr(&mut self, expr: ast::Expr) -> Option<Idx<Expr>> {
        // println!("Lowering expr: {:?}", expr.syntax().kind());
        let expr_span = expr.syntax().text_range();
        let hir_expr = match expr {
            ast::Expr::Literal(l) => {
                let token = first_non_trivia_token(l.syntax())?;
                let lit = match token.kind() {
                    SyntaxKind::IntNumber => Literal::Integer(parse_int_literal(token.text())),
                    SyntaxKind::FloatNumber => Literal::Float(parse_float_literal(token.text())),
                    SyntaxKind::StringLiteral => {
                        Literal::String(parse_string_literal(token.text()))
                    }
                    SyntaxKind::TrueKw => Literal::Boolean(true),
                    SyntaxKind::FalseKw => Literal::Boolean(false),
                    SyntaxKind::NothingKw => Literal::Nil,
                    _ => return None,
                };
                Expr::Literal(lit)
            }
            ast::Expr::Ident(i) => {
                let name = i.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                Expr::Variable(name)
            }
            ast::Expr::Capture(c) => {
                let capture_span = c.syntax().text_range();
                let target_expr = c.target()?;
                let target = self.lower_expr(target_expr)?;
                let callee = self.alloc_expr(Expr::Variable(SmolStr::new("capture")), capture_span);
                Expr::Call {
                    callee,
                    args: vec![Arg::Named {
                        name: SmolStr::new("scene"),
                        value: target,
                        span: capture_span,
                        name_span: capture_span,
                    }],
                    type_args: Vec::new(),
                }
            }
            ast::Expr::Bin(b) => {
                let lhs = self.lower_expr(b.lhs()?)?;
                let rhs = self.lower_expr(b.rhs()?)?;
                let (op, op_span) = self.lower_binary_op(b.syntax())?;
                Expr::Binary {
                    lhs,
                    op,
                    rhs,
                    op_span,
                }
            }
            ast::Expr::Prefix(p) => {
                let target = self.lower_expr(p.expr()?)?;
                let (op, op_span) = self.lower_unary_op(p.syntax())?;
                if matches!(op, UnaryOp::Spawn) {
                    let target_span = p
                        .expr()
                        .map(|e| e.syntax().text_range())
                        .unwrap_or(expr_span);
                    let (size, objective) = self.parse_detach_tail(p.syntax(), target_span);
                    Expr::Detach {
                        target,
                        size,
                        objective,
                    }
                } else {
                    Expr::Unary {
                        op,
                        expr: target,
                        op_span,
                    }
                }
            }
            ast::Expr::Try(t) => {
                let target = self.lower_expr(t.expr()?)?;
                let op_span = first_token_of_kind(t.syntax(), SyntaxKind::Question)
                    .map(|token| token.text_range())
                    .unwrap_or(expr_span);
                Expr::Unary {
                    op: UnaryOp::Try,
                    expr: target,
                    op_span,
                }
            }
            ast::Expr::Crash(c) => {
                let expr = self.lower_expr(c.expr()?)?;
                Expr::Crash { expr }
            }
            ast::Expr::TypeApply(t) => {
                let callee = self.lower_expr(t.callee()?)?;
                let type_args = t
                    .args()
                    .into_iter()
                    .map(|arg| self.lower_type_ref(arg))
                    .collect();
                Expr::TypeApply { callee, type_args }
            }
            ast::Expr::Index(i) => {
                let object = self.lower_expr(i.object()?)?;
                let index = self.lower_expr(i.index()?)?;
                Expr::Index {
                    object,
                    index,
                    index_span: i.syntax().text_range(),
                }
            }
            ast::Expr::Call(c) => {
                let mut type_args = Vec::new();
                let callee_expr = c.callee()?;
                let callee = if let ast::Expr::TypeApply(t) = callee_expr {
                    type_args = t
                        .args()
                        .into_iter()
                        .map(|arg| self.lower_type_ref(arg))
                        .collect();
                    self.lower_expr(t.callee()?)?
                } else {
                    self.lower_expr(callee_expr)?
                };
                let args = c.args().filter_map(|a| self.lower_arg(a)).collect();
                Expr::Call {
                    callee,
                    args,
                    type_args,
                }
            }
            ast::Expr::Member(m) => {
                let object = self.lower_expr(m.object()?)?;
                let member = m.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let member_span = m.name().map(|t| t.text_range()).unwrap_or(expr_span);
                Expr::Member {
                    object,
                    member,
                    member_span,
                }
            }
            ast::Expr::Paren(p) => {
                return self.lower_expr(p.syntax().children().filter_map(ast::Expr::cast).next()?);
            }
            ast::Expr::List(l) => {
                let items = l.items().filter_map(|e| self.lower_expr(e)).collect();
                Expr::List(items)
            }
            ast::Expr::Map(m) => {
                let mut items = Vec::new();
                let mut iter = m.items().filter_map(|e| self.lower_expr(e));
                while let (Some(key), Some(value)) = (iter.next(), iter.next()) {
                    items.push((key, value));
                }
                Expr::Map(items)
            }
            ast::Expr::StringInterp(s) => {
                let parts = self.lower_string_interp(s);
                Expr::StringInterp(parts)
            }
        };
        Some(self.alloc_expr(hir_expr, expr_span))
    }

    fn lower_arg(&mut self, arg: ast::Arg) -> Option<Arg> {
        match arg {
            ast::Arg::Positional(e) => {
                let span = e.syntax().text_range();
                Some(Arg::Positional {
                    value: self.lower_expr(e)?,
                    span,
                })
            }
            ast::Arg::Named(n) => {
                let name = n.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
                let span = n.syntax().text_range();
                let name_span = n.name().map(|t| t.text_range()).unwrap_or(span);
                let value = self.lower_expr(n.value()?)?;
                Some(Arg::Named {
                    name,
                    value,
                    span,
                    name_span,
                })
            }
        }
    }

    fn lower_binary_op(&self, node: &crate::parser::SyntaxNode) -> Option<(BinaryOp, TextRange)> {
        let op_tok = node
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| {
                matches!(
                    it.kind(),
                    SyntaxKind::Plus
                        | SyntaxKind::Minus
                        | SyntaxKind::Star
                        | SyntaxKind::Slash
                        | SyntaxKind::Percent
                        | SyntaxKind::EqEq
                        | SyntaxKind::BangEq
                        | SyntaxKind::Less
                        | SyntaxKind::LessEq
                        | SyntaxKind::Greater
                        | SyntaxKind::GreaterEq
                        | SyntaxKind::AndKw
                        | SyntaxKind::OrKw
                        | SyntaxKind::QuestionQuestion
                        | SyntaxKind::Ampersand
                        | SyntaxKind::Pipe
                        | SyntaxKind::Caret
                        | SyntaxKind::ShiftLeft
                        | SyntaxKind::ShiftRight
                        | SyntaxKind::Range
                        | SyntaxKind::Equals
                        | SyntaxKind::PlusEq
                        | SyntaxKind::MinusEq
                        | SyntaxKind::StarEq
                        | SyntaxKind::SlashEq
                )
            })?;

        let op = match op_tok.kind() {
            SyntaxKind::Plus => BinaryOp::Add,
            SyntaxKind::Minus => BinaryOp::Sub,
            SyntaxKind::Star => BinaryOp::Mul,
            SyntaxKind::Slash => BinaryOp::Div,
            SyntaxKind::Percent => BinaryOp::Mod,
            SyntaxKind::EqEq => BinaryOp::Eq,
            SyntaxKind::BangEq => BinaryOp::Ne,
            SyntaxKind::Less => BinaryOp::Lt,
            SyntaxKind::LessEq => BinaryOp::Le,
            SyntaxKind::Greater => BinaryOp::Gt,
            SyntaxKind::GreaterEq => BinaryOp::Ge,
            SyntaxKind::AndKw => BinaryOp::And,
            SyntaxKind::OrKw => BinaryOp::Or,
            SyntaxKind::QuestionQuestion => BinaryOp::Otherwise,
            SyntaxKind::Ampersand => BinaryOp::BitAnd,
            SyntaxKind::Pipe => BinaryOp::BitOr,
            SyntaxKind::Caret => BinaryOp::BitXor,
            SyntaxKind::ShiftLeft => BinaryOp::Shl,
            SyntaxKind::ShiftRight => BinaryOp::Shr,
            SyntaxKind::Range => BinaryOp::Range,
            SyntaxKind::Equals => BinaryOp::Assign,
            SyntaxKind::PlusEq => BinaryOp::AddAssign,
            SyntaxKind::MinusEq => BinaryOp::SubAssign,
            SyntaxKind::StarEq => BinaryOp::MulAssign,
            SyntaxKind::SlashEq => BinaryOp::DivAssign,
            _ => return None,
        };
        Some((op, op_tok.text_range()))
    }

    fn lower_unary_op(&self, node: &crate::parser::SyntaxNode) -> Option<(UnaryOp, TextRange)> {
        let op_tok = node
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| {
                matches!(
                    it.kind(),
                    SyntaxKind::Minus
                        | SyntaxKind::NotKw
                        | SyntaxKind::BitwiseNot
                        | SyntaxKind::AwaitKw
                        | SyntaxKind::DetachKw
                        | SyntaxKind::SpawnKw
                        | SyntaxKind::FireKw
                        | SyntaxKind::ErrKw
                )
            })?;

        let op = match op_tok.kind() {
            SyntaxKind::Minus => UnaryOp::Neg,
            SyntaxKind::NotKw => UnaryOp::Not,
            SyntaxKind::BitwiseNot => UnaryOp::BitNot,
            SyntaxKind::AwaitKw => UnaryOp::Await,
            SyntaxKind::DetachKw => UnaryOp::Spawn,
            SyntaxKind::SpawnKw => UnaryOp::Spawn,
            SyntaxKind::FireKw => UnaryOp::Fire,
            SyntaxKind::ErrKw => UnaryOp::Err,
            SyntaxKind::Question => UnaryOp::Try,
            _ => return None,
        };
        Some((op, op_tok.text_range()))
    }

    fn lower_string_interp(&mut self, s: ast::StringInterp) -> Vec<StringPart> {
        let mut parts = Vec::new();
        for element in s.syntax().children_with_tokens() {
            if let Some(token) = element.clone().into_token() {
                match token.kind() {
                    SyntaxKind::StringStart => {
                        let text = token.text();
                        let text = text.strip_prefix('"').unwrap_or(text);
                        let text = text.strip_suffix('{').unwrap_or(text);
                        parts.push(StringPart::Literal(parse_string_fragment(text)));
                    }
                    SyntaxKind::StringPart => {
                        let text = token.text();
                        let text = text.strip_suffix('{').unwrap_or(text);
                        parts.push(StringPart::Literal(parse_string_fragment(text)));
                    }
                    SyntaxKind::StringEnd => {
                        let text = token.text();
                        let text = text.strip_suffix('"').unwrap_or(text);
                        parts.push(StringPart::Literal(parse_string_fragment(text)));
                    }
                    _ => {}
                }
            } else if let Some(node) = element.into_node()
                && let Some(expr) = ast::Expr::cast(node)
                && let Some(expr) = self.lower_expr(expr)
            {
                parts.push(StringPart::Expr(expr));
            }
        }
        parts
    }

    fn parse_detach_tail(
        &self,
        node: &crate::parser::SyntaxNode,
        target_span: TextRange,
    ) -> (PoolSize, Option<Objective>) {
        let mut after_target = false;
        let mut size = PoolSize::Fixed(1);
        let objective = None;
        let mut iter = node.children_with_tokens().peekable();

        while let Some(child) = iter.next() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    if n.text_range() == target_span {
                        after_target = true;
                    }
                }
                rowan::NodeOrToken::Token(t) => {
                    if !after_target {
                        continue;
                    }
                    match t.kind() {
                        SyntaxKind::Star => {
                            for next in iter.by_ref() {
                                if let Some(tok) = next.into_token() {
                                    if tok.kind().is_trivia() {
                                        continue;
                                    }
                                    match tok.kind() {
                                        SyntaxKind::IntNumber => {
                                            let parsed = tok.text().parse::<i64>().unwrap_or(1);
                                            size = PoolSize::Fixed(parsed);
                                        }
                                        SyntaxKind::Ident => {
                                            if tok.text() == "n" {
                                                size = PoolSize::Auto;
                                            }
                                        }
                                        _ => {}
                                    }
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        (size, objective)
    }
}

fn visibility_for_node_default(node: &crate::parser::SyntaxNode) -> Visibility {
    match visibility_for_node(node) {
        Some(visibility) => visibility,
        None => Visibility::Public,
    }
}

fn visibility_for_node(node: &crate::parser::SyntaxNode) -> Option<Visibility> {
    if has_token(node, SyntaxKind::PrivateKw) || has_private_block_ancestor(node) {
        Some(Visibility::Private)
    } else {
        None
    }
}

fn has_token(node: &crate::parser::SyntaxNode, kind: SyntaxKind) -> bool {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .any(|token| token.kind() == kind)
}

fn has_private_block_ancestor(node: &crate::parser::SyntaxNode) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == SyntaxKind::PrivateBlock {
            return true;
        }
        current = parent.parent();
    }
    false
}

fn parse_use_stmt(u: &ast::UseStmt) -> (Vec<UseName>, SmolStr, Option<TextRange>) {
    let mut names = Vec::new();
    let mut module_parts: Vec<String> = Vec::new();
    let mut in_module = false;
    let mut module_span: Option<TextRange> = None;

    for token in u
        .syntax()
        .children_with_tokens()
        .filter_map(|it| it.into_token())
    {
        match token.kind() {
            SyntaxKind::FromKw => {
                in_module = true;
            }
            SyntaxKind::Ident => {
                if in_module {
                    module_span = Some(
                        module_span
                            .map(|span| span.cover(token.text_range()))
                            .unwrap_or_else(|| token.text_range()),
                    );
                    module_parts.push(token.text().to_string());
                } else {
                    names.push(UseName {
                        kind: UseNameKind::Name(SmolStr::new(token.text())),
                        span: token.text_range(),
                    });
                }
            }
            SyntaxKind::Star => {
                if !in_module {
                    names.push(UseName {
                        kind: UseNameKind::Glob,
                        span: token.text_range(),
                    });
                }
            }
            SyntaxKind::Slash | SyntaxKind::Dot => {
                // separators in module path
            }
            _ => {}
        }
    }

    let module = if module_parts.is_empty() {
        SmolStr::new("")
    } else {
        SmolStr::new(module_parts.join("/"))
    };
    (names, module, module_span)
}

fn assign_op_for_node(node: &crate::parser::SyntaxNode) -> Option<AssignOp> {
    let op_tok = node
        .children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|it| {
            matches!(
                it.kind(),
                SyntaxKind::Equals
                    | SyntaxKind::PlusEq
                    | SyntaxKind::MinusEq
                    | SyntaxKind::StarEq
                    | SyntaxKind::SlashEq
            )
        })?;

    match op_tok.kind() {
        SyntaxKind::Equals => Some(AssignOp::Assign),
        SyntaxKind::PlusEq => Some(AssignOp::AddAssign),
        SyntaxKind::MinusEq => Some(AssignOp::SubAssign),
        SyntaxKind::StarEq => Some(AssignOp::MulAssign),
        SyntaxKind::SlashEq => Some(AssignOp::DivAssign),
        _ => None,
    }
}

fn parse_int_literal(text: &str) -> i64 {
    let cleaned = text.replace('_', "");
    if let Some(hex) = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
    {
        return i64::from_str_radix(hex, 16).unwrap_or_default();
    }
    if let Some(bin) = cleaned
        .strip_prefix("0b")
        .or_else(|| cleaned.strip_prefix("0B"))
    {
        return i64::from_str_radix(bin, 2).unwrap_or_default();
    }
    if let Some(oct) = cleaned
        .strip_prefix("0o")
        .or_else(|| cleaned.strip_prefix("0O"))
    {
        return i64::from_str_radix(oct, 8).unwrap_or_default();
    }
    cleaned.parse::<i64>().unwrap_or_default()
}

fn parse_float_literal(text: &str) -> f64 {
    let cleaned = text.replace('_', "");
    cleaned.parse::<f64>().unwrap_or_default()
}

fn parse_string_literal(text: &str) -> SmolStr {
    let mut raw = text;
    if let Some(stripped) = raw.strip_prefix('"') {
        raw = stripped;
    }
    if let Some(stripped) = raw.strip_suffix('"') {
        raw = stripped;
    }
    parse_string_fragment(raw)
}

fn parse_string_fragment(text: &str) -> SmolStr {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            while matches!(chars.peek(), Some('\\')) {
                chars.next();
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('{') => out.push('{'),
                Some('}') => out.push('}'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(ch);
        }
    }
    SmolStr::new(out)
}

fn first_non_trivia_token(node: &crate::parser::SyntaxNode) -> Option<crate::parser::SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|token| !token.kind().is_trivia())
}

fn first_token_of_kind(
    node: &crate::parser::SyntaxNode,
    kind: SyntaxKind,
) -> Option<crate::parser::SyntaxToken> {
    node.children_with_tokens()
        .filter_map(|it| it.into_token())
        .find(|token| token.kind() == kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn test_lower_minimal() {
        let node = parse("1");
        let root = ast::Root::cast(node).unwrap();
        let _module = lower(root);
    }

    #[test]
    fn test_lower_basic() {
        let input = "fn add(a: Integer, b: Integer) -> Integer { return a + b }";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.functions.len(), 1);
        let func = &module.functions[Idx::new(0)];
        assert_eq!(func.name, "add");
        assert_eq!(func.visibility, Visibility::Public);
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[1].name, "b");

        let body = func.body.as_ref().unwrap();
        assert_eq!(body.root_stmts.len(), 1);
    }

    #[test]
    fn test_lower_type_args() {
        let input = "fn f(x: Result[Integer, Error]) -> List[String] { return [] }";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        let func = &module.functions[Idx::new(0)];
        let param_ty = func.params[0].ty.as_ref().unwrap();
        assert_eq!(param_ty.name, "Result");
        assert_eq!(param_ty.args.len(), 2);
        assert_eq!(param_ty.args[0].name, "Integer");
        assert_eq!(param_ty.args[1].name, "Error");

        let ret_ty = func.ret_type.as_ref().unwrap();
        assert_eq!(ret_ty.name, "List");
        assert_eq!(ret_ty.args.len(), 1);
        assert_eq!(ret_ty.args[0].name, "String");
    }

    #[test]
    fn test_lower_field_defaults() {
        let input = "\
class Defaults {
    name: String = \"ok\"
    count: Integer = 3
    flags: List = [true, false]
    meta: Map = {\"a\": 1}
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        let class = &module.classes[Idx::new(0)];
        assert_eq!(class.name, "Defaults");
        assert_eq!(class.fields.len(), 4);

        match class.fields[0].default.as_ref().unwrap() {
            FieldDefault::Literal(Literal::String(val)) => assert_eq!(val.as_str(), "ok"),
            other => panic!("unexpected default: {other:?}"),
        }
        match class.fields[1].default.as_ref().unwrap() {
            FieldDefault::Literal(Literal::Integer(val)) => assert_eq!(*val, 3),
            other => panic!("unexpected default: {other:?}"),
        }
        match class.fields[2].default.as_ref().unwrap() {
            FieldDefault::List(items) => assert_eq!(items.len(), 2),
            other => panic!("unexpected default: {other:?}"),
        }
        match class.fields[3].default.as_ref().unwrap() {
            FieldDefault::Map(items) => assert_eq!(items.len(), 1),
            other => panic!("unexpected default: {other:?}"),
        }
    }

    #[test]
    fn test_lower_for_match_use() {
        let input = "\
use {
    std,
    io
}
from core

fn f() -> Integer {
    for i in [1, 2] {
        if i == 1 {
            break
        }
    }
    match x {
        1 { return 1 }
        default { return 1 }
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.uses.len(), 1);
        let use_stmt = &module.uses[0];
        assert_eq!(
            use_stmt
                .names
                .iter()
                .filter_map(|name| name.name().cloned())
                .collect::<Vec<_>>(),
            vec![SmolStr::new("std"), SmolStr::new("io")]
        );
        assert_eq!(use_stmt.module, "core");

        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        assert_eq!(body.root_stmts.len(), 2);

        assert!(matches!(&body.stmts[body.root_stmts[0]], Stmt::For { .. }));
        assert!(matches!(
            &body.stmts[body.root_stmts[1]],
            Stmt::Match { .. }
        ));
    }

    #[test]
    fn test_lower_string_interp_and_ops() {
        let input = "\
fn f() -> String {
    return \"hi {name}\" 
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Return(Some(expr)) = stmt else {
            panic!("Expected return with expr");
        };
        let Expr::StringInterp(parts) = &body.exprs[*expr] else {
            panic!("Expected string interpolation");
        };
        assert_eq!(parts.len(), 3);
        assert!(matches!(parts[0], StringPart::Literal(_)));
        assert!(matches!(parts[1], StringPart::Expr(_)));
        assert!(matches!(parts[2], StringPart::Literal(_)));
    }

    #[test]
    fn test_lower_unary_and_binary_ops() {
        let input = "\
fn f() -> Result {
    return await detach Whale(name=\"moby\") * 1
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Return(Some(expr)) = stmt else {
            panic!("Expected return with expr");
        };
        let Expr::Unary { op, .. } = &body.exprs[*expr] else {
            panic!("Expected unary expr");
        };
        assert_eq!(*op, UnaryOp::Await);
    }

    #[test]
    fn test_lower_range_op() {
        let input = "\
fn f() -> Nothing {
    1...3
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Expr(expr) = stmt else {
            panic!("Expected expr stmt");
        };
        let Expr::Binary { op, .. } = &body.exprs[*expr] else {
            panic!("Expected binary expr");
        };
        assert_eq!(*op, BinaryOp::Range);
    }

    #[test]
    fn test_lower_map_member_and_named_args() {
        let input = "\
use std, io from core

fn f() -> Map {
    foo(a=1, b=2)
    foo.bar
    return {a: 1, b: 2}
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        assert_eq!(module.uses.len(), 1);
        let use_stmt = &module.uses[0];
        assert_eq!(
            use_stmt
                .names
                .iter()
                .filter_map(|name| name.name().cloned())
                .collect::<Vec<_>>(),
            vec![SmolStr::new("std"), SmolStr::new("io")]
        );
        assert_eq!(use_stmt.module, "core");

        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();

        let Stmt::Expr(call_expr) = &body.stmts[body.root_stmts[0]] else {
            panic!("Expected call expr stmt");
        };
        let Expr::Call { args, .. } = &body.exprs[*call_expr] else {
            panic!("Expected call expr");
        };
        assert_eq!(args.len(), 2, "args: {:?}", args);
        assert!(matches!(args[0], Arg::Named { .. }));
        assert!(matches!(args[1], Arg::Named { .. }));

        let Stmt::Expr(member_expr) = &body.stmts[body.root_stmts[1]] else {
            panic!("Expected member expr stmt");
        };
        let Expr::Member { member, .. } = &body.exprs[*member_expr] else {
            panic!("Expected member expr");
        };
        assert_eq!(member, "bar");

        let Stmt::Return(Some(ret_expr)) = &body.stmts[body.root_stmts[2]] else {
            panic!("Expected return stmt");
        };
        let Expr::Map(items) = &body.exprs[*ret_expr] else {
            panic!("Expected map expr");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_lower_bitwise_and_shift_ops() {
        let input = "\
fn f() -> Nothing {
    1 << 2
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Expr(expr) = stmt else {
            panic!("Expected expr stmt, got: {:?}", stmt);
        };
        let Expr::Binary { op, .. } = &body.exprs[*expr] else {
            panic!("Expected binary expr");
        };
        assert_eq!(*op, BinaryOp::Shl);
    }

    #[test]
    fn test_lower_member_assign_expr() {
        let input = "\
class Counter {
    value: Integer
    fn add(delta: Integer) -> Nothing {
        self.value += delta
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();
        let stmt = &body.stmts[body.root_stmts[0]];
        let Stmt::Expr(expr) = stmt else {
            panic!("Expected expr stmt, got: {:?}", stmt);
        };
        let Expr::Binary { lhs, op, .. } = &body.exprs[*expr] else {
            panic!("Expected binary expr");
        };
        assert_eq!(*op, BinaryOp::AddAssign);
        let Expr::Member { member, .. } = &body.exprs[*lhs] else {
            panic!("Expected member lhs");
        };
        assert_eq!(member, "value");
    }

    #[test]
    fn test_lower_kernel_function_marks_portable_lane() {
        let input = "\
kernel fn shade[T](value: Integer) -> Integer {
    return value
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];

        assert_eq!(func.name, "shade");
        assert_eq!(func.role, FunctionRole::Kernel);
        assert_eq!(func.lane(), FunctionLane::Portable);
        assert!(func.field.is_none());
        assert_eq!(func.type_params.len(), 1);
        assert_eq!(func.type_params[0].name, "T");
    }

    #[test]
    fn test_lower_plain_function_and_system_keep_host_lane() {
        let input = "\
fn helper() -> Integer {
    return 1
}

system tick[stage=fixed, reads=[Clock], writes=[FrameClock]]() -> Nothing {
    return nothing
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        let helper = &module.functions[Idx::new(0)];
        assert_eq!(helper.name, "helper");
        assert_eq!(helper.role, FunctionRole::Function);
        assert_eq!(helper.lane(), FunctionLane::Host);
        assert!(helper.field.is_none());

        let system = &module.functions[Idx::new(1)];
        assert_eq!(system.name, "tick");
        assert_eq!(system.role, FunctionRole::System);
        assert_eq!(system.lane(), FunctionLane::Host);
        assert!(system.field.is_none());
        let metadata = system
            .system_metadata
            .as_ref()
            .expect("system metadata should be preserved");
        assert_eq!(metadata.stage.as_deref(), Some("fixed"));
        assert_eq!(metadata.reads, vec![SmolStr::new("Clock")]);
        assert_eq!(metadata.writes, vec![SmolStr::new("FrameClock")]);
    }

    #[test]
    fn test_lower_field_declaration_marks_portable_lane_and_metadata() {
        let input = "\
field conservative distance shell(center: F32) -> F32 {
    return center
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let field = &module.functions[Idx::new(0)];
        assert_eq!(field.name, "shell");
        assert_eq!(field.role, FunctionRole::Field);
        assert_eq!(field.lane(), FunctionLane::Portable);
        assert_eq!(
            field.field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Conservative,
                kind: FieldKind::Distance,
                support: FieldSupport::Unknown,
                bounds: FieldBounds::Unknown,
                trace: GraphTraceMetadata::pessimistic(),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(field.params.len(), 1);
        assert_eq!(field.ret_type.as_ref().unwrap().name, "F32");
    }

    #[test]
    fn test_lower_field_metadata_derives_support_and_bounds_from_graph() {
        let input = "\
field exact distance sphere(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance shifted(p: Vec3) -> F32 {
    translate = vec3(1.0, 0.0, 0.0) {
        use sphere
    }
}

field exact distance scaled_literal(p: Vec3) -> F32 {
    uniform_scale = 2.0 {
        use sphere
    }
}

field exact distance ground(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 1.0, 0.0), offset = 0.0)
}

field conservative distance squashed(p: Vec3) -> F32 {
    ellipsoid(radii = vec3(1.0, 0.5, 0.75))
}

field exact distance mirrored(p: Vec3) -> F32 {
    mirror_array = vec3(0.0, 1.0, 0.0) {
        use sphere
    }
}

field exact distance repeated(p: Vec3) -> F32 {
    repeat_grid = vec3(2.0, 0.0, 0.0) {
        use sphere
    }
}

field conservative distance instanced(p: Vec3) -> F32 {
    instance_array = vec3(0.0, 0.0, 1.0) {
        use sphere
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let field = |name: &str| {
            module
                .functions
                .iter()
                .find(|(_, func)| func.name == name)
                .map(|(_, func)| func)
                .expect("field function")
        };

        assert_eq!(
            field("sphere").field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Exact,
                kind: FieldKind::Distance,
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                trace: GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true,),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(
            field("shifted").field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Exact,
                kind: FieldKind::Distance,
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                trace: GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true,),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(
            field("scaled_literal").field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Exact,
                kind: FieldKind::Distance,
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                trace: GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true,),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(
            field("ground").field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Exact,
                kind: FieldKind::Distance,
                support: FieldSupport::Unbounded,
                bounds: FieldBounds::Unbounded,
                trace: GraphTraceMetadata::exact(
                    FieldSupport::Unbounded,
                    FieldBounds::Unbounded,
                    false,
                ),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(
            field("squashed").field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Conservative,
                kind: FieldKind::Distance,
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                trace: GraphTraceMetadata::conservative(
                    FieldSupport::Bounded,
                    FieldBounds::Bounded,
                    true,
                ),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(
            field("mirrored").field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Exact,
                kind: FieldKind::Distance,
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                trace: GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true,),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(
            field("repeated").field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Exact,
                kind: FieldKind::Distance,
                support: FieldSupport::Periodic,
                bounds: FieldBounds::Unbounded,
                trace: GraphTraceMetadata::exact(
                    FieldSupport::Periodic,
                    FieldBounds::Unbounded,
                    true,
                ),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(
            field("instanced").field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Conservative,
                kind: FieldKind::Distance,
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                trace: GraphTraceMetadata {
                    class: FieldClass::Conservative,
                    support: FieldSupport::Bounded,
                    bounds: FieldBounds::Bounded,
                    can_coarse_support_pruning: false,
                    smooth_op_count: 0,
                    deform_op_count: 1,
                },
                authored_support: None,
                authored_bounds: None,
            })
        );
    }

    #[test]
    fn test_lower_field_metadata_resolves_forward_field_references() {
        let input = "\
field exact distance shifted(p: Vec3) -> F32 {
    translate = vec3(1.0, 0.0, 0.0) {
        use sphere
    }
}

field exact distance sphere(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let shifted = module
            .functions
            .iter()
            .find(|(_, func)| func.name == "shifted")
            .map(|(_, func)| func)
            .expect("shifted field");
        assert_eq!(
            shifted.field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Exact,
                kind: FieldKind::Distance,
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                trace: GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true,),
                authored_support: None,
                authored_bounds: None,
            })
        );
        assert_eq!(
            shifted.field_graph.as_ref().expect("field graph").trace,
            GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true)
        );
    }

    #[test]
    fn test_lower_field_metadata_uses_authored_support_for_custom_fields() {
        let input = "\
field conservative distance boxed(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    return length(p) - 0.5
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let field = module
            .functions
            .iter()
            .find(|(_, func)| func.name == "boxed")
            .map(|(_, func)| func)
            .expect("field");
        assert_eq!(
            field.field,
            Some(FieldMetadata {
                class: FieldClass::Conservative,
                kind: FieldKind::Distance,
                support: FieldSupport::Bounded,
                bounds: FieldBounds::Bounded,
                trace: GraphTraceMetadata::conservative(
                    FieldSupport::Bounded,
                    FieldBounds::Bounded,
                    true,
                ),
                authored_support: field
                    .field
                    .as_ref()
                    .and_then(|metadata| metadata.authored_support.clone()),
                authored_bounds: field
                    .field
                    .as_ref()
                    .and_then(|metadata| metadata.authored_bounds.clone()),
            })
        );
        let metadata = field.field.as_ref().expect("field metadata");
        assert!(
            metadata.authored_support.is_some(),
            "expected authored support body"
        );
        assert!(
            metadata.authored_bounds.is_some(),
            "expected authored bounds body"
        );
        match &field.field_graph.as_ref().expect("field graph").root {
            FieldExpr::Custom { .. } => {}
            other => panic!("expected custom field graph, got {other:?}"),
        }
        assert_eq!(
            field.field_graph.as_ref().expect("field graph").trace,
            GraphTraceMetadata::pessimistic(),
            "expected the inferred field graph trace to remain available for validation"
        );
    }

    #[test]
    fn test_lower_field_metadata_preserves_authored_support_and_bounds_clauses() {
        let input = "\
field conservative distance hinted(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-2.0, -2.0, -2.0),
        max=vec3(2.0, 2.0, 2.0)
    )
    return sphere(radius = 1.0)
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let field = module
            .functions
            .iter()
            .find(|(_, func)| func.name == "hinted")
            .map(|(_, func)| func)
            .expect("field function");
        let metadata = field.field.clone().expect("field metadata");
        assert!(
            metadata.authored_support.is_some(),
            "expected authored support clause"
        );
        assert!(
            metadata.authored_bounds.is_some(),
            "expected authored bounds clause"
        );
        assert!(
            metadata.trace.can_coarse_support_pruning,
            "expected authored support/bounds clauses to keep pruning enabled"
        );
        assert_eq!(metadata.class, FieldClass::Conservative);
        assert_eq!(
            field.field_graph.as_ref().expect("field graph").trace,
            GraphTraceMetadata::pessimistic(),
            "expected the inferred field graph trace to remain available for validation"
        );
    }

    #[test]
    fn test_lower_semantic_field_declaration_preserves_graph_and_emits_helper_calls() {
        let input = "\
field exact distance scene(p: Vec3) -> F32 {
    union {
        use sphere
        subtract {
            use sphere
            use sphere
        }
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let field = &module.functions[Idx::new(0)];
        let graph = field
            .field_graph
            .as_ref()
            .expect("semantic field graph should be preserved");
        assert_eq!(
            graph.trace,
            GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true)
        );
        match &graph.root {
            FieldExpr::Union { items } => {
                assert_eq!(items.len(), 2);
                match &items[0] {
                    FieldExpr::Use { target } => assert_eq!(target, "sphere"),
                    other => panic!("expected first union item to be use, got: {other:?}"),
                }
                match &items[1] {
                    FieldExpr::Subtract { left, right } => {
                        match &**left {
                            FieldExpr::Use { target } => assert_eq!(target, "sphere"),
                            other => panic!("expected subtract lhs to be use, got: {other:?}"),
                        }
                        match &**right {
                            FieldExpr::Use { target } => assert_eq!(target, "sphere"),
                            other => panic!("expected subtract rhs to be use, got: {other:?}"),
                        }
                    }
                    other => panic!("expected second union item to be subtract, got: {other:?}"),
                }
            }
            other => panic!("expected union field graph root, got: {other:?}"),
        }

        let body = field
            .body
            .as_ref()
            .expect("semantic field should lower to a body");
        assert_eq!(body.root_stmts.len(), 1);
        let Stmt::Return(Some(ret_expr)) = &body.stmts[body.root_stmts[0]] else {
            panic!("expected return stmt in semantic field body");
        };
        let Expr::Call { callee, args, .. } = &body.exprs[*ret_expr] else {
            panic!("expected call expression returned from semantic field");
        };
        assert_eq!(args.len(), 2);
        let Expr::Variable(name) = &body.exprs[*callee] else {
            panic!("expected union helper callee");
        };
        assert_eq!(name, "field_union");
    }

    #[test]
    fn test_lower_semantic_field_wrappers_preserve_graph_structure() {
        let input = "\
field exact distance scene(p: Vec3) -> F32 {
    translate = vec3(1, 0, 0) {
        mirror_array = vec3(0, 1, 0) {
            repeat_grid = vec3(2, 2, 2) {
                instance_array = vec3(0, 0, 1) {
                    use sphere
                }
            }
        }
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let field = &module.functions[Idx::new(0)];
        let graph = field
            .field_graph
            .as_ref()
            .expect("semantic field graph should be preserved");
        let FieldExpr::Translate { translate, body } = &graph.root else {
            panic!("expected translate field graph root");
        };
        assert_eq!(translate.root_stmts.len(), 1);
        let Stmt::Expr(_) = &translate.stmts[translate.root_stmts[0]] else {
            panic!("expected translate wrapper body to be a single expr stmt");
        };

        let FieldExpr::MirrorArray {
            mirror,
            body: mirror_body,
        } = &**body
        else {
            panic!("expected mirror array field graph body");
        };
        assert_eq!(mirror.root_stmts.len(), 1);
        let Stmt::Expr(_) = &mirror.stmts[mirror.root_stmts[0]] else {
            panic!("expected mirror array wrapper body to be a single expr stmt");
        };

        let FieldExpr::RepeatGrid {
            repeat,
            body: repeat_body,
        } = &**mirror_body
        else {
            panic!("expected repeat grid field graph body");
        };
        assert_eq!(repeat.root_stmts.len(), 1);
        let Stmt::Expr(_) = &repeat.stmts[repeat.root_stmts[0]] else {
            panic!("expected repeat grid wrapper body to be a single expr stmt");
        };

        let FieldExpr::InstanceArray {
            instance,
            body: instance_body,
        } = &**repeat_body
        else {
            panic!("expected instance array field graph body");
        };
        assert_eq!(instance.root_stmts.len(), 1);
        let Stmt::Expr(_) = &instance.stmts[instance.root_stmts[0]] else {
            panic!("expected instance array wrapper body to be a single expr stmt");
        };

        match &**instance_body {
            FieldExpr::Use { target } => assert_eq!(target, "sphere"),
            other => panic!("expected inner use field after wrapper chain, got: {other:?}"),
        }

        let body = field
            .body
            .as_ref()
            .expect("wrapper field should still lower to a body");
        assert_eq!(body.root_stmts.len(), 1);
        let Stmt::Return(Some(ret_expr)) = &body.stmts[body.root_stmts[0]] else {
            panic!("expected return stmt in wrapper field body");
        };
        let Expr::Call { .. } = &body.exprs[*ret_expr] else {
            panic!("expected lowered wrapper field body to still return an expression call");
        };
    }

    #[test]
    fn test_lower_legacy_field_declaration_keeps_custom_field_graph() {
        let input = "\
field exact distance sphere(p: F32) -> F32 {
    return p
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let field = &module.functions[Idx::new(0)];
        match &field
            .field_graph
            .as_ref()
            .expect("legacy field should still have a field graph")
            .root
        {
            FieldExpr::Custom { .. } => {}
            other => panic!("expected custom field graph for legacy body, got: {other:?}"),
        }
        assert_eq!(
            field
                .field_graph
                .as_ref()
                .expect("legacy field should still have a field graph")
                .trace,
            GraphTraceMetadata::pessimistic()
        );
        assert_eq!(
            field.field.clone(),
            Some(FieldMetadata {
                class: FieldClass::Exact,
                kind: FieldKind::Distance,
                support: FieldSupport::Unknown,
                bounds: FieldBounds::Unknown,
                trace: GraphTraceMetadata::pessimistic(),
                authored_support: None,
                authored_bounds: None,
            })
        );
    }

    #[test]
    fn test_lower_shape_metadata_derives_trace_from_fields_and_shapes() {
        let input = "\
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

shape sphere_shape {
    field = sphere_field
    material = surface
    payload = Payload(entity_id=u64(1), material_id=u64(2), actor=ActorHandle(id=u64(3), generation=u32(0)))
}

shape scene_shape {
    union {
        use sphere_shape
        use sphere_shape
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let shape = |name: &str| {
            module
                .shapes
                .iter()
                .find(|(_, shape)| shape.name == name)
                .map(|(_, shape)| shape)
                .expect("shape")
        };

        assert_eq!(
            shape("sphere_shape")
                .graph
                .as_ref()
                .expect("shape graph")
                .trace,
            GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true)
        );
        assert_eq!(
            shape("scene_shape")
                .graph
                .as_ref()
                .expect("shape graph")
                .trace,
            GraphTraceMetadata::exact(FieldSupport::Bounded, FieldBounds::Bounded, true)
        );
    }

    #[test]
    fn test_lower_shape_boolean_provenance_policies_are_preserved() {
        let input = "\
shape union_shape {
    union {
        provenance_policy = nearest
        use left_shape
        use right_shape
    }
}

shape intersection_shape {
    intersection {
        provenance_policy = ordered
        use left_shape
        use right_shape
    }
}

shape subtract_shape {
    subtract {
        provenance_policy = right
        use left_shape
        use cutter_shape
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        let union = module
            .shapes
            .iter()
            .find(|(_, shape)| shape.name == "union_shape")
            .map(|(_, shape)| shape)
            .expect("union shape");
        let union_graph = union.graph.as_ref().expect("union graph");
        match union_graph.provenance.as_ref().expect("union provenance") {
            ShapeProvenanceExpr::Union { provenance, items } => {
                assert_eq!(*provenance, ShapeMergeProvenancePolicy::Nearest);
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected union provenance tree, got {other:?}"),
        }

        let intersection = module
            .shapes
            .iter()
            .find(|(_, shape)| shape.name == "intersection_shape")
            .map(|(_, shape)| shape)
            .expect("intersection shape");
        let intersection_graph = intersection.graph.as_ref().expect("intersection graph");
        match intersection_graph
            .provenance
            .as_ref()
            .expect("intersection provenance")
        {
            ShapeProvenanceExpr::Intersection { provenance, items } => {
                assert_eq!(*provenance, ShapeMergeProvenancePolicy::Ordered);
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected intersection provenance tree, got {other:?}"),
        }

        let subtract = module
            .shapes
            .iter()
            .find(|(_, shape)| shape.name == "subtract_shape")
            .map(|(_, shape)| shape)
            .expect("subtract shape");
        let subtract_graph = subtract.graph.as_ref().expect("subtract graph");
        match subtract_graph
            .provenance
            .as_ref()
            .expect("subtract provenance")
        {
            ShapeProvenanceExpr::Subtract {
                provenance,
                left,
                right,
            } => {
                assert_eq!(*provenance, ShapeSubtractProvenancePolicy::Right);
                assert!(matches!(left.as_ref(), ShapeProvenanceExpr::Use { .. }));
                assert!(matches!(right.as_ref(), ShapeProvenanceExpr::Use { .. }));
            }
            other => panic!("expected subtract provenance tree, got {other:?}"),
        }
    }

    #[test]
    fn test_lower_material_declaration_marks_portable_lane() {
        let input = "\
material surface(hit: Hit3) -> Surface {
    return hit
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let material = &module.functions[Idx::new(0)];
        assert_eq!(material.name, "surface");
        assert_eq!(material.role, FunctionRole::Material);
        assert_eq!(material.lane(), FunctionLane::Portable);
        assert!(material.field.is_none());
        assert_eq!(material.params.len(), 1);
        assert_eq!(material.params[0].name, "hit");
        assert_eq!(material.ret_type.as_ref().unwrap().name, "Surface");
    }

    #[test]
    fn test_lower_index_expr_and_extended_for_headers() {
        let input = "\
fn f() -> Nothing {
    xs = [1, 2]
    m = {\"a\": 1}
    xs[0] = 3
    for value in xs with index i {
        nothing
    }
    for k, v in m {
        nothing
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = &module.functions[Idx::new(0)];
        let body = func.body.as_ref().unwrap();

        let Stmt::Expr(assign_expr) = &body.stmts[body.root_stmts[2]] else {
            panic!("Expected index assign expr");
        };
        let Expr::Binary { lhs, .. } = &body.exprs[*assign_expr] else {
            panic!("Expected binary assign");
        };
        assert!(matches!(&body.exprs[*lhs], Expr::Index { .. }));

        let Stmt::For {
            value_name,
            key_name,
            index_name,
            ..
        } = &body.stmts[body.root_stmts[3]]
        else {
            panic!("Expected for-with-index");
        };
        assert_eq!(value_name, "value");
        assert!(key_name.is_none());
        let _ = index_name;

        let Stmt::For {
            value_name,
            key_name,
            index_name,
            ..
        } = &body.stmts[body.root_stmts[4]]
        else {
            panic!("Expected map for");
        };
        assert_eq!(value_name, "v");
        assert_eq!(key_name.as_deref(), Some("k"));
        assert!(index_name.is_none());
    }
}
