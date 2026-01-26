use crate::mir::ir::{
    CallKind, CallTarget, MirFunction, MirModule, MirType, Place, Rvalue, Stmt, Terminator, Value,
};
use cranelift_codegen::ir::{
    AbiParam, Function, InstBuilder, MemFlags, Signature, StackSlotData, StackSlotKind, types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use target_lexicon::Triple;

#[derive(Debug)]
pub struct CodegenError(pub String);

fn declare_method_wrappers(
    module: &mut ObjectModule,
    mir: &MirModule,
    func_ids: &HashMap<SmolStr, cranelift_module::FuncId>,
) -> Result<HashMap<(SmolStr, SmolStr), cranelift_module::FuncId>, CodegenError> {
    let mut wrappers = HashMap::new();
    let ptr_ty = module.target_config().pointer_type();
    for class in &mir.classes {
        for method in &class.methods {
            let wrapper_name = format!("__wr_method_{}_{}", class.name, method.name);
            let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty, ptr_ty], &[types::I64]);
            let wrapper_id = module
                .declare_function(&wrapper_name, Linkage::Local, &sig)
                .map_err(|err| CodegenError(format!("declare wrapper failed: {err}")))?;
            let target_id = func_ids
                .get(&method.func)
                .copied()
                .ok_or_else(|| CodegenError(format!("missing method function {}", method.func)))?;
            let mut ctx = module.make_context();
            ctx.func = Function::with_name_signature(
                cranelift_codegen::ir::UserFuncName::user(0, wrapper_id.as_u32()),
                sig,
            );
            let mut fb_ctx = FunctionBuilderContext::new();
            {
                let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
                let entry = builder.create_block();
                builder.append_block_params_for_function_params(entry);
                builder.switch_to_block(entry);
                let argc = builder.block_params(entry)[0];
                let argv = builder.block_params(entry)[1];
                let ptr_ty = module.target_config().pointer_type();
                let expected_minus_one = builder
                    .ins()
                    .iconst(ptr_ty, (method.arity.saturating_sub(1)) as i64);
                let needs_shift = builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    argc,
                    expected_minus_one,
                );
                let normal_block = builder.create_block();
                let shifted_block = builder.create_block();
                builder
                    .ins()
                    .brif(needs_shift, shifted_block, &[], normal_block, &[]);

                builder.switch_to_block(shifted_block);
                let mut call_args = Vec::with_capacity(method.arity);
                let nil = builder.ins().iconst(types::I64, 3);
                call_args.push(nil);
                let flags = MemFlags::new();
                for idx in 0..method.arity.saturating_sub(1) {
                    let offset = (idx * 8) as i32;
                    let val = builder.ins().load(types::I64, flags, argv, offset);
                    call_args.push(val);
                }
                let callee = module.declare_func_in_func(target_id, builder.func);
                let call = builder.ins().call(callee, &call_args);
                let result = builder.inst_results(call)[0];
                builder.ins().return_(&[result]);

                builder.switch_to_block(normal_block);
                let mut call_args = Vec::with_capacity(method.arity);
                let flags = MemFlags::new();
                for idx in 0..method.arity {
                    let offset = (idx * 8) as i32;
                    let val = builder.ins().load(types::I64, flags, argv, offset);
                    call_args.push(val);
                }
                let callee = module.declare_func_in_func(target_id, builder.func);
                let call = builder.ins().call(callee, &call_args);
                let result = builder.inst_results(call)[0];
                builder.ins().return_(&[result]);
                builder.seal_all_blocks();
                builder.finalize();
            }
            module
                .define_function(wrapper_id, &mut ctx)
                .map_err(|err| CodegenError(format!("define wrapper failed: {err}")))?;
            wrappers.insert((class.name.clone(), method.name.clone()), wrapper_id);
        }
    }
    Ok(wrappers)
}

struct RuntimeRegistry {
    funcs: HashMap<&'static str, cranelift_module::FuncId>,
    data: HashMap<String, cranelift_module::DataId>,
    data_counter: usize,
}

impl RuntimeRegistry {
    fn new() -> Self {
        Self {
            funcs: HashMap::new(),
            data: HashMap::new(),
            data_counter: 0,
        }
    }

    fn runtime_sig(
        module: &ObjectModule,
        params: &[cranelift_codegen::ir::Type],
        returns: &[cranelift_codegen::ir::Type],
    ) -> Signature {
        let mut sig = module.make_signature();
        sig.call_conv = module.target_config().default_call_conv;
        for ty in params {
            sig.params.push(AbiParam::new(*ty));
        }
        for ty in returns {
            sig.returns.push(AbiParam::new(*ty));
        }
        sig
    }

    fn get_func(
        &mut self,
        module: &mut ObjectModule,
        name: &'static str,
        sig: Signature,
    ) -> Result<cranelift_module::FuncId, CodegenError> {
        if let Some(id) = self.funcs.get(name) {
            return Ok(*id);
        }
        let id = module
            .declare_function(name, Linkage::Import, &sig)
            .map_err(|err| CodegenError(format!("declare runtime function failed: {err}")))?;
        self.funcs.insert(name, id);
        Ok(id)
    }

    fn get_string_data(
        &mut self,
        module: &mut ObjectModule,
        bytes: &[u8],
    ) -> Result<cranelift_module::DataId, CodegenError> {
        let key = String::from_utf8_lossy(bytes).to_string();
        if let Some(id) = self.data.get(&key) {
            return Ok(*id);
        }
        let name = format!("__wr_str_{}", self.data_counter);
        self.data_counter += 1;
        let id = module
            .declare_data(&name, Linkage::Local, true, false)
            .map_err(|err| CodegenError(format!("declare data failed: {err}")))?;
        let mut data_ctx = DataDescription::new();
        data_ctx.define(bytes.to_vec().into_boxed_slice());
        module
            .define_data(id, &data_ctx)
            .map_err(|err| CodegenError(format!("define data failed: {err}")))?;
        self.data.insert(key, id);
        Ok(id)
    }
}

pub fn compile_to_object(mir: &MirModule) -> Result<Vec<u8>, CodegenError> {
    let mut module = create_object_module()?;
    let func_ids = declare_functions(&mut module, mir)?;
    let method_wrappers = declare_method_wrappers(&mut module, mir, &func_ids)?;
    let mut runtime = RuntimeRegistry::new();

    for func in &mir.functions {
        let func_id = func_ids
            .get(&func.name)
            .ok_or_else(|| CodegenError(format!("missing function id for {}", func.name)))?;
        let sig = function_signature(&module, func);
        let mut ctx = module.make_context();
        ctx.func = Function::with_name_signature(
            cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32()),
            sig,
        );
        let mut fb_ctx = FunctionBuilderContext::new();
        lower_function(
            func,
            &func_ids,
            &mut ctx.func,
            &mut fb_ctx,
            &mut module,
            &mut runtime,
            &method_wrappers,
            &mir.classes,
        )?;
        module
            .define_function(*func_id, &mut ctx)
            .map_err(|err| CodegenError(format!("define_function failed: {err}")))?;
    }

    let product = module.finish();
    let obj = product
        .emit()
        .map_err(|err| CodegenError(format!("emit failed: {err}")))?;
    Ok(obj)
}

