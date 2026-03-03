use crate::hir::*;
use crate::parser::ast::{self, AstNode};
use crate::parser::kind::SyntaxKind;
use crate::parser::{SyntaxNode, SyntaxToken};
use rowan::{TextRange, TextSize};
use smol_str::SmolStr;
use std::collections::HashSet;

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
            | ast::Stmt::SystemDef(_)
            | ast::Stmt::ViewDef(_)
            | ast::Stmt::MaterialDef(_)
            | ast::Stmt::AnimDef(_)
            | ast::Stmt::GpuFuncDef(_)
            | ast::Stmt::RenderDef(_)
            | ast::Stmt::AssetsDef(_)
            | ast::Stmt::MmoDef(_)
            | ast::Stmt::AssetSpecDef(_)
            | ast::Stmt::StyleProfileDef(_)
            | ast::Stmt::GeneratorProfileDef(_)
            | ast::Stmt::QualityProfileDef(_)
            | ast::Stmt::ProvenancePolicyDef(_)
            | ast::Stmt::CharacterSpecDef(_)
            | ast::Stmt::RigSpecDef(_)
            | ast::Stmt::AnimSetSpecDef(_)
            | ast::Stmt::AudioSpecDef(_)
            | ast::Stmt::VfxSpecDef(_)
            | ast::Stmt::UiSpecDef(_)
            | ast::Stmt::WorldRecipeDef(_)
            | ast::Stmt::ClassDef(_)
            | ast::Stmt::NodeDef(_)
            | ast::Stmt::ResourceDef(_)
            | ast::Stmt::EventDef(_)
            | ast::Stmt::ThemeDef(_)
            | ast::Stmt::EnumDef(_)
            | ast::Stmt::UseStmt(_)
            | ast::Stmt::PrivateBlock(_)
            | ast::Stmt::ShaderDef(_)
            | ast::Stmt::AssetDecl(_)
            | ast::Stmt::SceneDecl(_) => {}
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
            uses: Vec::new(),
            material_declarations: Vec::new(),
            render_contracts: Vec::new(),
            gpu_functions: Vec::new(),
            asset_specs: Vec::new(),
            style_profiles: Vec::new(),
            generator_plans: Vec::new(),
            asset_build_graphs: Vec::new(),
            provenance_ledgers: Vec::new(),
            quality_gates: Vec::new(),
            shader_functions: Vec::new(),
            asset_declarations: Vec::new(),
            scene_declarations: Vec::new(),
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
                ast::Stmt::SystemDef(f) => {
                    let func = self.lower_system_def(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::ViewDef(f) => {
                    let func = self.lower_view_def(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::MaterialDef(f) => {
                    let material = self.lower_material_def(f);
                    self.module.material_declarations.push(material);
                }
                ast::Stmt::AnimDef(f) => {
                    let func = self.lower_anim_def(f);
                    self.module.functions.alloc(func);
                }
                ast::Stmt::GpuFuncDef(f) => {
                    let gpu = self.lower_gpu_func_def(f);
                    self.module.gpu_functions.push(gpu);
                }
                ast::Stmt::RenderDef(r) => {
                    let render = self.lower_render_def(r);
                    self.module.render_contracts.push(render);
                }
                ast::Stmt::AssetsDef(a) => {
                    let assets = self.lower_assets_def(a);
                    self.module.render_contracts.push(assets);
                }
                ast::Stmt::MmoDef(m) => {
                    let mmo = self.lower_mmo_def(m);
                    self.module.render_contracts.push(mmo);
                }
                ast::Stmt::AssetSpecDef(d) => {
                    let asset_spec = self.lower_asset_spec_def(d);
                    self.module.asset_specs.push(asset_spec);
                }
                ast::Stmt::StyleProfileDef(d) => {
                    let style_profile = self.lower_style_profile_def(d);
                    self.module.style_profiles.push(style_profile);
                }
                ast::Stmt::GeneratorProfileDef(d) => {
                    let generator_plan = self.lower_generator_profile_def(d);
                    self.module.generator_plans.push(generator_plan);
                }
                ast::Stmt::QualityProfileDef(d) => {
                    let quality_gate = self.lower_quality_profile_def(d);
                    self.module.quality_gates.push(quality_gate);
                }
                ast::Stmt::ProvenancePolicyDef(d) => {
                    let provenance_ledger = self.lower_provenance_policy_def(d);
                    self.module.provenance_ledgers.push(provenance_ledger);
                }
                ast::Stmt::CharacterSpecDef(d) => {
                    let asset_build_graph = self.lower_character_spec_def(d);
                    self.module.asset_build_graphs.push(asset_build_graph);
                }
                ast::Stmt::RigSpecDef(d) => {
                    let asset_build_graph = self.lower_rig_spec_def(d);
                    self.module.asset_build_graphs.push(asset_build_graph);
                }
                ast::Stmt::AnimSetSpecDef(d) => {
                    let asset_build_graph = self.lower_anim_set_spec_def(d);
                    self.module.asset_build_graphs.push(asset_build_graph);
                }
                ast::Stmt::AudioSpecDef(d) => {
                    let asset_build_graph = self.lower_audio_spec_def(d);
                    self.module.asset_build_graphs.push(asset_build_graph);
                }
                ast::Stmt::VfxSpecDef(d) => {
                    let asset_build_graph = self.lower_vfx_spec_def(d);
                    self.module.asset_build_graphs.push(asset_build_graph);
                }
                ast::Stmt::UiSpecDef(d) => {
                    let asset_build_graph = self.lower_ui_spec_def(d);
                    self.module.asset_build_graphs.push(asset_build_graph);
                }
                ast::Stmt::WorldRecipeDef(d) => {
                    let asset_build_graph = self.lower_world_recipe_def(d);
                    self.module.asset_build_graphs.push(asset_build_graph);
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
                ast::Stmt::NodeDef(c) => {
                    let class = self.lower_node_def(c);
                    self.module.classes.alloc(class);
                }
                ast::Stmt::ResourceDef(c) => {
                    let class = self.lower_resource_def(c);
                    self.module.classes.alloc(class);
                }
                ast::Stmt::EventDef(c) => {
                    let class = self.lower_event_def(c);
                    self.module.classes.alloc(class);
                }
                ast::Stmt::ThemeDef(c) => {
                    let class = self.lower_theme_def(c);
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
                            ast::Stmt::SystemDef(f) => {
                                let func = self.lower_system_def(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::ViewDef(f) => {
                                let func = self.lower_view_def(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::MaterialDef(f) => {
                                let material = self.lower_material_def(f);
                                self.module.material_declarations.push(material);
                            }
                            ast::Stmt::AnimDef(f) => {
                                let func = self.lower_anim_def(f);
                                self.module.functions.alloc(func);
                            }
                            ast::Stmt::GpuFuncDef(f) => {
                                let gpu = self.lower_gpu_func_def(f);
                                self.module.gpu_functions.push(gpu);
                            }
                            ast::Stmt::RenderDef(r) => {
                                let render = self.lower_render_def(r);
                                self.module.render_contracts.push(render);
                            }
                            ast::Stmt::AssetsDef(a) => {
                                let assets = self.lower_assets_def(a);
                                self.module.render_contracts.push(assets);
                            }
                            ast::Stmt::MmoDef(m) => {
                                let mmo = self.lower_mmo_def(m);
                                self.module.render_contracts.push(mmo);
                            }
                            ast::Stmt::AssetSpecDef(d) => {
                                let asset_spec = self.lower_asset_spec_def(d);
                                self.module.asset_specs.push(asset_spec);
                            }
                            ast::Stmt::StyleProfileDef(d) => {
                                let style_profile = self.lower_style_profile_def(d);
                                self.module.style_profiles.push(style_profile);
                            }
                            ast::Stmt::GeneratorProfileDef(d) => {
                                let generator_plan = self.lower_generator_profile_def(d);
                                self.module.generator_plans.push(generator_plan);
                            }
                            ast::Stmt::QualityProfileDef(d) => {
                                let quality_gate = self.lower_quality_profile_def(d);
                                self.module.quality_gates.push(quality_gate);
                            }
                            ast::Stmt::ProvenancePolicyDef(d) => {
                                let provenance_ledger = self.lower_provenance_policy_def(d);
                                self.module.provenance_ledgers.push(provenance_ledger);
                            }
                            ast::Stmt::CharacterSpecDef(d) => {
                                let asset_build_graph = self.lower_character_spec_def(d);
                                self.module.asset_build_graphs.push(asset_build_graph);
                            }
                            ast::Stmt::RigSpecDef(d) => {
                                let asset_build_graph = self.lower_rig_spec_def(d);
                                self.module.asset_build_graphs.push(asset_build_graph);
                            }
                            ast::Stmt::AnimSetSpecDef(d) => {
                                let asset_build_graph = self.lower_anim_set_spec_def(d);
                                self.module.asset_build_graphs.push(asset_build_graph);
                            }
                            ast::Stmt::AudioSpecDef(d) => {
                                let asset_build_graph = self.lower_audio_spec_def(d);
                                self.module.asset_build_graphs.push(asset_build_graph);
                            }
                            ast::Stmt::VfxSpecDef(d) => {
                                let asset_build_graph = self.lower_vfx_spec_def(d);
                                self.module.asset_build_graphs.push(asset_build_graph);
                            }
                            ast::Stmt::UiSpecDef(d) => {
                                let asset_build_graph = self.lower_ui_spec_def(d);
                                self.module.asset_build_graphs.push(asset_build_graph);
                            }
                            ast::Stmt::WorldRecipeDef(d) => {
                                let asset_build_graph = self.lower_world_recipe_def(d);
                                self.module.asset_build_graphs.push(asset_build_graph);
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
                            ast::Stmt::NodeDef(c) => {
                                let class = self.lower_node_def(c);
                                self.module.classes.alloc(class);
                            }
                            ast::Stmt::ResourceDef(c) => {
                                let class = self.lower_resource_def(c);
                                self.module.classes.alloc(class);
                            }
                            ast::Stmt::EventDef(c) => {
                                let class = self.lower_event_def(c);
                                self.module.classes.alloc(class);
                            }
                            ast::Stmt::ThemeDef(c) => {
                                let class = self.lower_theme_def(c);
                                self.module.classes.alloc(class);
                            }
                            ast::Stmt::EnumDef(e) => {
                                let en = self.lower_enum(e);
                                self.module.enums.alloc(en);
                            }
                            ast::Stmt::AssetDecl(d) => {
                                let asset = self.lower_asset_decl(d);
                                self.module.asset_declarations.push(asset);
                            }
                            ast::Stmt::SceneDecl(d) => {
                                let scene = self.lower_scene_decl(d);
                                self.module.scene_declarations.push(scene);
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
                ast::Stmt::ShaderDef(s) => {
                    let shader = self.lower_shader(s);
                    self.module.shader_functions.push(shader);
                }
                ast::Stmt::AssetDecl(d) => {
                    let asset = self.lower_asset_decl(d);
                    self.module.asset_declarations.push(asset);
                }
                ast::Stmt::SceneDecl(d) => {
                    let scene = self.lower_scene_decl(d);
                    self.module.scene_declarations.push(scene);
                }
                _ => {
                    // Top-level executable statements are rejected; entrypoint is `run`.
                }
            }
        }
        std::mem::take(&mut self.module)
    }

    fn lower_shader(&mut self, s: ast::ShaderDef) -> ShaderFunction {
        let name = s.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = s.name().map(|t| t.text_range());
        let params = s.params().map(|p| self.lower_param(p)).collect();
        let ret_type = s.ret_type().map(|t| self.lower_type_ref(t));

        let mut body_ctx = BodyLoweringContext::new();
        for stmt in s.statements() {
            let st = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(st);
        }

        ShaderFunction {
            name,
            name_span,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_asset_decl(&mut self, d: ast::AssetDecl) -> HirAssetDecl {
        let name = d.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let kind_str = d
            .kind_clause()
            .and_then(|c| c.value())
            .map(|t| t.text().to_string())
            .unwrap_or_default();
        let kind = match kind_str.as_str() {
            "mesh" => AssetDeclKind::Mesh,
            "texture" => AssetDeclKind::Texture,
            "audio" => AssetDeclKind::Audio,
            _ => AssetDeclKind::Mesh,
        };
        let prompt = d
            .prompt_clause()
            .and_then(|c| c.value())
            .map(|t| SmolStr::new(t.text()))
            .unwrap_or_default();
        let style = d
            .style_clause()
            .and_then(|c| c.value())
            .map(|t| SmolStr::new(t.text()));
        let negative = d
            .negative_clause()
            .and_then(|c| c.value())
            .map(|t| SmolStr::new(t.text()));
        let lod_budget = d
            .lod_budget_clause()
            .and_then(|c| c.value())
            .and_then(|t| t.text().parse::<i64>().ok());
        HirAssetDecl {
            name,
            kind,
            prompt,
            style,
            negative,
            lod_budget,
            span: d.syntax().text_range(),
        }
    }

    fn lower_scene_decl(&mut self, d: ast::SceneDecl) -> HirSceneDecl {
        let name = d.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let entities: Vec<HirSceneEntity> = d
            .entity_blocks()
            .map(|eb| {
                let entity_name = eb
                    .name()
                    .map(|t| SmolStr::new(t.text()))
                    .unwrap_or_default();
                let mesh_asset = eb
                    .mesh_clause()
                    .and_then(|c| c.value())
                    .map(|t| SmolStr::new(t.text()))
                    .unwrap_or_default();
                let position = eb
                    .position_clause()
                    .map(|c| c.values())
                    .unwrap_or([0, 0, 0]);
                let rotation = eb
                    .rotation_clause()
                    .map(|c| c.values())
                    .unwrap_or([0, 0, 0]);
                let scale = eb
                    .scale_clause()
                    .map(|c| c.values())
                    .unwrap_or([1000, 1000, 1000]);
                HirSceneEntity {
                    name: entity_name,
                    mesh_asset,
                    position,
                    rotation,
                    scale,
                }
            })
            .collect();

        let lighting = d.lighting_block().map(|lb| {
            let sun_direction = lb
                .sun_direction_clause()
                .map(|c| c.values())
                .unwrap_or([0, -1000, 0]);
            let sun_color = lb
                .sun_color_clause()
                .map(|c| c.values())
                .unwrap_or([255, 255, 255]);
            let sun_intensity = lb
                .sun_intensity_clause()
                .and_then(|c| c.value())
                .unwrap_or(1000);
            let ambient_color = lb
                .ambient_color_clause()
                .map(|c| c.values())
                .unwrap_or([50, 50, 50]);
            let ambient_intensity = lb
                .ambient_intensity_clause()
                .and_then(|c| c.value())
                .unwrap_or(500);
            HirSceneLighting {
                sun_direction,
                sun_color,
                sun_intensity,
                ambient_color,
                ambient_intensity,
            }
        });

        let camera = d.camera_block().map(|cb| {
            let mode = cb
                .mode_clause()
                .and_then(|c| c.value())
                .map(|t| SmolStr::new(t.text()))
                .unwrap_or_else(|| SmolStr::new("orbit"));
            let target = cb
                .target_clause()
                .and_then(|c| c.value())
                .map(|t| SmolStr::new(t.text()))
                .unwrap_or_default();
            let distance = cb.distance_clause().and_then(|c| c.value()).unwrap_or(5000);
            let pitch = cb.pitch_clause().and_then(|c| c.value()).unwrap_or(0);
            HirSceneCamera {
                mode,
                target,
                distance,
                pitch,
            }
        });

        HirSceneDecl {
            name,
            entities,
            lighting,
            camera,
            span: d.syntax().text_range(),
        }
    }

    fn lower_gpu_func_def(&mut self, f: ast::GpuFuncDef) -> GpuFunctionSurface {
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let params = f.params().map(|p| self.lower_param(p)).collect();
        let ret_type = f.ret_type().map(|t| self.lower_type_ref(t));
        let mut body_ctx = BodyLoweringContext::new();
        for stmt in f.statements() {
            let s = body_ctx.lower_stmt(stmt);
            body_ctx.body.root_stmts.push(s);
        }
        Self::finalize_implicit_return(&mut body_ctx.body, ret_type.as_ref());
        GpuFunctionSurface {
            name,
            name_span,
            params,
            ret_type,
            body: Some(body_ctx.body),
            span: f.syntax().text_range(),
        }
    }

    fn lower_render_def(&mut self, r: ast::RenderDef) -> RenderContract {
        let name = r.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = r.name().map(|t| t.text_range());
        let resources = r
            .resources_clause()
            .and_then(|clause| clause.value())
            .map(|token| RenderClauseValueSurface {
                value: lower_attribute_arg_value(&token),
                span: token.text_range(),
            });
        let temporal = r
            .temporal_clause()
            .and_then(|clause| clause.value())
            .map(|token| RenderClauseValueSurface {
                value: lower_attribute_arg_value(&token),
                span: token.text_range(),
            });
        let quality_tier = r
            .quality_tier_clause()
            .and_then(|clause| clause.value())
            .map(|token| RenderClauseValueSurface {
                value: lower_attribute_arg_value(&token),
                span: token.text_range(),
            });
        let budget_tags = r
            .budget_tags_clause()
            .map(|clause| RenderBudgetTagsSurface {
                tags: clause
                    .tags()
                    .map(|token| RenderClauseValueSurface {
                        value: lower_attribute_arg_value(&token),
                        span: token.text_range(),
                    })
                    .collect(),
                span: clause.syntax().text_range(),
            });
        let mut shader_modes = RenderShaderModeSet::default();
        for shader_clause in r.shader_clauses() {
            let Some(mode_token) = shader_clause.mode() else {
                continue;
            };
            let mode = lower_attribute_arg_value(&mode_token);
            let mode_span = mode_token.text_range();
            let symbol_token = shader_clause.symbol();
            let symbol = symbol_token.as_ref().map(lower_attribute_arg_value);
            match mode.as_str() {
                "generated" => {
                    if shader_modes.generated.is_some() {
                        shader_modes.duplicate_modes.push(RenderShaderModeSurface {
                            kind: RenderShaderModeKind::Generated,
                            symbol: None,
                            span: mode_span,
                        });
                    } else {
                        shader_modes.generated = Some(mode_span);
                    }
                }
                "material" => {
                    let material = RenderShaderSymbolSurface {
                        symbol: symbol.unwrap_or_default(),
                        span: symbol_token
                            .as_ref()
                            .map(|token| token.text_range())
                            .unwrap_or(mode_span),
                    };
                    if shader_modes.material.is_some() {
                        shader_modes.duplicate_modes.push(RenderShaderModeSurface {
                            kind: RenderShaderModeKind::Material,
                            symbol: Some(material.symbol.clone()),
                            span: material.span,
                        });
                    } else {
                        shader_modes.material = Some(material);
                    }
                }
                "gpu" => {
                    let gpu = RenderShaderSymbolSurface {
                        symbol: symbol.unwrap_or_default(),
                        span: symbol_token
                            .as_ref()
                            .map(|token| token.text_range())
                            .unwrap_or(mode_span),
                    };
                    if shader_modes.gpu.is_some() {
                        shader_modes.duplicate_modes.push(RenderShaderModeSurface {
                            kind: RenderShaderModeKind::Gpu,
                            symbol: Some(gpu.symbol.clone()),
                            span: gpu.span,
                        });
                    } else {
                        shader_modes.gpu = Some(gpu);
                    }
                }
                _ => {}
            }
        }
        RenderContract {
            kind: SurfaceDeclarationKind::Render,
            name,
            name_span,
            resources,
            temporal,
            quality_tier,
            budget_tags,
            preset: None,
            profile: None,
            target: None,
            shader_modes,
            overrides: RenderOverrideTiers::default(),
            assets: None,
            mmo: None,
            span: r.syntax().text_range(),
        }
    }

    fn lower_assets_def(&mut self, a: ast::AssetsDef) -> RenderContract {
        let name = a.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = a.name().map(|t| t.text_range());
        let manifest = a
            .manifest_clause()
            .and_then(|clause| clause.value())
            .map(|token| lower_attribute_arg_value(&token));
        let streaming = a
            .streaming_clause()
            .and_then(|clause| clause.value())
            .map(|token| lower_attribute_arg_value(&token));
        RenderContract {
            kind: SurfaceDeclarationKind::Assets,
            name,
            name_span,
            resources: None,
            temporal: None,
            quality_tier: None,
            budget_tags: None,
            preset: None,
            profile: None,
            target: None,
            shader_modes: RenderShaderModeSet::default(),
            overrides: RenderOverrideTiers::default(),
            assets: Some(AssetsContract {
                manifest,
                streaming,
            }),
            mmo: None,
            span: a.syntax().text_range(),
        }
    }

    fn lower_mmo_def(&mut self, m: ast::MmoDef) -> RenderContract {
        let name = m.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = m.name().map(|t| t.text_range());
        let gateway = m
            .gateway_clause()
            .and_then(|clause| clause.value())
            .map(|token| lower_attribute_arg_value(&token));
        let zone = m
            .zone_clause()
            .and_then(|clause| clause.value())
            .map(|token| lower_attribute_arg_value(&token));
        let world = m
            .world_clause()
            .and_then(|clause| clause.value())
            .map(|token| lower_attribute_arg_value(&token));
        RenderContract {
            kind: SurfaceDeclarationKind::Mmo,
            name,
            name_span,
            resources: None,
            temporal: None,
            quality_tier: None,
            budget_tags: None,
            preset: None,
            profile: None,
            target: None,
            shader_modes: RenderShaderModeSet::default(),
            overrides: RenderOverrideTiers::default(),
            assets: None,
            mmo: Some(MmoContract {
                gateway,
                zone,
                world,
            }),
            span: m.syntax().text_range(),
        }
    }

    fn lower_asset_factory_surface(
        &self,
        kind: AssetFactoryDeclarationKind,
        syntax: &SyntaxNode,
        name: Option<SyntaxToken>,
        id: Option<SyntaxToken>,
        profile: Option<SyntaxToken>,
    ) -> AssetFactoryDeclarationSurface {
        AssetFactoryDeclarationSurface {
            kind,
            name: name
                .as_ref()
                .map(|token| SmolStr::new(token.text()))
                .unwrap_or_default(),
            name_span: name.as_ref().map(|token| token.text_range()),
            id: id.as_ref().map(lower_attribute_arg_value),
            id_span: id.as_ref().map(|token| token.text_range()),
            profile: profile.as_ref().map(lower_attribute_arg_value),
            profile_span: profile.as_ref().map(|token| token.text_range()),
            span: syntax.text_range(),
        }
    }

    fn lower_asset_spec_def(&mut self, d: ast::AssetSpecDef) -> AssetSpecIRV2 {
        AssetSpecIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::AssetSpec,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_style_profile_def(&mut self, d: ast::StyleProfileDef) -> StyleProfileIRV1 {
        StyleProfileIRV1 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::StyleProfile,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_generator_profile_def(&mut self, d: ast::GeneratorProfileDef) -> GeneratorPlanIRV1 {
        GeneratorPlanIRV1 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::GeneratorProfile,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_quality_profile_def(&mut self, d: ast::QualityProfileDef) -> QualityGateIRV2 {
        QualityGateIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::QualityProfile,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_provenance_policy_def(&mut self, d: ast::ProvenancePolicyDef) -> ProvenanceLedgerIRV1 {
        ProvenanceLedgerIRV1 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::ProvenancePolicy,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_character_spec_def(&mut self, d: ast::CharacterSpecDef) -> AssetBuildGraphIRV2 {
        AssetBuildGraphIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::CharacterSpec,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_rig_spec_def(&mut self, d: ast::RigSpecDef) -> AssetBuildGraphIRV2 {
        AssetBuildGraphIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::RigSpec,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_anim_set_spec_def(&mut self, d: ast::AnimSetSpecDef) -> AssetBuildGraphIRV2 {
        AssetBuildGraphIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::AnimSetSpec,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_audio_spec_def(&mut self, d: ast::AudioSpecDef) -> AssetBuildGraphIRV2 {
        AssetBuildGraphIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::AudioSpec,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_vfx_spec_def(&mut self, d: ast::VfxSpecDef) -> AssetBuildGraphIRV2 {
        AssetBuildGraphIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::VfxSpec,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_ui_spec_def(&mut self, d: ast::UiSpecDef) -> AssetBuildGraphIRV2 {
        AssetBuildGraphIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::UiSpec,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_world_recipe_def(&mut self, d: ast::WorldRecipeDef) -> AssetBuildGraphIRV2 {
        AssetBuildGraphIRV2 {
            declaration: self.lower_asset_factory_surface(
                AssetFactoryDeclarationKind::WorldRecipe,
                d.syntax(),
                d.name(),
                d.id_clause().and_then(|clause| clause.value()),
                d.profile_clause().and_then(|clause| clause.value()),
            ),
        }
    }

    fn lower_func(&mut self, f: ast::FuncDef) -> Function {
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
            role: FunctionRole::Function,
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
            system_metadata: parse_system_metadata(f.syntax()),
            type_params,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_view_def(&mut self, f: ast::ViewDef) -> Function {
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
            role: FunctionRole::View,
            system_metadata: None,
            type_params,
            params,
            ret_type,
            body: Some(body_ctx.body),
        }
    }

    fn lower_material_def(&mut self, f: ast::MaterialDef) -> MaterialDeclarationSurface {
        let name = f.name().map(|t| SmolStr::new(t.text())).unwrap_or_default();
        let name_span = f.name().map(|t| t.text_range());
        let visibility = visibility_for_node_default(f.syntax());
        let surface_model = f
            .surface_model_clause()
            .and_then(|clause| clause.value())
            .map(|token| lower_material_clause_value(&token));
        let preset = f
            .preset_clause()
            .and_then(|clause| clause.value())
            .map(|token| lower_material_clause_value(&token));
        let textures = f
            .textures_clauses()
            .filter_map(|clause| {
                let slot = clause.slot()?;
                let value = clause.value()?;
                Some(MaterialTextureSurface {
                    slot: lower_material_clause_value(&slot),
                    value: lower_material_clause_value(&value),
                    span: clause.syntax().text_range(),
                })
            })
            .collect();
        let params = f
            .params_clauses()
            .filter_map(|clause| {
                let name = clause.name()?;
                let value = clause.value()?;
                Some(MaterialParamSurface {
                    name: lower_material_clause_value(&name),
                    value: lower_material_clause_value(&value),
                    span: clause.syntax().text_range(),
                })
            })
            .collect();
        let features = f
            .features_clauses()
            .filter_map(|clause| {
                let name = clause.name()?;
                let value = clause.value()?;
                Some(MaterialFeatureSurface {
                    name: lower_material_clause_value(&name),
                    value: lower_material_clause_value(&value),
                    span: clause.syntax().text_range(),
                })
            })
            .collect();
        let semantic_bindings: Vec<_> = f
            .semantics_clauses()
            .filter_map(|clause| {
                let key = clause.name()?;
                let value = clause.value()?;
                Some(MaterialSemanticBindingSurface {
                    key: lower_material_clause_value(&key),
                    value: lower_material_clause_value(&value),
                    span: clause.syntax().text_range(),
                })
            })
            .collect();
        let mut semantics = MaterialSemanticSurface::default();
        for binding in &semantic_bindings {
            match binding.key.value.as_str() {
                "physics_surface" => {
                    if semantics.physics_surface.is_none() {
                        semantics.physics_surface = Some(binding.value.clone());
                    }
                }
                "footstep_class" => {
                    if semantics.footstep_class.is_none() {
                        semantics.footstep_class = Some(binding.value.clone());
                    }
                }
                "impact_vfx_class" => {
                    if semantics.impact_vfx_class.is_none() {
                        semantics.impact_vfx_class = Some(binding.value.clone());
                    }
                }
                "wear_response" => {
                    if semantics.wear_response.is_none() {
                        semantics.wear_response = Some(binding.value.clone());
                    }
                }
                "friction" => {
                    if semantics.friction.is_none() {
                        semantics.friction = Some(binding.value.clone());
                    }
                }
                "restitution" => {
                    if semantics.restitution.is_none() {
                        semantics.restitution = Some(binding.value.clone());
                    }
                }
                _ => {}
            }
        }
        let render = MaterialRenderSurface {
            alpha: f
                .render_alpha_clause()
                .and_then(|clause| clause.mode())
                .map(|token| lower_material_clause_value(&token)),
            double_sided: f
                .render_double_sided_clause()
                .and_then(|clause| clause.value())
                .map(|token| lower_material_clause_value(&token)),
            receives_decals: f
                .render_receives_decals_clause()
                .and_then(|clause| clause.value())
                .map(|token| lower_material_clause_value(&token)),
        };

        MaterialDeclarationSurface {
            name,
            name_span,
            visibility,
            surface_model,
            preset,
            textures,
            params,
            features,
            semantic_bindings,
            semantics,
            render,
            span: f.syntax().text_range(),
        }
    }

    fn lower_anim_def(&mut self, f: ast::AnimDef) -> Function {
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
            role: FunctionRole::Anim,
            system_metadata: None,
            type_params,
            params,
            ret_type,
            body: Some(body_ctx.body),
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
            node_profile: None,
            type_params,
            fields,
            methods,
            implements,
        }
    }

    fn lower_node_def(&mut self, c: ast::NodeDef) -> Class {
        let node_profile = c
            .profile_clause()
            .and_then(|clause| clause.profile())
            .map(|profile| match profile {
                ast::NodeProfile::Ui => NodeProfile::Ui,
                ast::NodeProfile::World => NodeProfile::World,
                ast::NodeProfile::Canvas => NodeProfile::Canvas,
            });
        let mut class = self.lower_class_like(c, ClassRole::Node);
        class.node_profile = node_profile;
        class
    }

    fn lower_resource_def(&mut self, c: ast::ResourceDef) -> Class {
        self.lower_class_like(c, ClassRole::Resource)
    }

    fn lower_event_def(&mut self, c: ast::EventDef) -> Class {
        self.lower_class_like(c, ClassRole::Event)
    }

    fn lower_theme_def(&mut self, c: ast::ThemeDef) -> Class {
        self.lower_class_like(c, ClassRole::Theme)
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
            node_profile: None,
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

impl ClassLikeDef for ast::NodeDef {
    fn syntax(&self) -> &SyntaxNode {
        AstNode::syntax(self)
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

impl ClassLikeDef for ast::ResourceDef {
    fn syntax(&self) -> &SyntaxNode {
        AstNode::syntax(self)
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

impl ClassLikeDef for ast::EventDef {
    fn syntax(&self) -> &SyntaxNode {
        AstNode::syntax(self)
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

impl ClassLikeDef for ast::ThemeDef {
    fn syntax(&self) -> &SyntaxNode {
        AstNode::syntax(self)
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

fn lower_attributes(attributes: impl Iterator<Item = ast::Attribute>) -> Vec<AttributeAnnotation> {
    attributes
        .filter_map(|attribute| {
            let name = attribute.name()?;
            let args = attribute
                .args()
                .into_iter()
                .map(|arg| AttributeArg {
                    key: SmolStr::new(arg.key().text()),
                    key_span: Some(arg.key().text_range()),
                    value: lower_attribute_arg_value(arg.value()),
                    value_span: Some(arg.value().text_range()),
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

fn lower_material_clause_value(value: &SyntaxToken) -> MaterialClauseValueSurface {
    MaterialClauseValueSurface {
        value: lower_attribute_arg_value(value),
        span: value.text_range(),
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
                };
                Stmt::Assert { kind, expr }
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

    #[test]
    fn test_lower_node_and_material_declarations() {
        let input = "\
node Position profile ui {
    x: Integer
}
material shade {
    surface_model pbr
    preset ui
    textures albedo ui_albedo
    params roughness 0.5
    features clearcoat true
    semantics physics_surface wood
    semantics footstep_class forest
    semantics impact_vfx_class splinters
    semantics wear_response weathered
    semantics friction 0.7
    semantics restitution 0.2
    render alpha blend
    render double_sided false
    render receives_decals true
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.classes.len(), 1);
        let class = &module.classes[Idx::new(0)];
        assert_eq!(class.name, "Position");
        assert_eq!(class.role, ClassRole::Node);
        assert_eq!(class.node_profile, Some(NodeProfile::Ui));

        assert_eq!(module.material_declarations.len(), 1);
        let material = &module.material_declarations[0];
        assert_eq!(material.name, "shade");
        assert_eq!(
            material
                .surface_model
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("pbr")
        );
        assert_eq!(
            material
                .preset
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("ui")
        );
        assert_eq!(material.textures.len(), 1);
        assert_eq!(material.params.len(), 1);
        assert_eq!(material.features.len(), 1);
        assert_eq!(material.semantic_bindings.len(), 6);
        assert_eq!(
            material
                .semantics
                .physics_surface
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("wood")
        );
        assert_eq!(
            material
                .semantics
                .footstep_class
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("forest")
        );
        assert_eq!(
            material
                .semantics
                .impact_vfx_class
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("splinters")
        );
        assert_eq!(
            material
                .semantics
                .wear_response
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("weathered")
        );
        assert_eq!(
            material
                .semantics
                .friction
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("0.7")
        );
        assert_eq!(
            material
                .semantics
                .restitution
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("0.2")
        );
        assert_eq!(
            material
                .render
                .alpha
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("blend")
        );
        assert_eq!(
            material
                .render
                .double_sided
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("false")
        );
        assert_eq!(
            material
                .render
                .receives_decals
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("true")
        );
    }

    #[test]
    fn test_lower_render_contract_and_gpu_function_scaffolding() {
        let input = "\
render UiLane {
    resources UiAssets
    temporal reproject
    quality tier high
    budget tags ui, frame
}
gpu fn shade(vertex_id: Integer) -> String {
    return \"wgsl\"
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.render_contracts.len(), 1);
        let render = &module.render_contracts[0];
        assert_eq!(render.name, "UiLane");
        assert_eq!(
            render
                .resources
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("UiAssets")
        );
        assert_eq!(
            render
                .temporal
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("reproject")
        );
        assert_eq!(
            render
                .quality_tier
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("high")
        );
        assert_eq!(
            render.budget_tags.as_ref().map(|surface| {
                surface
                    .tags
                    .iter()
                    .map(|tag| tag.value.to_string())
                    .collect::<Vec<_>>()
            }),
            Some(vec!["ui".to_string(), "frame".to_string()])
        );
        assert!(render.preset.is_none());
        assert!(render.profile.is_none());
        assert!(render.target.is_none());
        assert!(render.shader_modes.generated.is_none());
        assert!(render.shader_modes.material.is_none());
        assert!(render.shader_modes.gpu.is_none());
        assert!(render.shader_modes.duplicate_modes.is_empty());
        assert!(render.overrides.tier0.is_none());
        assert!(render.overrides.tier1.is_none());
        assert!(render.overrides.tier2.is_none());
        assert!(render.overrides.duplicate_tiers.is_empty());

        assert_eq!(module.gpu_functions.len(), 1);
        let gpu = &module.gpu_functions[0];
        assert_eq!(gpu.name, "shade");
        assert_eq!(gpu.params.len(), 1);
        assert_eq!(gpu.params[0].name, "vertex_id");
    }

    #[test]
    fn test_lower_render_contract_without_budget_tags_keeps_empty_budget_surface() {
        let input = "\
render UiLane {
    resources UiAssets
    temporal stable
    quality tier medium
    budget tags
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let render = &module.render_contracts[0];
        assert_eq!(
            render
                .resources
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("UiAssets")
        );
        assert_eq!(
            render
                .temporal
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("stable")
        );
        assert_eq!(
            render
                .quality_tier
                .as_ref()
                .map(|surface| surface.value.as_str()),
            Some("medium")
        );
        assert_eq!(
            render
                .budget_tags
                .as_ref()
                .map(|surface| surface.tags.len()),
            Some(0)
        );
    }

    #[test]
    fn test_lower_render_contract_tracks_duplicate_shader_clauses_in_order() {
        let input = "\
render UiLane {
    resources UiAssets
    temporal stable
    quality tier medium
    shader generated
    shader material UiMaterial
    shader generated
    shader gpu ui_lane_shader
    shader material UiMaterialFallback
    shader gpu ui_lane_shader_fallback
    budget tags ui
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let render = &module.render_contracts[0];

        assert!(render.shader_modes.generated.is_some());
        assert_eq!(
            render
                .shader_modes
                .material
                .as_ref()
                .map(|surface| surface.symbol.as_str()),
            Some("UiMaterial")
        );
        assert_eq!(
            render
                .shader_modes
                .gpu
                .as_ref()
                .map(|surface| surface.symbol.as_str()),
            Some("ui_lane_shader")
        );
        assert_eq!(render.shader_modes.duplicate_modes.len(), 3);
        assert_eq!(
            render.shader_modes.duplicate_modes[0].kind,
            RenderShaderModeKind::Generated
        );
        assert!(render.shader_modes.duplicate_modes[0].symbol.is_none());
        assert_eq!(
            render.shader_modes.duplicate_modes[1].kind,
            RenderShaderModeKind::Material
        );
        assert_eq!(
            render.shader_modes.duplicate_modes[1].symbol.as_deref(),
            Some("UiMaterialFallback")
        );
        assert_eq!(
            render.shader_modes.duplicate_modes[2].kind,
            RenderShaderModeKind::Gpu
        );
        assert_eq!(
            render.shader_modes.duplicate_modes[2].symbol.as_deref(),
            Some("ui_lane_shader_fallback")
        );
    }

    #[test]
    fn test_lower_assets_and_mmo_declarations() {
        let input = "\
assets UiAssets {
    manifest web_manifest
    streaming chunked
}
mmo GlobalShard {
    gateway edge_gateway
    zone us_east_zone
    world earth_world
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        assert_eq!(module.render_contracts.len(), 2);

        let assets = &module.render_contracts[0];
        assert_eq!(assets.kind, SurfaceDeclarationKind::Assets);
        assert_eq!(assets.name, "UiAssets");
        let assets_fields = assets.assets.as_ref().expect("expected assets payload");
        assert_eq!(assets_fields.manifest.as_deref(), Some("web_manifest"));
        assert_eq!(assets_fields.streaming.as_deref(), Some("chunked"));

        let mmo = &module.render_contracts[1];
        assert_eq!(mmo.kind, SurfaceDeclarationKind::Mmo);
        assert_eq!(mmo.name, "GlobalShard");
        let mmo_fields = mmo.mmo.as_ref().expect("expected mmo payload");
        assert_eq!(mmo_fields.gateway.as_deref(), Some("edge_gateway"));
        assert_eq!(mmo_fields.zone.as_deref(), Some("us_east_zone"));
        assert_eq!(mmo_fields.world.as_deref(), Some("earth_world"));
    }

    #[test]
    fn test_lower_asset_factory_declarations() {
        let input = "\
asset_spec Assets { id assets_v2 profile balanced }
style_profile Style { id style_v1 profile fast }
generator_profile Generator { id generator_v1 profile strict }
quality_profile Quality { id quality_v2 profile balanced }
provenance_policy Provenance { id provenance_v1 profile strict }
character_spec Character { id character_v1 profile fast }
rig_spec Rig { id rig_v1 }
anim_set_spec AnimSet { id anim_set_v1 profile balanced }
audio_spec Audio { id audio_v1 }
vfx_spec Vfx { id vfx_v1 profile strict }
ui_spec Ui { id ui_v1 profile fast }
world_recipe World { id world_v1 profile balanced }
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.asset_specs.len(), 1);
        assert_eq!(module.asset_specs[0].declaration.name, "Assets");
        assert_eq!(
            module.asset_specs[0].declaration.id.as_deref(),
            Some("assets_v2")
        );
        assert_eq!(
            module.asset_specs[0].declaration.profile.as_deref(),
            Some("balanced")
        );

        assert_eq!(module.style_profiles.len(), 1);
        assert_eq!(module.style_profiles[0].declaration.name, "Style");
        assert_eq!(
            module.style_profiles[0].declaration.profile.as_deref(),
            Some("fast")
        );

        assert_eq!(module.generator_plans.len(), 1);
        assert_eq!(module.generator_plans[0].declaration.name, "Generator");
        assert_eq!(
            module.generator_plans[0].declaration.profile.as_deref(),
            Some("strict")
        );

        assert_eq!(module.quality_gates.len(), 1);
        assert_eq!(module.quality_gates[0].declaration.name, "Quality");
        assert_eq!(
            module.quality_gates[0].declaration.id.as_deref(),
            Some("quality_v2")
        );

        assert_eq!(module.provenance_ledgers.len(), 1);
        assert_eq!(module.provenance_ledgers[0].declaration.name, "Provenance");

        assert_eq!(module.asset_build_graphs.len(), 7);
        assert_eq!(
            module.asset_build_graphs[0].declaration.kind,
            AssetFactoryDeclarationKind::CharacterSpec
        );
        assert_eq!(
            module.asset_build_graphs[6].declaration.kind,
            AssetFactoryDeclarationKind::WorldRecipe
        );
    }

    #[test]
    fn test_lower_asset_factory_declarations_in_private_block() {
        let input = "\
asset_spec RootAssets { id root_assets profile fast }
private {
    asset_spec PrivateAssets { id private_assets profile strict }
    character_spec PrivateCharacter { id private_character profile balanced }
}
character_spec RootCharacter { id root_character profile fast }
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);

        assert_eq!(module.asset_specs.len(), 2);
        assert_eq!(module.asset_specs[0].declaration.name, "RootAssets");
        assert_eq!(module.asset_specs[1].declaration.name, "PrivateAssets");

        assert_eq!(module.asset_build_graphs.len(), 2);
        assert_eq!(
            module.asset_build_graphs[0].declaration.name,
            "PrivateCharacter"
        );
        assert_eq!(
            module.asset_build_graphs[1].declaration.name,
            "RootCharacter"
        );
    }

    #[test]
    fn test_hir_asset_decl_lowers() {
        let text = r#"asset keyblade_crystal {
    kind: mesh
    prompt: "crystalline keyblade with blue energy veins"
    style: "anime_fantasy"
    lod_budget: 10000
    negative: "blurry, low-poly"
}"#;
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        assert_eq!(module.asset_declarations.len(), 1);
        let asset = &module.asset_declarations[0];
        assert_eq!(asset.name, "keyblade_crystal");
        assert_eq!(asset.kind, AssetDeclKind::Mesh);
        assert_eq!(asset.prompt, "crystalline keyblade with blue energy veins");
        assert_eq!(asset.style.as_deref(), Some("anime_fantasy"));
        assert_eq!(asset.negative.as_deref(), Some("blurry, low-poly"));
        assert_eq!(asset.lod_budget, Some(10000));
    }

    #[test]
    fn test_hir_scene_entity_references_asset() {
        let text = r#"asset keyblade_crystal {
    kind: mesh
    prompt: "crystalline keyblade"
}

scene preview_arena {
    entity player_weapon {
        mesh: keyblade_crystal
        position: vec3(0, 0, 0)
        rotation: vec3(0, 1000, 0)
        scale: vec3(1000, 1000, 1000)
    }
}"#;
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        assert_eq!(module.asset_declarations.len(), 1);
        assert_eq!(module.scene_declarations.len(), 1);
        let scene = &module.scene_declarations[0];
        assert_eq!(scene.name, "preview_arena");
        assert_eq!(scene.entities.len(), 1);
        assert_eq!(scene.entities[0].name, "player_weapon");
        assert_eq!(scene.entities[0].mesh_asset, "keyblade_crystal");
    }

    #[test]
    fn test_hir_scene_invalid_asset_ref() {
        let text = r#"scene test_scene {
    entity item {
        mesh: nonexistent_asset
        position: vec3(0, 0, 0)
    }
}"#;
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diags = crate::hir::semantic::check_module(&module);
        assert!(
            diags.errors.iter().any(|e| {
                let msg = format!("{}", e);
                msg.contains("nonexistent_asset")
            }),
            "Expected error about nonexistent_asset, got: {:?}",
            diags.errors
        );
    }

    #[test]
    fn test_hir_scene_wrong_asset_kind() {
        let text = r#"asset my_sound {
    kind: audio
    prompt: "ambient forest sounds"
}

scene test_scene {
    entity item {
        mesh: my_sound
        position: vec3(0, 0, 0)
    }
}"#;
        let node = parse(text);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let diags = crate::hir::semantic::check_module(&module);
        assert!(
            diags.errors.iter().any(|e| {
                let msg = format!("{}", e);
                msg.contains("my_sound") && msg.contains("not a mesh")
            }),
            "Expected error about my_sound not being a mesh, got: {:?}",
            diags.errors
        );
    }
}