pub fn compile_to_executable(mir: &MirModule, output: &Path) -> Result<(), CodegenError> {
    let obj = compile_to_object(mir)?;
    let obj_path = temp_object_path(output);
    fs::write(&obj_path, obj).map_err(|err| CodegenError(format!("write obj failed: {err}")))?;

    let mut linker = linker_command()?;
    let cmd = &mut linker.cmd;
    cmd.arg(&obj_path).arg("-o").arg(output);
    let runtime_lib = ensure_runtime_built()?;
    cmd.arg(runtime_lib);
    if cfg!(target_os = "macos") {
        if let Some(sdk_path) = macos_sdk_path()? {
            cmd.arg("-isysroot").arg(sdk_path);
        }
        cmd.env("MACOSX_DEPLOYMENT_TARGET", "11.0");
        cmd.arg("-Wl,-w");
    }
    let output = cmd
        .output()
        .map_err(|err| linker_io_error(&linker.name, err))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.trim().is_empty() {
            "linker failed".to_string()
        } else {
            format!("linker failed: {}", stderr.trim())
        };
        return Err(CodegenError(message));
    }

    Ok(())
}

fn ensure_runtime_built() -> Result<PathBuf, CodegenError> {
    let lib_name = if cfg!(target_os = "windows") {
        "wrela_runtime.lib"
    } else {
        "libwrela_runtime.a"
    };

    // 1. Check WRELA_RUNTIME_PATH env var
    if let Ok(path) = env::var("WRELA_RUNTIME_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    // 2. Check relative to current executable (Distribution mode)
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // bin/wrela -> lib/libwrela_runtime.a
            let lib_path = exe_dir.parent().map(|p| p.join("lib").join(lib_name));
            if let Some(path) = lib_path {
                if path.exists() {
                    return Ok(path);
                }
            }
        }
    }

    // 3. Development fallback (Cargo workspace)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.join("..").join("..");
    let profile = env::var("WRELA_RUNTIME_PROFILE").unwrap_or_else(|_| "debug".to_string());
    let lib_path = workspace_root.join("target").join(&profile).join(lib_name);
    if lib_path.exists()
        && !runtime_needs_rebuild(&lib_path, &workspace_root.join("crates").join("runtime"))
    {
        return Ok(lib_path);
    }

    let mut cmd = Command::new("cargo");
    cmd.args(["build", "-p", "wrela_runtime"]);
    if profile == "release" {
        cmd.arg("--release");
    }
    let output = cmd
        .current_dir(&workspace_root)
        .output()
        .map_err(|err| tool_io_error("cargo", err))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.trim().is_empty() {
            "runtime build failed".to_string()
        } else {
            format!("runtime build failed: {}", stderr.trim())
        };
        return Err(CodegenError(message));
    }
    if lib_path.exists() {
        return Ok(lib_path);
    }
    if let Some(fallback) = find_runtime_archive(&workspace_root.join("target").join(&profile)) {
        return Ok(fallback);
    }
    Err(CodegenError(
        "runtime library not found after build".to_string(),
    ))
}

struct LinkerCommand {
    cmd: Command,
    name: String,
}

fn linker_command() -> Result<LinkerCommand, CodegenError> {
    if cfg!(target_os = "macos") {
        if let Some(path) = macos_clang_path()? {
            return Ok(LinkerCommand {
                cmd: Command::new(&path),
                name: path,
            });
        }
    }
    Ok(LinkerCommand {
        cmd: Command::new("cc"),
        name: "cc".to_string(),
    })
}

fn macos_clang_path() -> Result<Option<String>, CodegenError> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--find", "clang"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        }
        _ => Ok(None),
    }
}

fn macos_sdk_path() -> Result<Option<String>, CodegenError> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            }
        }
        _ => Ok(None),
    }
}

fn temp_object_path(output: &Path) -> PathBuf {
    let mut path = std::env::temp_dir();
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("wr_obj");
    path.push(format!("{}.o", name));
    path
}

fn find_runtime_archive(target_dir: &Path) -> Option<PathBuf> {
    let deps = target_dir.join("deps");
    if let Ok(entries) = fs::read_dir(&deps) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy();
            if name.starts_with("libwrela_runtime") && name.ends_with(".a") {
                return Some(path);
            }
        }
    }
    None
}

fn runtime_needs_rebuild(lib_path: &Path, runtime_root: &Path) -> bool {
    let lib_time = match fs::metadata(lib_path).and_then(|meta| meta.modified()) {
        Ok(time) => time,
        Err(_) => return true,
    };
    let src_dir = runtime_root.join("src");
    if newest_mtime_in_tree(&src_dir).map_or(true, |time| time > lib_time) {
        return true;
    }
    let manifest = runtime_root.join("Cargo.toml");
    if let Some(time) = mtime(&manifest) {
        if time > lib_time {
            return true;
        }
    }
    false
}

fn newest_mtime_in_tree(root: &Path) -> Option<SystemTime> {
    let mut newest = None;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = fs::read_dir(&path).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(time) = mtime(&path) {
                newest = Some(match newest {
                    Some(current) if current > time => current,
                    _ => time,
                });
            }
        }
    }
    newest
}

fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok().and_then(|meta| meta.modified().ok())
}

fn tool_io_error(tool: &str, err: io::Error) -> CodegenError {
    if err.kind() == io::ErrorKind::NotFound {
        return CodegenError(format!(
            "failed to run '{tool}': not found (install the Rust toolchain)"
        ));
    }
    CodegenError(format!("failed to run '{tool}': {err}"))
}

fn linker_io_error(name: &str, err: io::Error) -> CodegenError {
    if err.kind() == io::ErrorKind::NotFound {
        return CodegenError(format!(
            "linker '{name}' not found (install a C toolchain such as Xcode/clang or build-essential)"
        ));
    }
    CodegenError(format!("link failed: {err}"))
}

fn create_object_module() -> Result<ObjectModule, CodegenError> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "true")
        .map_err(|err| CodegenError(format!("flags error: {err}")))?;
    let flags = settings::Flags::new(flag_builder);
    let isa_builder = cranelift_codegen::isa::lookup(Triple::host())
        .map_err(|err| CodegenError(format!("isa lookup error: {err}")))?;
    let isa = isa_builder
        .finish(flags)
        .map_err(|err| CodegenError(format!("isa finish error: {err}")))?;
    let builder = ObjectBuilder::new(isa, "wrela", cranelift_module::default_libcall_names())
        .map_err(|err| CodegenError(format!("object builder error: {err}")))?;
    Ok(ObjectModule::new(builder))
}

fn declare_functions(
    module: &mut ObjectModule,
    mir: &MirModule,
) -> Result<HashMap<SmolStr, cranelift_module::FuncId>, CodegenError> {
    let mut ids = HashMap::new();
    for func in &mir.functions {
        let sig = function_signature(module, func);
        let linkage = if func.name == "main" {
            Linkage::Export
        } else {
            Linkage::Local
        };
        let id = module
            .declare_function(func.name.as_str(), linkage, &sig)
            .map_err(|err| CodegenError(format!("declare_function failed: {err}")))?;
        ids.insert(func.name.clone(), id);
    }
    Ok(ids)
}

fn function_signature(module: &ObjectModule, func: &MirFunction) -> Signature {
    let mut sig = module.make_signature();
    sig.call_conv = module.target_config().default_call_conv;
    for _param in &func.params {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn lower_function(
    func: &MirFunction,
    func_ids: &HashMap<SmolStr, cranelift_module::FuncId>,
    clif: &mut Function,
    fb_ctx: &mut FunctionBuilderContext,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    method_wrappers: &HashMap<(SmolStr, SmolStr), cranelift_module::FuncId>,
    classes: &[crate::mir::ir::MirClassInfo],
) -> Result<(), CodegenError> {
    let mut builder = FunctionBuilder::new(clif, fb_ctx);
    let mut block_map = Vec::with_capacity(func.blocks.len());
    for _ in &func.blocks {
        block_map.push(builder.create_block());
    }

    let entry_block = block_map[func.entry.0];
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    let mut locals = HashMap::new();
    for (idx, local) in func.locals.iter().enumerate() {
        let var = Variable::from_u32(idx as u32);
        builder.declare_var(var, ty_to_clif(&local.ty)?);
        locals.insert(idx, var);
    }

    for (param_idx, local_id) in func.params.iter().enumerate() {
        let param_val = builder.block_params(entry_block)[param_idx];
        if let Some(var) = locals.get(&local_id.0) {
            builder.def_var(*var, param_val);
        }
    }

    let mut temps: HashMap<usize, cranelift_codegen::ir::Value> = HashMap::new();
    let locals_tys: Vec<MirType> = func.locals.iter().map(|local| local.ty.clone()).collect();
    let temps_tys: Vec<MirType> = func.temps.iter().map(|temp| temp.ty.clone()).collect();

    for (block_idx, block) in func.blocks.iter().enumerate() {
        let block_id = block_map[block_idx];
        builder.switch_to_block(block_id);
        if block_idx == func.entry.0 && func.name == "main" {
            emit_method_registrations(&mut builder, module, runtime, method_wrappers, classes)?;
        }
        for stmt in &block.stmts {
            lower_stmt(
                stmt,
                &mut builder,
                &locals,
                &mut temps,
                &locals_tys,
                &temps_tys,
                func_ids,
                module,
                runtime,
            )?;
        }
        lower_terminator(
            &block.terminator,
            &mut builder,
            &locals,
            &temps,
            &locals_tys,
            &temps_tys,
            &block_map,
            func_ids,
            module,
            runtime,
        )?;
    }

    builder.seal_all_blocks();
    builder.finalize();
    Ok(())
}

fn lower_stmt(
    stmt: &Stmt,
    builder: &mut FunctionBuilder,
    locals: &HashMap<usize, Variable>,
    temps: &mut HashMap<usize, cranelift_codegen::ir::Value>,
    locals_tys: &Vec<MirType>,
    temps_tys: &Vec<MirType>,
    func_ids: &HashMap<SmolStr, cranelift_module::FuncId>,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<(), CodegenError> {
    match stmt {
        Stmt::Assign { place, value, .. } => {
            let val = lower_rvalue(
                value, builder, locals, temps, locals_tys, temps_tys, func_ids, module, runtime,
            )?;
            match place {
                Place::Local(local) => {
                    if let Some(var) = locals.get(&local.0) {
                        builder.def_var(*var, val);
                    }
                }
                Place::Temp(temp) => {
                    temps.insert(temp.0, val);
                }
            }
        }
        Stmt::RcInc { value, .. } => {
            let val = lower_value(value, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_rc_inc(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            builder.ins().call(callee, &[val]);
        }
        Stmt::RcDec { value, .. } => {
            let val = lower_value(value, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_rc_dec(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            builder.ins().call(callee, &[val]);
        }
        Stmt::Await { dst, pending, .. } => {
            let pending_val = lower_value(pending, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_pending_await(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[pending_val]);
            let result = builder.inst_results(call)[0];
            assign_place(builder, locals, temps, dst, result);
        }
        Stmt::Fire { pending, .. } => {
            let pending_val = lower_value(pending, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_rc_dec(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            builder.ins().call(callee, &[pending_val]);
        }
        Stmt::SetField {
            base, field, value, ..
        } => {
            let obj = lower_value(base, builder, locals, temps, module, runtime)?;
            let val = lower_value(value, builder, locals, temps, module, runtime)?;
            let (name_ptr, len_val) =
                lower_bytes_literal(builder, module, runtime, field.as_str())?;
            let func_id = runtime_fn_class_set(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            builder.ins().call(callee, &[obj, name_ptr, len_val, val]);
        }
        Stmt::IterInit { dst, iterable, .. } => {
            let iter_val = lower_value(iterable, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_iter_init(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[iter_val]);
            let result = builder.inst_results(call)[0];
            assign_place(builder, locals, temps, dst, result);
        }
        Stmt::IterNext {
            iter,
            dst_value,
            dst_done,
            ..
        } => {
            let iter_val = lower_value(iter, builder, locals, temps, module, runtime)?;
            let ptr_ty = module.target_config().pointer_type();
            let slot_val = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8u32,
                3,
            ));
            let slot_done = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8u32,
                3,
            ));
            let addr_val = builder.ins().stack_addr(ptr_ty, slot_val, 0);
            let addr_done = builder.ins().stack_addr(ptr_ty, slot_done, 0);
            let func_id = runtime_fn_iter_next(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            builder.ins().call(callee, &[iter_val, addr_val, addr_done]);
            let loaded_val = builder.ins().load(types::I64, MemFlags::new(), addr_val, 0);
            let loaded_done = builder
                .ins()
                .load(types::I64, MemFlags::new(), addr_done, 0);
            assign_place(builder, locals, temps, dst_value, loaded_val);
            assign_place(builder, locals, temps, dst_done, loaded_done);
        }
    }
    Ok(())
}

fn lower_rvalue(
    value: &Rvalue,
    builder: &mut FunctionBuilder,
    locals: &HashMap<usize, Variable>,
    temps: &mut HashMap<usize, cranelift_codegen::ir::Value>,
    locals_tys: &Vec<MirType>,
    temps_tys: &Vec<MirType>,
    func_ids: &HashMap<SmolStr, cranelift_module::FuncId>,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    match value {
        Rvalue::Use(value) => Ok(lower_value(value, builder, locals, temps, module, runtime)?),
        Rvalue::Unary { op, operand } => {
            let v = lower_value(operand, builder, locals, temps, module, runtime)?;
            match op {
                crate::hir::UnaryOp::Neg => {
                    let ty = mir_type_of_value(operand, locals_tys, temps_tys);
                    match ty {
                        MirType::Int => {
                            let unboxed = untag_int(builder, v);
                            let neg = builder.ins().ineg(unboxed);
                            Ok(tag_int(builder, neg))
                        }
                        MirType::Float => {
                            let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                            let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                            let unbox_call = builder.ins().call(unbox_callee, &[v]);
                            let fv = builder.inst_results(unbox_call)[0];
                            let neg = builder.ins().fneg(fv);
                            let box_id = runtime_fn_box_float(module, runtime)?;
                            let box_callee = module.declare_func_in_func(box_id, builder.func);
                            let box_call = builder.ins().call(box_callee, &[neg]);
                            Ok(builder.inst_results(box_call)[0])
                        }
                        _ => {
                            let func_id = runtime_fn_num_neg(module, runtime)?;
                            let callee = module.declare_func_in_func(func_id, builder.func);
                            let call = builder.ins().call(callee, &[v]);
                            Ok(builder.inst_results(call)[0])
                        }
                    }
                }
                crate::hir::UnaryOp::BitNot => {
                    let unboxed = untag_int(builder, v);
                    let res = builder.ins().bnot(unboxed);
                    Ok(tag_int(builder, res))
                }
                crate::hir::UnaryOp::Not => {
                    let unboxed = untag_bool(builder, v);
                    let one = builder.ins().iconst(types::I64, 1);
                    let toggled = builder.ins().bxor(unboxed, one);
                    Ok(tag_bool(builder, toggled))
                }
                _ => Err(CodegenError("unsupported unary op in codegen".to_string())),
            }
        }
        Rvalue::Binary { op, lhs, rhs } => {
            let lhs_val = lower_value(lhs, builder, locals, temps, module, runtime)?;
            let rhs_val = lower_value(rhs, builder, locals, temps, module, runtime)?;
            let val = match op {
                crate::hir::BinaryOp::Add => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let res = builder.ins().iadd(l, r);
                        tag_int(builder, res)
                    } else if matches!(lty, MirType::Float) && matches!(rty, MirType::Float) {
                        let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                        let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                        let lcall = builder.ins().call(unbox_callee, &[lhs_val]);
                        let rcall = builder.ins().call(unbox_callee, &[rhs_val]);
                        let lf = builder.inst_results(lcall)[0];
                        let rf = builder.inst_results(rcall)[0];
                        let res = builder.ins().fadd(lf, rf);
                        let box_id = runtime_fn_box_float(module, runtime)?;
                        let box_callee = module.declare_func_in_func(box_id, builder.func);
                        let call = builder.ins().call(box_callee, &[res]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::String) && matches!(rty, MirType::String) {
                        let ptr_ty = module.target_config().pointer_type();
                        let (args_ptr, args_len) =
                            build_value_array(builder, ptr_ty, &[lhs_val, rhs_val]);
                        let func_id = runtime_fn_str_concat(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[args_ptr, args_len]);
                        builder.inst_results(call)[0]
                    } else {
                        let func_id = runtime_fn_num_add(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Sub => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let res = builder.ins().isub(l, r);
                        tag_int(builder, res)
                    } else if matches!(lty, MirType::Float) && matches!(rty, MirType::Float) {
                        let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                        let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                        let lcall = builder.ins().call(unbox_callee, &[lhs_val]);
                        let rcall = builder.ins().call(unbox_callee, &[rhs_val]);
                        let lf = builder.inst_results(lcall)[0];
                        let rf = builder.inst_results(rcall)[0];
                        let res = builder.ins().fsub(lf, rf);
                        let box_id = runtime_fn_box_float(module, runtime)?;
                        let box_callee = module.declare_func_in_func(box_id, builder.func);
                        let call = builder.ins().call(box_callee, &[res]);
                        builder.inst_results(call)[0]
                    } else {
                        let func_id = runtime_fn_num_sub(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Mul => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let res = builder.ins().imul(l, r);
                        tag_int(builder, res)
                    } else if matches!(lty, MirType::Float) && matches!(rty, MirType::Float) {
                        let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                        let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                        let lcall = builder.ins().call(unbox_callee, &[lhs_val]);
                        let rcall = builder.ins().call(unbox_callee, &[rhs_val]);
                        let lf = builder.inst_results(lcall)[0];
                        let rf = builder.inst_results(rcall)[0];
                        let res = builder.ins().fmul(lf, rf);
                        let box_id = runtime_fn_box_float(module, runtime)?;
                        let box_callee = module.declare_func_in_func(box_id, builder.func);
                        let call = builder.ins().call(box_callee, &[res]);
                        builder.inst_results(call)[0]
                    } else {
                        let func_id = runtime_fn_num_mul(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Div => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let zero = builder.ins().iconst(types::I64, 0);
                        let is_zero = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            r,
                            zero,
                        );
                        let trap_block = builder.create_block();
                        let cont_block = builder.create_block();
                        builder
                            .ins()
                            .brif(is_zero, trap_block, &[], cont_block, &[]);
                        builder.switch_to_block(trap_block);
                        builder
                            .ins()
                            .trap(cranelift_codegen::ir::TrapCode::INTEGER_DIVISION_BY_ZERO);
                        builder.switch_to_block(cont_block);
                        let res = builder.ins().sdiv(l, r);
                        tag_int(builder, res)
                    } else if matches!(lty, MirType::Float) && matches!(rty, MirType::Float) {
                        let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                        let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                        let lcall = builder.ins().call(unbox_callee, &[lhs_val]);
                        let rcall = builder.ins().call(unbox_callee, &[rhs_val]);
                        let lf = builder.inst_results(lcall)[0];
                        let rf = builder.inst_results(rcall)[0];
                        let res = builder.ins().fdiv(lf, rf);
                        let box_id = runtime_fn_box_float(module, runtime)?;
                        let box_callee = module.declare_func_in_func(box_id, builder.func);
                        let call = builder.ins().call(box_callee, &[res]);
                        builder.inst_results(call)[0]
                    } else {
                        let func_id = runtime_fn_num_div(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Mod => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let res = builder.ins().srem(l, r);
                        tag_int(builder, res)
                    } else {
                        let func_id = runtime_fn_num_mod(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::BitAnd => {
                    let l = untag_int(builder, lhs_val);
                    let r = untag_int(builder, rhs_val);
                    let res = builder.ins().band(l, r);
                    tag_int(builder, res)
                }
                crate::hir::BinaryOp::BitOr => {
                    let l = untag_int(builder, lhs_val);
                    let r = untag_int(builder, rhs_val);
                    let res = builder.ins().bor(l, r);
                    tag_int(builder, res)
                }
                crate::hir::BinaryOp::BitXor => {
                    let l = untag_int(builder, lhs_val);
                    let r = untag_int(builder, rhs_val);
                    let res = builder.ins().bxor(l, r);
                    tag_int(builder, res)
                }
                crate::hir::BinaryOp::Shl => {
                    let l = untag_int(builder, lhs_val);
                    let r = untag_int(builder, rhs_val);
                    let res = builder.ins().ishl(l, r);
                    tag_int(builder, res)
                }
                crate::hir::BinaryOp::Shr => {
                    let l = untag_int(builder, lhs_val);
                    let r = untag_int(builder, rhs_val);
                    let res = builder.ins().sshr(l, r);
                    tag_int(builder, res)
                }
                crate::hir::BinaryOp::Eq => runtime_eq(builder, lhs_val, rhs_val, module, runtime)?,
                crate::hir::BinaryOp::Ne => {
                    let eq = runtime_eq(builder, lhs_val, rhs_val, module, runtime)?;
                    let unboxed = untag_bool(builder, eq);
                    let one = builder.ins().iconst(types::I64, 1);
                    let toggled = builder.ins().bxor(unboxed, one);
                    tag_bool(builder, toggled)
                }
                crate::hir::BinaryOp::Lt => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
                            l,
                            r,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else if matches!(lty, MirType::Float) && matches!(rty, MirType::Float) {
                        let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                        let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                        let lcall = builder.ins().call(unbox_callee, &[lhs_val]);
                        let rcall = builder.ins().call(unbox_callee, &[rhs_val]);
                        let lf = builder.inst_results(lcall)[0];
                        let rf = builder.inst_results(rcall)[0];
                        let cmp = builder.ins().fcmp(
                            cranelift_codegen::ir::condcodes::FloatCC::LessThan,
                            lf,
                            rf,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else {
                        let func_id = runtime_fn_num_lt(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Gt => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan,
                            l,
                            r,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else if matches!(lty, MirType::Float) && matches!(rty, MirType::Float) {
                        let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                        let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                        let lcall = builder.ins().call(unbox_callee, &[lhs_val]);
                        let rcall = builder.ins().call(unbox_callee, &[rhs_val]);
                        let lf = builder.inst_results(lcall)[0];
                        let rf = builder.inst_results(rcall)[0];
                        let cmp = builder.ins().fcmp(
                            cranelift_codegen::ir::condcodes::FloatCC::GreaterThan,
                            lf,
                            rf,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else {
                        let func_id = runtime_fn_num_gt(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Le => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::SignedLessThanOrEqual,
                            l,
                            r,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else if matches!(lty, MirType::Float) && matches!(rty, MirType::Float) {
                        let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                        let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                        let lcall = builder.ins().call(unbox_callee, &[lhs_val]);
                        let rcall = builder.ins().call(unbox_callee, &[rhs_val]);
                        let lf = builder.inst_results(lcall)[0];
                        let rf = builder.inst_results(rcall)[0];
                        let cmp = builder.ins().fcmp(
                            cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual,
                            lf,
                            rf,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else {
                        let func_id = runtime_fn_num_le(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Ge => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Int) && matches!(rty, MirType::Int) {
                        let l = untag_int(builder, lhs_val);
                        let r = untag_int(builder, rhs_val);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual,
                            l,
                            r,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else if matches!(lty, MirType::Float) && matches!(rty, MirType::Float) {
                        let unbox_id = runtime_fn_unbox_float(module, runtime)?;
                        let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
                        let lcall = builder.ins().call(unbox_callee, &[lhs_val]);
                        let rcall = builder.ins().call(unbox_callee, &[rhs_val]);
                        let lf = builder.inst_results(lcall)[0];
                        let rf = builder.inst_results(rcall)[0];
                        let cmp = builder.ins().fcmp(
                            cranelift_codegen::ir::condcodes::FloatCC::GreaterThanOrEqual,
                            lf,
                            rf,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else {
                        let func_id = runtime_fn_num_ge(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::And => {
                    let l = untag_bool(builder, lhs_val);
                    let r = untag_bool(builder, rhs_val);
                    let res = builder.ins().band(l, r);
                    tag_bool(builder, res)
                }
                crate::hir::BinaryOp::Or => {
                    let l = untag_bool(builder, lhs_val);
                    let r = untag_bool(builder, rhs_val);
                    let res = builder.ins().bor(l, r);
                    tag_bool(builder, res)
                }
                crate::hir::BinaryOp::Range => {
                    let func_id = runtime_fn_range_new(module, runtime)?;
                    let callee = module.declare_func_in_func(func_id, builder.func);
                    let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                    builder.inst_results(call)[0]
                }
                _ => return Err(CodegenError("unsupported binary op in codegen".to_string())),
            };
            Ok(val)
        }
        Rvalue::ResultOk { value } => {
            let v = lower_value(value, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_result_ok(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[v]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::ResultErr { value } => {
            let v = lower_value(value, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_result_err(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[v]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::ResultIsOk { value } => {
            let v = lower_value(value, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_result_is_ok(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[v]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::ResultUnwrap { value } => {
            let v = lower_value(value, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_result_unwrap(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[v]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::Crash { value } => {
            let v = lower_value(value, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_crash(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[v]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::Call { kind, target, args } => {
            let mut call_args = Vec::with_capacity(args.len());
            for arg in args {
                call_args.push(lower_value(arg, builder, locals, temps, module, runtime)?);
            }
            match kind {
                CallKind::Sync => {
                    let func_id = match target {
                        CallTarget::Function(name) => {
                            if let Some(id) = func_ids.get(name).copied() {
                                Some(id)
                            } else {
                                match name.as_str() {
                                    "print" => Some(runtime_fn_print(module, runtime)?),
                                    "parse_int" => Some(runtime_fn_parse_int(module, runtime)?),
                                    "parse_float" => Some(runtime_fn_parse_float(module, runtime)?),
                                    "read_file" => Some(runtime_fn_read_file(module, runtime)?),
                                    "write_file" => Some(runtime_fn_write_file(module, runtime)?),
                                    _ => None,
                                }
                            }
                        }
                        CallTarget::Method {
                            receiver, method, ..
                        } => {
                            let recv =
                                lower_value(receiver, builder, locals, temps, module, runtime)?;
                            call_args.insert(0, recv);
                            func_ids.get(method).copied()
                        }
                        CallTarget::Indirect(_) => None,
                    }
                    .ok_or_else(|| CodegenError("unsupported call target".to_string()))?;
                    let callee = module.declare_func_in_func(func_id, builder.func);
                    let call_inst = builder.ins().call(callee, &call_args);
                    let results = builder.inst_results(call_inst);
                    Ok(results[0])
                }
                CallKind::Actor => {
                    let (handle, method_id) = match target {
                        CallTarget::Method {
                            receiver,
                            method_id,
                            ..
                        } => {
                            let recv =
                                lower_value(receiver, builder, locals, temps, module, runtime)?;
                            let method_id = method_id.ok_or_else(|| {
                                CodegenError("missing method id for actor call".to_string())
                            })?;
                            (recv, method_id)
                        }
                        _ => return Err(CodegenError("unsupported actor call target".to_string())),
                    };
                    let ptr_ty = module.target_config().pointer_type();
                    let (args_ptr, args_len) = build_value_array(builder, ptr_ty, &call_args);
                    let func_id = runtime_fn_actor_send(module, runtime)?;
                    let callee = module.declare_func_in_func(func_id, builder.func);
                    let method_id_val = builder.ins().iconst(types::I64, method_id as i64);
                    let call_inst = builder
                        .ins()
                        .call(callee, &[handle, method_id_val, args_len, args_ptr]);
                    let results = builder.inst_results(call_inst);
                    Ok(results[0])
                }
            }
        }
        Rvalue::Spawn { target, instance } => {
            let class_val = lower_value(target, builder, locals, temps, module, runtime)?;
            let class_id = untag_int(builder, class_val);
            let instance_val = lower_value(instance, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_actor_spawn(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[class_id, instance_val]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::ClassInit { class_id, fields } => {
            let (names_ptr, lens_ptr, count_val) =
                build_bytes_arrays(builder, module, runtime, fields)?;
            let func_id = runtime_fn_class_new(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let class_id_val = builder.ins().iconst(types::I64, *class_id as i64);
            let call = builder
                .ins()
                .call(callee, &[class_id_val, names_ptr, lens_ptr, count_val]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::GetField { base, field } => {
            let obj = lower_value(base, builder, locals, temps, module, runtime)?;
            let (name_ptr, len_val) =
                lower_bytes_literal(builder, module, runtime, field.as_str())?;
            let func_id = runtime_fn_class_get(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[obj, name_ptr, len_val]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::BuildList { items } => {
            let func_id = runtime_fn_list_new(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let len_val = builder
                .ins()
                .iconst(module.target_config().pointer_type(), items.len() as i64);
            let call = builder.ins().call(callee, &[len_val]);
            let list_val = builder.inst_results(call)[0];
            for (idx, item) in items.iter().enumerate() {
                let item_val = lower_value(item, builder, locals, temps, module, runtime)?;
                let func_id = runtime_fn_list_set(module, runtime)?;
                let callee = module.declare_func_in_func(func_id, builder.func);
                let idx_val = builder
                    .ins()
                    .iconst(module.target_config().pointer_type(), idx as i64);
                builder.ins().call(callee, &[list_val, idx_val, item_val]);
            }
            Ok(list_val)
        }
        Rvalue::BuildMap { items } => {
            let func_id = runtime_fn_map_new(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[]);
            let map_val = builder.inst_results(call)[0];
            for (key, value) in items.iter() {
                let key_val = lower_value(key, builder, locals, temps, module, runtime)?;
                let val_val = lower_value(value, builder, locals, temps, module, runtime)?;
                let func_id = runtime_fn_map_set(module, runtime)?;
                let callee = module.declare_func_in_func(func_id, builder.func);
                builder.ins().call(callee, &[map_val, key_val, val_val]);
            }
            Ok(map_val)
        }
        Rvalue::StringInterp { parts } => {
            let mut values = Vec::with_capacity(parts.len());
            for part in parts {
                match part {
                    crate::mir::ir::StringPartValue::Literal(lit) => {
                        let val = lower_string_literal(builder, module, runtime, lit.as_str())?;
                        values.push(val);
                    }
                    crate::mir::ir::StringPartValue::Value(v) => {
                        values.push(lower_value(v, builder, locals, temps, module, runtime)?);
                    }
                }
            }
            let ptr_ty = module.target_config().pointer_type();
            let (args_ptr, args_len) = build_value_array(builder, ptr_ty, &values);
            let func_id = runtime_fn_str_concat(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[args_ptr, args_len]);
            Ok(builder.inst_results(call)[0])
        }
    }
}

fn mir_type_of_value(
    value: &Value,
    locals_tys: &Vec<MirType>,
    temps_tys: &Vec<MirType>,
) -> MirType {
    match value {
        Value::Const(lit) => mir_type_of_literal(lit),
        Value::Local(local) => locals_tys.get(local.0).cloned().unwrap_or(MirType::Unknown),
        Value::Temp(temp) => temps_tys.get(temp.0).cloned().unwrap_or(MirType::Unknown),
    }
}

fn mir_type_of_literal(lit: &crate::hir::Literal) -> MirType {
    match lit {
        crate::hir::Literal::Int(_) => MirType::Int,
        crate::hir::Literal::Float(_) => MirType::Float,
        crate::hir::Literal::Bool(_) => MirType::Bool,
        crate::hir::Literal::Nil => MirType::Nil,
        crate::hir::Literal::String(_) => MirType::String,
    }
}

fn lower_terminator(
    term: &Terminator,
    builder: &mut FunctionBuilder,
    locals: &HashMap<usize, Variable>,
    temps: &HashMap<usize, cranelift_codegen::ir::Value>,
    _locals_tys: &Vec<MirType>,
    _temps_tys: &Vec<MirType>,
    block_map: &[cranelift_codegen::ir::Block],
    _func_ids: &HashMap<SmolStr, cranelift_module::FuncId>,
    _module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<(), CodegenError> {
    match term {
        Terminator::Return { value, .. } => {
            let ret = value
                .as_ref()
                .map(|value| lower_value(value, builder, locals, temps, _module, runtime))
                .transpose()?
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 3));
            builder.ins().return_(&[ret]);
        }
        Terminator::Jump { target, .. } => {
            builder.ins().jump(block_map[target.0], &[]);
        }
        Terminator::Branch {
            cond,
            then_target,
            else_target,
            ..
        } => {
            let cond = lower_value(cond, builder, locals, temps, _module, runtime)?;
            let unboxed = untag_bool(builder, cond);
            let cmp = builder.ins().icmp_imm(
                cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                unboxed,
                0,
            );
            builder.ins().brif(
                cmp,
                block_map[then_target.0],
                &[],
                block_map[else_target.0],
                &[],
            );
        }
        Terminator::Switch {
            scrutinee,
            cases,
            default,
            ..
        } => {
            let scrutinee = lower_value(scrutinee, builder, locals, temps, _module, runtime)?;
            let has_type_cases = cases
                .iter()
                .any(|(case, _)| matches!(case, crate::mir::ir::SwitchCase::Type(_)));
            let type_id_val = if has_type_cases {
                let func_id = runtime_fn_type_id(_module, runtime)?;
                let callee = _module.declare_func_in_func(func_id, builder.func);
                let call = builder.ins().call(callee, &[scrutinee]);
                Some(builder.inst_results(call)[0])
            } else {
                None
            };
            let mut next_block = builder.create_block();
            for (case, target) in cases {
                let cond = match case {
                    crate::mir::ir::SwitchCase::Literal(lit) => {
                        let const_val = lower_literal(lit, builder, _module, runtime)?;
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            scrutinee,
                            const_val,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    }
                    crate::mir::ir::SwitchCase::Type(tag) => {
                        let type_id_val = type_id_val.ok_or_else(|| {
                            CodegenError("type switch missing type id".to_string())
                        })?;
                        let tag_val = builder.ins().iconst(types::I64, tag.0 as i64);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            type_id_val,
                            tag_val,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    }
                };
                let unboxed = untag_bool(builder, cond);
                let cmp = builder.ins().icmp_imm(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                    unboxed,
                    0,
                );
                builder
                    .ins()
                    .brif(cmp, block_map[target.0], &[], next_block, &[]);
                builder.switch_to_block(next_block);
                next_block = builder.create_block();
            }
            builder.ins().jump(block_map[default.0], &[]);
        }
        Terminator::Unreachable { .. } => {
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::unwrap_user(0));
        }
    }
    Ok(())
}

fn lower_value(
    value: &Value,
    builder: &mut FunctionBuilder,
    locals: &HashMap<usize, Variable>,
    temps: &HashMap<usize, cranelift_codegen::ir::Value>,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    match value {
        Value::Const(lit) => lower_literal(lit, builder, module, runtime),
        Value::Local(local) => Ok(locals
            .get(&local.0)
            .map(|var| builder.use_var(*var))
            .unwrap_or_else(|| builder.ins().iconst(types::I64, 3))),
        Value::Temp(temp) => Ok(*temps
            .get(&temp.0)
            .unwrap_or(&builder.ins().iconst(types::I64, 3))),
    }
}

fn lower_literal(
    lit: &crate::hir::Literal,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    match lit {
        crate::hir::Literal::Int(v) => Ok(builder.ins().iconst(types::I64, ((*v as i64) << 3) | 1)),
        crate::hir::Literal::Bool(v) => Ok(builder
            .ins()
            .iconst(types::I64, ((if *v { 1 } else { 0 }) << 3) | 2)),
        crate::hir::Literal::Nil => Ok(builder.ins().iconst(types::I64, 3)),
        crate::hir::Literal::Float(v) => {
            let func_id = runtime_fn_box_float(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let val = builder.ins().f64const(*v);
            let call = builder.ins().call(callee, &[val]);
            Ok(builder.inst_results(call)[0])
        }
        crate::hir::Literal::String(s) => {
            lower_string_literal(builder, module, runtime, s.as_str())
        }
    }
}

fn bool_to_int(
    builder: &mut FunctionBuilder,
    cond: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let one = builder.ins().iconst(types::I64, 1);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().select(cond, one, zero)
}

fn runtime_fn_rc_dec(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[]);
    runtime.get_func(module, "wr_rc_dec", sig)
}

fn runtime_fn_rc_inc(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[]);
    runtime.get_func(module, "wr_rc_inc", sig)
}

fn runtime_fn_num_add(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_add", sig)
}

fn runtime_fn_num_sub(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_sub", sig)
}

fn runtime_fn_num_mul(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_mul", sig)
}

fn runtime_fn_num_div(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_div", sig)
}

fn runtime_fn_num_mod(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_mod", sig)
}

fn runtime_fn_num_neg(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_neg", sig)
}

fn runtime_fn_num_lt(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_lt", sig)
}

fn runtime_fn_num_gt(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_gt", sig)
}

fn runtime_fn_num_le(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_le", sig)
}

fn runtime_fn_num_ge(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_num_ge", sig)
}

fn runtime_fn_range_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_range_new", sig)
}

fn runtime_fn_box_float(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::F64], &[types::I64]);
    runtime.get_func(module, "wr_box_float", sig)
}

fn runtime_fn_unbox_float(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::F64]);
    runtime.get_func(module, "wr_unbox_float", sig)
}

fn runtime_fn_value_eq(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_value_eq", sig)
}

fn runtime_fn_str_from_utf8(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty, ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_str_from_utf8", sig)
}

fn runtime_fn_str_intern(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_str_intern", sig)
}

fn runtime_fn_str_concat(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty, ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_str_concat", sig)
}

fn runtime_fn_list_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_list_new", sig)
}

fn runtime_fn_list_set(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, ptr_ty, types::I64], &[]);
    runtime.get_func(module, "wr_list_set", sig)
}

fn runtime_fn_map_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_map_new", sig)
}

fn runtime_fn_map_set(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[]);
    runtime.get_func(module, "wr_map_set", sig)
}

fn runtime_fn_result_ok(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_result_ok", sig)
}

fn runtime_fn_result_err(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_result_err", sig)
}

fn runtime_fn_result_is_ok(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_result_is_ok", sig)
}

fn runtime_fn_result_unwrap(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_result_unwrap", sig)
}

fn runtime_fn_crash(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_crash", sig)
}

fn runtime_fn_iter_init(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_iter_init", sig)
}

fn runtime_fn_iter_next(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, ptr_ty, ptr_ty], &[]);
    runtime.get_func(module, "wr_iter_next", sig)
}

fn runtime_fn_pending_await(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_pending_await", sig)
}

fn runtime_fn_actor_send(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, ptr_ty, ptr_ty],
        &[types::I64],
    );
    runtime.get_func(module, "wr_actor_send", sig)
}

fn runtime_fn_actor_spawn(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_spawn", sig)
}

fn runtime_fn_class_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, ptr_ty, ptr_ty, ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_class_new", sig)
}

fn runtime_fn_class_get(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, ptr_ty, ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_class_get", sig)
}

fn runtime_fn_class_set(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, ptr_ty, ptr_ty, types::I64], &[]);
    runtime.get_func(module, "wr_class_set", sig)
}

fn runtime_fn_print(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_print", sig)
}

fn runtime_fn_parse_int(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_parse_int", sig)
}

fn runtime_fn_parse_float(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_parse_float", sig)
}

fn runtime_fn_read_file(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_read_file", sig)
}

fn runtime_fn_write_file(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_write_file", sig)
}

fn runtime_fn_type_id(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_type_id", sig)
}

fn runtime_fn_register_method(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, ptr_ty], &[]);
    runtime.get_func(module, "wr_register_method", sig)
}

fn tag_int(
    builder: &mut FunctionBuilder,
    value: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let shift = builder.ins().iconst(types::I64, 3);
    let shifted = builder.ins().ishl(value, shift);
    let tag = builder.ins().iconst(types::I64, 1);
    builder.ins().bor(shifted, tag)
}

fn untag_int(
    builder: &mut FunctionBuilder,
    value: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let shift = builder.ins().iconst(types::I64, 3);
    builder.ins().sshr(value, shift)
}

fn tag_bool(
    builder: &mut FunctionBuilder,
    value: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let shift = builder.ins().iconst(types::I64, 3);
    let shifted = builder.ins().ishl(value, shift);
    let tag = builder.ins().iconst(types::I64, 2);
    builder.ins().bor(shifted, tag)
}

fn untag_bool(
    builder: &mut FunctionBuilder,
    value: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let shift = builder.ins().iconst(types::I64, 3);
    let shifted = builder.ins().sshr(value, shift);
    let mask = builder.ins().iconst(types::I64, 1);
    builder.ins().band(shifted, mask)
}

fn assign_place(
    builder: &mut FunctionBuilder,
    locals: &HashMap<usize, Variable>,
    temps: &mut HashMap<usize, cranelift_codegen::ir::Value>,
    place: &Place,
    value: cranelift_codegen::ir::Value,
) {
    match place {
        Place::Local(local) => {
            if let Some(var) = locals.get(&local.0) {
                builder.def_var(*var, value);
            }
        }
        Place::Temp(temp) => {
            temps.insert(temp.0, value);
        }
    }
}

fn build_value_array(
    builder: &mut FunctionBuilder,
    ptr_ty: cranelift_codegen::ir::Type,
    values: &[cranelift_codegen::ir::Value],
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    if values.is_empty() {
        let zero = builder.ins().iconst(ptr_ty, 0);
        return (zero, zero);
    }
    let size = (values.len() * 8) as u32;
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, size, 3));
    let base = builder.ins().stack_addr(ptr_ty, slot, 0);
    let flags = MemFlags::new();
    for (idx, val) in values.iter().enumerate() {
        let offset = (idx * 8) as i32;
        builder.ins().store(flags, *val, base, offset);
    }
    let len_val = builder.ins().iconst(ptr_ty, values.len() as i64);
    (base, len_val)
}

fn runtime_eq(
    builder: &mut FunctionBuilder,
    lhs: cranelift_codegen::ir::Value,
    rhs: cranelift_codegen::ir::Value,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let func_id = runtime_fn_value_eq(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    let call = builder.ins().call(callee, &[lhs, rhs]);
    Ok(builder.inst_results(call)[0])
}

fn emit_method_registrations(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    wrappers: &HashMap<(SmolStr, SmolStr), cranelift_module::FuncId>,
    classes: &[crate::mir::ir::MirClassInfo],
) -> Result<(), CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let func_id = runtime_fn_register_method(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    for class in classes {
        for method in &class.methods {
            let key = (class.name.clone(), method.name.clone());
            let wrapper_id = wrappers.get(&key).ok_or_else(|| {
                CodegenError(format!(
                    "missing wrapper for {}.{}",
                    class.name, method.name
                ))
            })?;
            let class_id = builder.ins().iconst(types::I64, class.id.0 as i64);
            let method_id_val = builder.ins().iconst(types::I64, method.id as i64);
            let func_ref = module.declare_func_in_func(*wrapper_id, builder.func);
            let func_ptr = builder.ins().func_addr(ptr_ty, func_ref);
            builder
                .ins()
                .call(callee, &[class_id, method_id_val, func_ptr]);
        }
    }
    Ok(())
}

fn lower_string_literal(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    text: &str,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let (addr, len_val) = lower_bytes_literal(builder, module, runtime, text)?;
    let func_id = runtime_fn_str_from_utf8(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    let call = builder.ins().call(callee, &[addr, len_val]);
    let str_val = builder.inst_results(call)[0];
    let intern_id = runtime_fn_str_intern(module, runtime)?;
    let intern_callee = module.declare_func_in_func(intern_id, builder.func);
    let intern_call = builder.ins().call(intern_callee, &[str_val]);
    Ok(builder.inst_results(intern_call)[0])
}

fn lower_bytes_literal(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    text: &str,
) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value), CodegenError> {
    let data_id = runtime.get_string_data(module, text.as_bytes())?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let ptr_ty = module.target_config().pointer_type();
    let addr = builder.ins().symbol_value(ptr_ty, data_ref);
    let len_val = builder.ins().iconst(ptr_ty, text.len() as i64);
    Ok((addr, len_val))
}

fn build_bytes_arrays(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    fields: &[SmolStr],
) -> Result<
    (
        cranelift_codegen::ir::Value,
        cranelift_codegen::ir::Value,
        cranelift_codegen::ir::Value,
    ),
    CodegenError,
> {
    let ptr_ty = module.target_config().pointer_type();
    if fields.is_empty() {
        let zero = builder.ins().iconst(ptr_ty, 0);
        return Ok((zero, zero, zero));
    }
    let size = (fields.len() * (ptr_ty.bytes() as usize)) as u32;
    let names_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, size, 3));
    let lens_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, size, 3));
    let names_base = builder.ins().stack_addr(ptr_ty, names_slot, 0);
    let lens_base = builder.ins().stack_addr(ptr_ty, lens_slot, 0);
    let flags = MemFlags::new();
    for (idx, name) in fields.iter().enumerate() {
        let (addr, len_val) = lower_bytes_literal(builder, module, runtime, name.as_str())?;
        let offset = (idx * (ptr_ty.bytes() as usize)) as i32;
        builder.ins().store(flags, addr, names_base, offset);
        builder.ins().store(flags, len_val, lens_base, offset);
    }
    let count_val = builder.ins().iconst(ptr_ty, fields.len() as i64);
    Ok((names_base, lens_base, count_val))
}

fn ty_to_clif(ty: &MirType) -> Result<cranelift_codegen::ir::Type, CodegenError> {
    match ty {
        MirType::Int | MirType::Bool | MirType::Nil | MirType::Unknown => Ok(types::I64),
        MirType::Float => Ok(types::I64),
        MirType::String
        | MirType::Named(_)
        | MirType::Actor(_)
        | MirType::Pending(_)
        | MirType::Result(_, _) => Ok(types::I64),
    }
}
