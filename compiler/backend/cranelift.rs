use crate::hir::{CheckBinaryOp, CheckDagShapeFamily, CheckIrFunction, CheckValue, DecisionNode};
use crate::hir::{Objective, PoolSize};
use crate::mir::ir::{
    AllocKind, CallKind, CallTarget, MirFunction, MirModule, MirType, Place, Rvalue, Stmt,
    Terminator, Value,
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
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};
use target_lexicon::Triple;

const NANBOX_QNAN: u64 = 0x7ff8_0000_0000_0000;
const NANBOX_TAG_SHIFT: u64 = 49;
const NANBOX_PAYLOAD_MASK: u64 = (1u64 << NANBOX_TAG_SHIFT) - 1;
const NANBOX_TAG_MASK: u64 = 0x3 << NANBOX_TAG_SHIFT;
const NANBOX_TAG_INT: u64 = 2;
const NANBOX_TAG_IMM: u64 = 3;
const NANBOX_IMM_NIL: u64 = 0;
const NANBOX_IMM_FALSE: u64 = 1;
const NANBOX_IMM_TRUE: u64 = 2;
const NANBOX_INT_MIN: i64 = -(1i64 << (NANBOX_TAG_SHIFT - 1));
const NANBOX_INT_MAX: i64 = (1i64 << (NANBOX_TAG_SHIFT - 1)) - 1;
const RUNTIME_ABI_VERSION: i64 = 4;
const FNV1A_OFFSET_BASIS_64: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME_64: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug)]
pub struct CodegenError(pub String);

#[derive(Clone, Debug)]
struct PhiNode {
    place: Place,
    sources: Vec<(crate::mir::ir::BlockId, Value)>,
}

pub(crate) fn classify_check_shape_family(func: &CheckIrFunction) -> CheckDagShapeFamily {
    let Some(root) = func.dag.nodes.get(func.dag.root) else {
        return CheckDagShapeFamily::Generic;
    };
    match root {
        DecisionNode::Binary { op, lhs, rhs } if is_cmp_op(*op) => {
            if is_param_const_pair(&func.dag.nodes, *lhs, *rhs)
                || is_param_const_pair(&func.dag.nodes, *rhs, *lhs)
            {
                return CheckDagShapeFamily::ParamCmpConst;
            }
            if is_param_param_pair(&func.dag.nodes, *lhs, *rhs) {
                return CheckDagShapeFamily::ParamCmpParam;
            }
            CheckDagShapeFamily::Generic
        }
        DecisionNode::Binary { op, lhs, rhs }
            if matches!(op, CheckBinaryOp::And | CheckBinaryOp::Or) =>
        {
            if is_cmp_leaf(&func.dag.nodes, *lhs) && is_cmp_leaf(&func.dag.nodes, *rhs) {
                CheckDagShapeFamily::AndOrCmpPair
            } else {
                CheckDagShapeFamily::Generic
            }
        }
        _ => CheckDagShapeFamily::Generic,
    }
}

pub(crate) fn try_eval_specialized_check_family(
    func: &CheckIrFunction,
    args: &[CheckValue],
    family: CheckDagShapeFamily,
) -> Option<bool> {
    match family {
        CheckDagShapeFamily::Generic => None,
        CheckDagShapeFamily::ParamCmpConst
        | CheckDagShapeFamily::ParamCmpParam
        | CheckDagShapeFamily::AndOrCmpPair => func.dag.eval_bool(args),
    }
}

pub(crate) fn dispatch_vector_lane_stub(
    func: &CheckIrFunction,
    rows: &[Vec<CheckValue>],
    family: CheckDagShapeFamily,
    lane_width: usize,
) -> Option<Vec<Option<bool>>> {
    if !func.supports_vector_lane {
        return None;
    }
    if matches!(family, CheckDagShapeFamily::Generic) {
        return None;
    }

    let mut out = Vec::with_capacity(rows.len());
    for chunk in rows.chunks(lane_width.max(1)) {
        for row in chunk {
            let value = try_eval_specialized_check_family(func, row, family)?;
            out.push(Some(value));
        }
    }
    Some(out)
}

fn is_cmp_op(op: CheckBinaryOp) -> bool {
    matches!(
        op,
        CheckBinaryOp::Eq
            | CheckBinaryOp::Ne
            | CheckBinaryOp::Lt
            | CheckBinaryOp::Gt
            | CheckBinaryOp::Le
            | CheckBinaryOp::Ge
    )
}

fn is_param_const_pair(nodes: &[DecisionNode], lhs: usize, rhs: usize) -> bool {
    matches!(nodes.get(lhs), Some(DecisionNode::Param(_)))
        && matches!(
            nodes.get(rhs),
            Some(DecisionNode::Const(
                CheckValue::Integer(_) | CheckValue::Boolean(_)
            ))
        )
}

fn is_param_param_pair(nodes: &[DecisionNode], lhs: usize, rhs: usize) -> bool {
    matches!(nodes.get(lhs), Some(DecisionNode::Param(_)))
        && matches!(nodes.get(rhs), Some(DecisionNode::Param(_)))
}

fn is_cmp_leaf(nodes: &[DecisionNode], id: usize) -> bool {
    matches!(nodes.get(id), Some(DecisionNode::Binary { op, .. }) if is_cmp_op(*op))
}

fn nanbox_const(tag: u64, payload: u64) -> i64 {
    (NANBOX_QNAN | (tag << NANBOX_TAG_SHIFT) | (payload & NANBOX_PAYLOAD_MASK)) as i64
}

fn nanbox_nil_const() -> i64 {
    nanbox_const(NANBOX_TAG_IMM, NANBOX_IMM_NIL)
}

fn nanbox_bool_const(value: bool) -> i64 {
    nanbox_const(
        NANBOX_TAG_IMM,
        if value {
            NANBOX_IMM_TRUE
        } else {
            NANBOX_IMM_FALSE
        },
    )
}

fn stable_function_coverage_id(name: &str) -> u64 {
    let mut hash = FNV1A_OFFSET_BASIS_64;
    for byte in name.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV1A_PRIME_64);
    }
    hash
}

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
                let nil = builder.ins().iconst(types::I64, nanbox_nil_const());
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
    if std::env::var("WRELA_CODEGEN_DEBUG").is_ok() {
        eprintln!("codegen: start");
    }
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
            .map_err(|err| CodegenError(format!("define_function failed for {}: {err}", func.name)))?;
    }

    if std::env::var("WRELA_CODEGEN_DEBUG").is_ok() {
        eprintln!("codegen: finish module");
    }
    let product = module.finish();
    if std::env::var("WRELA_CODEGEN_DEBUG").is_ok() {
        eprintln!("codegen: emit object");
    }
    let obj = product
        .emit()
        .map_err(|err| CodegenError(format!("emit failed: {err}")))?;
    if std::env::var("WRELA_CODEGEN_DEBUG").is_ok() {
        eprintln!("codegen: done");
    }
    Ok(obj)
}

pub fn compile_to_executable(mir: &MirModule, output: &Path) -> Result<(), CodegenError> {
    let obj = compile_to_object(mir)?;
    if std::env::var("WRELA_SKIP_LINK").is_ok() {
        fs::write(output, obj).map_err(|err| CodegenError(format!("write obj failed: {err}")))?;
        if std::env::var("WRELA_CODEGEN_DEBUG").is_ok() {
            eprintln!("linker: skipped (WRELA_SKIP_LINK=1)");
        }
        return Ok(());
    }
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
        cmd.arg("-framework").arg("Security");
        cmd.arg("-framework").arg("CoreFoundation");
        cmd.arg("-framework").arg("SystemConfiguration");
        cmd.arg("-lc++");
    }
    if std::env::var("WRELA_LINKER_DEBUG").is_ok() {
        eprintln!("linker: {:?}", cmd);
    }
    let output = if let Ok(timeout_ms) = std::env::var("WRELA_LINK_TIMEOUT_MS") {
        let timeout_ms: u64 = timeout_ms.parse().unwrap_or(0);
        if timeout_ms == 0 {
            cmd.output()
                .map_err(|err| linker_io_error(&linker.name, err))?
        } else {
            run_linker_with_timeout(cmd, Duration::from_millis(timeout_ms), &linker.name)?
        }
    } else {
        cmd.output()
            .map_err(|err| linker_io_error(&linker.name, err))?
    };

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

fn run_linker_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
    name: &str,
) -> Result<std::process::Output, CodegenError> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| linker_io_error(name, err))?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|err| linker_io_error(name, err))? {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                let _ = out.read_to_end(&mut stdout);
            }
            if let Some(mut err) = child.stderr.take() {
                let _ = err.read_to_end(&mut stderr);
            }
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            return Err(CodegenError(format!(
                "linker timed out after {} ms",
                timeout.as_millis()
            )));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
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
                    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                    let workspace_root = manifest_dir.join("..");
                    let runtime_root = workspace_root.join("runtime");
                    if runtime_root.exists() && runtime_needs_rebuild(&path, &runtime_root) {
                        // Fall through to rebuild in dev contexts.
                    } else {
                        return Ok(path);
                    }
                }
            }
        }
    }

    // 3. Development fallback (Cargo workspace)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.join("..");
    let profile = env::var("WRELA_RUNTIME_PROFILE").unwrap_or_else(|_| "debug".to_string());
    let lib_path = workspace_root.join("target").join(&profile).join(lib_name);
    if lib_path.exists() && !runtime_needs_rebuild(&lib_path, &workspace_root.join("runtime")) {
        return Ok(lib_path);
    }

    let mut cmd = Command::new("cargo");
    cmd.args(["build", "-p", "wrela_runtime"]);
    if profile == "release" {
        cmd.arg("--release");
        let metrics_env = env::var("WRELA_RUNTIME_METRICS")
            .ok()
            .map(|v| !matches!(v.as_str(), "0" | "false" | "off"))
            .unwrap_or(false);
        if !metrics_env {
            cmd.arg("--no-default-features");
        }
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
    // The old implementation wrote to `$TMPDIR/<output_name>.o`, which collides when tests/codegen
    // compile the same basename in parallel (corrupt object during link -> runtime crashes).
    //
    // Make the object path unique and preferably colocated with the output binary.
    static OBJ_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = OBJ_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();

    let mut dir = output
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("wr_obj");
    dir.push(format!("{name}.{pid}.{seq}.o"));
    dir
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
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
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
        .set("opt_level", "speed")
        .map_err(|err| CodegenError(format!("flags error: {err}")))?;
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
    let phi_map = collect_phi_nodes(func);
    let locals_tys: Vec<MirType> = func.locals.iter().map(|local| local.ty.clone()).collect();
    let temps_tys: Vec<MirType> = func.temps.iter().map(|temp| temp.ty.clone()).collect();
    for (block_idx, block_phis) in phi_map.iter().enumerate() {
        let block_id = block_map[block_idx];
        for phi in block_phis {
            let ty = place_ty(&phi.place, &locals_tys, &temps_tys);
            builder.append_block_param(block_id, ty_to_clif(&ty)?);
        }
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
        let phi_offset = phi_map
            .get(func.entry.0)
            .map(|phis| phis.len())
            .unwrap_or(0);
        let param_val = builder.block_params(entry_block)[param_idx + phi_offset];
        if let Some(var) = locals.get(&local_id.0) {
            builder.def_var(*var, param_val);
        }
    }

    let mut temps: HashMap<usize, cranelift_codegen::ir::Value> = HashMap::new();

    for (block_idx, block) in func.blocks.iter().enumerate() {
        let block_id = block_map[block_idx];
        builder.switch_to_block(block_id);
        if let Some(phis) = phi_map.get(block_idx) {
            let params = builder.block_params(block_id).to_vec();
            for (idx, phi) in phis.iter().enumerate() {
                if let Some(param) = params.get(idx) {
                    assign_place(&mut builder, &locals, &mut temps, &phi.place, *param);
                }
            }
        }
        if block_idx == func.entry.0 {
            let coverage_id = stable_function_coverage_id(func.name.as_str());
            let coverage_fn = runtime_fn_coverage_hit(module, runtime)?;
            let coverage_callee = module.declare_func_in_func(coverage_fn, builder.func);
            let coverage_arg = builder.ins().iconst(types::I64, coverage_id as i64);
            builder.ins().call(coverage_callee, &[coverage_arg]);
            if func.name == "main" {
                emit_runtime_init_and_check(&mut builder, module, runtime)?;
                emit_method_registrations(&mut builder, module, runtime, method_wrappers, classes)?;
            }
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
            block_idx,
            &mut builder,
            &locals,
            &temps,
            &locals_tys,
            &temps_tys,
            &block_map,
            &phi_map,
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
        Stmt::Phi { .. } => {}
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
            base,
            field,
            slot,
            value,
            ..
        } => {
            let obj = lower_value(base, builder, locals, temps, module, runtime)?;
            let val = lower_value(value, builder, locals, temps, module, runtime)?;
            let (name_ptr, len_val) =
                lower_bytes_literal(builder, module, runtime, field.as_str())?;
            let func_id = runtime_fn_class_set_slot(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let slot_val = builder.ins().iconst(
                module.target_config().pointer_type(),
                slot.unwrap_or(u32::MAX) as i64,
            );
            builder
                .ins()
                .call(callee, &[obj, name_ptr, len_val, slot_val, val]);
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
                        MirType::Integer => {
                            let unboxed = untag_int(builder, module, runtime, v)?;
                            let neg = builder.ins().ineg(unboxed);
                            tag_int(builder, module, runtime, neg)
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
                    let unboxed = untag_int(builder, module, runtime, v)?;
                    let res = builder.ins().bnot(unboxed);
                    tag_int(builder, module, runtime, res)
                }
                crate::hir::UnaryOp::Not => {
                    let unboxed = untag_bool(builder, v);
                    let one = builder.ins().iconst(types::I64, 1);
                    let toggled = builder.ins().bxor(unboxed, one);
                    Ok(tag_bool(builder, toggled))
                }
                crate::hir::UnaryOp::Resolve => Ok(v),
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
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
                        let res = builder.ins().iadd(l, r);
                        tag_int(builder, module, runtime, res)?
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
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Add, module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Sub => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
                        let res = builder.ins().isub(l, r);
                        tag_int(builder, module, runtime, res)?
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
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Sub, module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Mul => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
                        let res = builder.ins().imul(l, r);
                        tag_int(builder, module, runtime, res)?
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
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Mul, module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Div => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
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
                        tag_int(builder, module, runtime, res)?
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
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Div, module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Mod => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
                        let res = builder.ins().srem(l, r);
                        tag_int(builder, module, runtime, res)?
                    } else {
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Mod, module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::BitAnd => {
                    let l = untag_int(builder, module, runtime, lhs_val)?;
                    let r = untag_int(builder, module, runtime, rhs_val)?;
                    let res = builder.ins().band(l, r);
                    tag_int(builder, module, runtime, res)?
                }
                crate::hir::BinaryOp::BitOr => {
                    let l = untag_int(builder, module, runtime, lhs_val)?;
                    let r = untag_int(builder, module, runtime, rhs_val)?;
                    let res = builder.ins().bor(l, r);
                    tag_int(builder, module, runtime, res)?
                }
                crate::hir::BinaryOp::BitXor => {
                    let l = untag_int(builder, module, runtime, lhs_val)?;
                    let r = untag_int(builder, module, runtime, rhs_val)?;
                    let res = builder.ins().bxor(l, r);
                    tag_int(builder, module, runtime, res)?
                }
                crate::hir::BinaryOp::Shl => {
                    let l = untag_int(builder, module, runtime, lhs_val)?;
                    let r = untag_int(builder, module, runtime, rhs_val)?;
                    let res = builder.ins().ishl(l, r);
                    tag_int(builder, module, runtime, res)?
                }
                crate::hir::BinaryOp::Shr => {
                    let l = untag_int(builder, module, runtime, lhs_val)?;
                    let r = untag_int(builder, module, runtime, rhs_val)?;
                    let res = builder.ins().sshr(l, r);
                    tag_int(builder, module, runtime, res)?
                }
                crate::hir::BinaryOp::Eq => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            l,
                            r,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else {
                        runtime_eq(builder, lhs_val, rhs_val, module, runtime)?
                    }
                }
                crate::hir::BinaryOp::Ne => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            l,
                            r,
                        );
                        let b = bool_to_int(builder, cmp);
                        tag_bool(builder, b)
                    } else {
                        let eq = runtime_eq(builder, lhs_val, rhs_val, module, runtime)?;
                        let unboxed = untag_bool(builder, eq);
                        let one = builder.ins().iconst(types::I64, 1);
                        let toggled = builder.ins().bxor(unboxed, one);
                        tag_bool(builder, toggled)
                    }
                }
                crate::hir::BinaryOp::Lt => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
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
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Lt, module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Gt => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
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
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Gt, module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Le => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
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
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Le, module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    }
                }
                crate::hir::BinaryOp::Ge => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                        let l = untag_int(builder, module, runtime, lhs_val)?;
                        let r = untag_int(builder, module, runtime, rhs_val)?;
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
                        let func_id =
                            runtime_fn_numeric_binary(crate::hir::BinaryOp::Ge, module, runtime)?;
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
        Rvalue::StrConcat { parts, alloc } => {
            let ptr_ty = module.target_config().pointer_type();
            let mut lowered = Vec::with_capacity(parts.len());
            for part in parts {
                lowered.push(lower_value(part, builder, locals, temps, module, runtime)?);
            }
            let (args_ptr, args_len) = build_value_array(builder, ptr_ty, &lowered);
            let func_id = match alloc {
                AllocKind::LocalTemp => runtime_fn_str_concat_local(module, runtime)?,
                AllocKind::Escaping => runtime_fn_str_concat(module, runtime)?,
            };
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[args_ptr, args_len]);
            Ok(builder.inst_results(call)[0])
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
        Rvalue::ResultErrUnwrap { value } => {
            let v = lower_value(value, builder, locals, temps, module, runtime)?;
            let func_id = runtime_fn_result_err_unwrap(module, runtime)?;
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
                    let target_name = match &target {
                        CallTarget::Function(name) => name.as_str().to_string(),
                        CallTarget::Method { method, .. } => method.as_str().to_string(),
                        CallTarget::GuardedInterface { fallback, .. } => {
                            fallback.as_str().to_string()
                        }
                        CallTarget::Indirect(_) => "<indirect>".to_string(),
                    };
                    let func_id = match target {
                        CallTarget::Function(name) => {
                            if let Some(id) = func_ids.get(name).copied() {
                                Some(id)
                            } else {
                                match name.as_str() {
                                    "__wr_print" => Some(runtime_fn_print(module, runtime)?),
                                    "assert" => Some(runtime_fn_assert(module, runtime)?),
                                    "assert_eq" => Some(runtime_fn_assert_eq(module, runtime)?),
                                    "value_deep_eq" => {
                                        Some(runtime_fn_value_deep_eq(module, runtime)?)
                                    }
                                    "identity_eq" => Some(runtime_fn_identity_eq(module, runtime)?),
                                    "assert_value_equality" => {
                                        Some(runtime_fn_assert_value_equality(module, runtime)?)
                                    }
                                    "assert_identity" => {
                                        Some(runtime_fn_assert_identity(module, runtime)?)
                                    }
                                    "__wr_assert_err" => {
                                        Some(runtime_fn_assert_err(module, runtime)?)
                                    }
                                    "__wr_log" => Some(runtime_fn_log(module, runtime)?),
                                    "__wr_log_configure" => {
                                        Some(runtime_fn_log_configure(module, runtime)?)
                                    }
                                    "__wr_list_push" => {
                                        Some(runtime_fn_list_push(module, runtime)?)
                                    }
                                    "__wr_map_new" => Some(runtime_fn_map_new(module, runtime)?),
                                    "__wr_map_get" => Some(runtime_fn_map_get(module, runtime)?),
                                    "__wr_map_len" => Some(runtime_fn_map_len(module, runtime)?),
                                    "__wr_map_set" => Some(runtime_fn_map_set(module, runtime)?),
                                    "__wr_str_len" => Some(runtime_fn_str_len(module, runtime)?),
                                    "__wr_runtime_cpu_count" => {
                                        Some(runtime_fn_runtime_cpu_count(module, runtime)?)
                                    }
                                    "__wr_reactor_new" => {
                                        Some(runtime_fn_reactor_new(module, runtime)?)
                                    }
                                    "__wr_reactor_drop" => {
                                        Some(runtime_fn_reactor_drop(module, runtime)?)
                                    }
                                    "__wr_reactor_register" => {
                                        Some(runtime_fn_reactor_register(module, runtime)?)
                                    }
                                    "__wr_reactor_deregister" => {
                                        Some(runtime_fn_reactor_deregister(module, runtime)?)
                                    }
                                    "__wr_reactor_arm_timer" => {
                                        Some(runtime_fn_reactor_arm_timer(module, runtime)?)
                                    }
                                    "__wr_task_signal_new" => {
                                        Some(runtime_fn_task_signal_new(module, runtime)?)
                                    }
                                    "__wr_task_signal_drop" => {
                                        Some(runtime_fn_task_signal_drop(module, runtime)?)
                                    }
                                    "__wr_task_unpark_one" => {
                                        Some(runtime_fn_task_unpark_one(module, runtime)?)
                                    }
                                    "__wr_task_unpark_all" => {
                                        Some(runtime_fn_task_unpark_all(module, runtime)?)
                                    }
                                    "__wr_task_epoch" => {
                                        Some(runtime_fn_task_epoch(module, runtime)?)
                                    }
                                    "__wr_atomic_i64_new" => {
                                        Some(runtime_fn_atomic_i64_new(module, runtime)?)
                                    }
                                    "__wr_atomic_i64_drop" => {
                                        Some(runtime_fn_atomic_i64_drop(module, runtime)?)
                                    }
                                    "__wr_atomic_i64_load" => {
                                        Some(runtime_fn_atomic_i64_load(module, runtime)?)
                                    }
                                    "__wr_atomic_i64_store" => {
                                        Some(runtime_fn_atomic_i64_store(module, runtime)?)
                                    }
                                    "__wr_atomic_i64_fetch_add" => {
                                        Some(runtime_fn_atomic_i64_fetch_add(module, runtime)?)
                                    }
                                    "__wr_pool_size" => {
                                        Some(runtime_fn_pool_size(module, runtime)?)
                                    }
                                    "__wr_pool_rr" => Some(runtime_fn_pool_rr(module, runtime)?),
                                    "__wr_pool_queue_len" => {
                                        Some(runtime_fn_pool_queue_len(module, runtime)?)
                                    }
                                    "__wr_actor_mailbox_len" => {
                                        Some(runtime_fn_actor_mailbox_len(module, runtime)?)
                                    }
                                    "__wr_actor_pause" => {
                                        Some(runtime_fn_actor_pause(module, runtime)?)
                                    }
                                    "__wr_actor_resume" => {
                                        Some(runtime_fn_actor_resume(module, runtime)?)
                                    }
                                    "__wr_actor_pause_wait" => {
                                        Some(runtime_fn_actor_pause_wait(module, runtime)?)
                                    }
                                    "__wr_actor_fire_burst_begin" => {
                                        Some(runtime_fn_actor_fire_burst_begin(module, runtime)?)
                                    }
                                    "__wr_actor_fire_burst_end" => {
                                        Some(runtime_fn_actor_fire_burst_end(module, runtime)?)
                                    }
                                    "__wr_actor_fire_burst_abort" => {
                                        Some(runtime_fn_actor_fire_burst_abort(module, runtime)?)
                                    }
                                    "__wr_metrics_get" => {
                                        Some(runtime_fn_metrics_get(module, runtime)?)
                                    }
                                    "__wr_metrics_dropped_paused_id" => {
                                        Some(runtime_fn_metrics_dropped_paused_id(module, runtime)?)
                                    }
                                    "__wr_metrics_messages_dropped_id" => Some(
                                        runtime_fn_metrics_messages_dropped_id(module, runtime)?,
                                    ),
                                    "__wr_clock_ns" => Some(runtime_fn_clock_ns(module, runtime)?),
                                    "__wr_sleep_ms" => Some(runtime_fn_sleep_ms(module, runtime)?),
                                    "__wr_env_get" => Some(runtime_fn_env_get(module, runtime)?),
                                    "__wr_env_set" => Some(runtime_fn_env_set(module, runtime)?),
                                    "__wr_process_argv" => {
                                        Some(runtime_fn_process_argv(module, runtime)?)
                                    }
                                    "__wr_process_cwd" => {
                                        Some(runtime_fn_process_cwd(module, runtime)?)
                                    }
                                    "__wr_process_run" => {
                                        Some(runtime_fn_process_run(module, runtime)?)
                                    }
                                    "__wr_process_exit" => {
                                        Some(runtime_fn_process_exit(module, runtime)?)
                                    }
                                    "__wr_runtime_configure" => {
                                        Some(runtime_fn_runtime_configure(module, runtime)?)
                                    }
                                    "__wr_bytes_from_string" => {
                                        Some(runtime_fn_bytes_from_string(module, runtime)?)
                                    }
                                    "__wr_bytes_from_list" => {
                                        Some(runtime_fn_bytes_from_list(module, runtime)?)
                                    }
                                    "__wr_bytes_to_string" => {
                                        Some(runtime_fn_bytes_to_string(module, runtime)?)
                                    }
                                    "__wr_bytes_to_list" => {
                                        Some(runtime_fn_bytes_to_list(module, runtime)?)
                                    }
                                    "__wr_bytes_len" => {
                                        Some(runtime_fn_bytes_len(module, runtime)?)
                                    }
                                    "__wr_fs_read_bytes" => {
                                        Some(runtime_fn_fs_read_bytes(module, runtime)?)
                                    }
                                    "__wr_fs_write_bytes" => {
                                        Some(runtime_fn_fs_write_bytes(module, runtime)?)
                                    }
                                    "__wr_fs_read_dir" => {
                                        Some(runtime_fn_fs_read_dir(module, runtime)?)
                                    }
                                    "__wr_fs_metadata" => {
                                        Some(runtime_fn_fs_metadata(module, runtime)?)
                                    }
                                    "__wr_fs_mkdir_all" => {
                                        Some(runtime_fn_fs_mkdir_all(module, runtime)?)
                                    }
                                    "__wr_fs_remove_file" => {
                                        Some(runtime_fn_fs_remove_file(module, runtime)?)
                                    }
                                    "__wr_fs_remove_dir_all" => {
                                        Some(runtime_fn_fs_remove_dir_all(module, runtime)?)
                                    }
                                    "__wr_fs_rename" => {
                                        Some(runtime_fn_fs_rename(module, runtime)?)
                                    }
                                    "__wr_fs_set_executable" => {
                                        Some(runtime_fn_fs_set_executable(module, runtime)?)
                                    }
                                    "__wr_external_call" => {
                                        Some(runtime_fn_external_call(module, runtime)?)
                                    }
                                    "__wr_http_call" => {
                                        Some(runtime_fn_http_call(module, runtime)?)
                                    }
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
                        CallTarget::GuardedInterface {
                            fast_paths,
                            fallback,
                        } => {
                            let receiver = *call_args.first().ok_or_else(|| {
                                CodegenError("guarded interface call missing receiver".to_string())
                            })?;
                            let type_func = runtime_fn_type_id(module, runtime)?;
                            let type_callee = module.declare_func_in_func(type_func, builder.func);
                            let type_call = builder.ins().call(type_callee, &[receiver]);
                            let type_id = builder.inst_results(type_call)[0];

                            let result_block = builder.create_block();
                            builder.append_block_param(result_block, types::I64);
                            let fallback_block = builder.create_block();
                            let mut case_blocks = Vec::with_capacity(fast_paths.len());
                            for _ in fast_paths {
                                case_blocks.push(builder.create_block());
                            }

                            let mut next_block = builder.create_block();
                            for (idx, (tag, _func_name)) in fast_paths.iter().enumerate() {
                                let tag_val = builder.ins().iconst(types::I64, tag.0 as i64);
                                let cond = builder.ins().icmp(
                                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                                    type_id,
                                    tag_val,
                                );
                                builder
                                    .ins()
                                    .brif(cond, case_blocks[idx], &[], next_block, &[]);
                                builder.switch_to_block(next_block);
                                next_block = builder.create_block();
                            }
                            builder.ins().jump(fallback_block, &[]);

                            for (idx, (_tag, func_name)) in fast_paths.iter().enumerate() {
                                builder.switch_to_block(case_blocks[idx]);
                                let case_func_id =
                                    func_ids.get(func_name).copied().ok_or_else(|| {
                                        CodegenError(format!(
                                            "missing guarded interface fast path target: {}",
                                            func_name
                                        ))
                                    })?;
                                let case_callee =
                                    module.declare_func_in_func(case_func_id, builder.func);
                                let case_call = builder.ins().call(case_callee, &call_args);
                                let case_result = builder.inst_results(case_call)[0];
                                builder.ins().jump(result_block, &[case_result]);
                            }

                            builder.switch_to_block(fallback_block);
                            let fallback_id = func_ids.get(fallback).copied().ok_or_else(|| {
                                CodegenError(format!(
                                    "missing guarded interface fallback target: {}",
                                    fallback
                                ))
                            })?;
                            let fallback_callee =
                                module.declare_func_in_func(fallback_id, builder.func);
                            let fallback_call = builder.ins().call(fallback_callee, &call_args);
                            let fallback_result = builder.inst_results(fallback_call)[0];
                            builder.ins().jump(result_block, &[fallback_result]);

                            builder.switch_to_block(result_block);
                            return Ok(builder.block_params(result_block)[0]);
                        }
                        CallTarget::Indirect(_) => None,
                    }
                    .ok_or_else(|| {
                        CodegenError(format!("unsupported call target: {}", target_name))
                    })?;
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
        Rvalue::Spawn {
            target,
            instance,
            size,
            objective,
            config,
        } => {
            let class_val = lower_value(target, builder, locals, temps, module, runtime)?;
            let class_id = untag_int(builder, module, runtime, class_val)?;
            let instance_val = lower_value(instance, builder, locals, temps, module, runtime)?;
            let __wr_pool_size = match size {
                PoolSize::Fixed(value) => *value,
                PoolSize::Auto => -1,
            };
            let objective = match objective {
                Objective::Latency => 0,
                Objective::Throughput => 1,
                Objective::Conservation => 2,
                Objective::Balance => 3,
            };
            let func_id = runtime_fn_actor_spawn(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let pool_size_val = builder.ins().iconst(types::I64, __wr_pool_size);
            let objective_val = builder.ins().iconst(types::I64, objective);
            let mailbox_cap = config.mailbox_cap.unwrap_or(-1);
            let enqueue_timeout_ms = config.enqueue_timeout_ms.unwrap_or(-1);
            let batch_limit = config.batch_limit.unwrap_or(-1);
            let mailbox_cap_val = builder.ins().iconst(types::I64, mailbox_cap);
            let enqueue_timeout_val = builder.ins().iconst(types::I64, enqueue_timeout_ms);
            let batch_limit_val = builder.ins().iconst(types::I64, batch_limit);
            let call = builder.ins().call(
                callee,
                &[
                    class_id,
                    instance_val,
                    pool_size_val,
                    objective_val,
                    mailbox_cap_val,
                    enqueue_timeout_val,
                    batch_limit_val,
                ],
            );
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::PoolNew {
            handles,
            objective,
            min_size,
            max_size,
            weight,
            queue_cap,
        } => {
            let handles_val = lower_value(handles, builder, locals, temps, module, runtime)?;
            let objective = match objective {
                Objective::Latency => 0,
                Objective::Throughput => 1,
                Objective::Conservation => 2,
                Objective::Balance => 3,
            };
            let func_id = runtime_fn_pool_new(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let objective_val = builder.ins().iconst(types::I64, objective);
            let min_val = builder.ins().iconst(types::I64, *min_size);
            let max_val = builder.ins().iconst(types::I64, *max_size);
            let weight_val = builder.ins().iconst(types::I64, *weight);
            let queue_cap_val = builder.ins().iconst(types::I64, *queue_cap);
            let call = builder.ins().call(
                callee,
                &[
                    handles_val,
                    objective_val,
                    min_val,
                    max_val,
                    weight_val,
                    queue_cap_val,
                ],
            );
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
        Rvalue::GetField { base, field, slot } => {
            let obj = lower_value(base, builder, locals, temps, module, runtime)?;
            let (name_ptr, len_val) =
                lower_bytes_literal(builder, module, runtime, field.as_str())?;
            let func_id = runtime_fn_class_get_slot(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let slot_val = builder.ins().iconst(
                module.target_config().pointer_type(),
                slot.unwrap_or(u32::MAX) as i64,
            );
            let call = builder
                .ins()
                .call(callee, &[obj, name_ptr, len_val, slot_val]);
            Ok(builder.inst_results(call)[0])
        }
        Rvalue::BuildList { items, alloc } => {
            let func_id = match alloc {
                crate::mir::ir::AllocKind::LocalTemp => runtime_fn_list_new_local(module, runtime)?,
                crate::mir::ir::AllocKind::Escaping => runtime_fn_list_new(module, runtime)?,
            };
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
        Rvalue::BuildMap { items, alloc } => {
            let func_id = match alloc {
                crate::mir::ir::AllocKind::LocalTemp => runtime_fn_map_new_local(module, runtime)?,
                crate::mir::ir::AllocKind::Escaping => runtime_fn_map_new(module, runtime)?,
            };
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
        Rvalue::StringInterp { parts, alloc } => {
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
            let func_id = match alloc {
                crate::mir::ir::AllocKind::LocalTemp => {
                    runtime_fn_str_concat_local(module, runtime)?
                }
                crate::mir::ir::AllocKind::Escaping => runtime_fn_str_concat(module, runtime)?,
            };
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
        crate::hir::Literal::Integer(_) => MirType::Integer,
        crate::hir::Literal::Float(_) => MirType::Float,
        crate::hir::Literal::Boolean(_) => MirType::Boolean,
        crate::hir::Literal::Nil => MirType::Nil,
        crate::hir::Literal::String(_) => MirType::String,
    }
}

fn collect_phi_nodes(func: &MirFunction) -> Vec<Vec<PhiNode>> {
    let mut phi_map = vec![Vec::new(); func.blocks.len()];
    for (idx, block) in func.blocks.iter().enumerate() {
        for stmt in &block.stmts {
            if let Stmt::Phi { place, sources, .. } = stmt {
                phi_map[idx].push(PhiNode {
                    place: place.clone(),
                    sources: sources.clone(),
                });
            } else {
                break;
            }
        }
    }
    phi_map
}

fn place_ty(place: &Place, locals_tys: &[MirType], temps_tys: &[MirType]) -> MirType {
    match place {
        Place::Local(local) => locals_tys.get(local.0).cloned().unwrap_or(MirType::Unknown),
        Place::Temp(temp) => temps_tys.get(temp.0).cloned().unwrap_or(MirType::Unknown),
    }
}

fn lower_terminator(
    term: &Terminator,
    block_idx: usize,
    builder: &mut FunctionBuilder,
    locals: &HashMap<usize, Variable>,
    temps: &HashMap<usize, cranelift_codegen::ir::Value>,
    _locals_tys: &Vec<MirType>,
    _temps_tys: &Vec<MirType>,
    block_map: &[cranelift_codegen::ir::Block],
    phi_map: &[Vec<PhiNode>],
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
                .unwrap_or_else(|| builder.ins().iconst(types::I64, nanbox_nil_const()));
            builder.ins().return_(&[ret]);
        }
        Terminator::Jump { target, .. } => {
            let args = phi_args_for_target(
                block_idx, target.0, phi_map, builder, locals, temps, _module, runtime,
            )?;
            builder.ins().jump(block_map[target.0], &args);
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
            let then_args = phi_args_for_target(
                block_idx,
                then_target.0,
                phi_map,
                builder,
                locals,
                temps,
                _module,
                runtime,
            )?;
            let else_args = phi_args_for_target(
                block_idx,
                else_target.0,
                phi_map,
                builder,
                locals,
                temps,
                _module,
                runtime,
            )?;
            builder.ins().brif(
                cmp,
                block_map[then_target.0],
                &then_args,
                block_map[else_target.0],
                &else_args,
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
                let args = phi_args_for_target(
                    block_idx, target.0, phi_map, builder, locals, temps, _module, runtime,
                )?;
                builder
                    .ins()
                    .brif(cmp, block_map[target.0], &args, next_block, &[]);
                builder.switch_to_block(next_block);
                next_block = builder.create_block();
            }
            let default_args = phi_args_for_target(
                block_idx, default.0, phi_map, builder, locals, temps, _module, runtime,
            )?;
            builder.ins().jump(block_map[default.0], &default_args);
        }
        Terminator::Unreachable { .. } => {
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::unwrap_user(0));
        }
    }
    Ok(())
}

fn phi_args_for_target(
    pred_idx: usize,
    target_idx: usize,
    phi_map: &[Vec<PhiNode>],
    builder: &mut FunctionBuilder,
    locals: &HashMap<usize, Variable>,
    temps: &HashMap<usize, cranelift_codegen::ir::Value>,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<Vec<cranelift_codegen::ir::Value>, CodegenError> {
    let mut args = Vec::new();
    let Some(phis) = phi_map.get(target_idx) else {
        return Ok(args);
    };
    for phi in phis {
        let mut src = Value::Const(crate::hir::Literal::Nil);
        for (pred, value) in &phi.sources {
            if pred.0 == pred_idx {
                src = value.clone();
                break;
            }
        }
        let lowered = lower_value(&src, builder, locals, temps, module, runtime)?;
        args.push(lowered);
    }
    Ok(args)
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
            .unwrap_or_else(|| builder.ins().iconst(types::I64, nanbox_nil_const()))),
        Value::Temp(temp) => Ok(*temps
            .get(&temp.0)
            .unwrap_or(&builder.ins().iconst(types::I64, nanbox_nil_const()))),
    }
}

fn lower_literal(
    lit: &crate::hir::Literal,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    match lit {
        crate::hir::Literal::Integer(v) => {
            let val = builder.ins().iconst(types::I64, *v as i64);
            tag_int(builder, module, runtime, val)
        }
        crate::hir::Literal::Boolean(v) => {
            Ok(builder.ins().iconst(types::I64, nanbox_bool_const(*v)))
        }
        crate::hir::Literal::Nil => Ok(builder.ins().iconst(types::I64, nanbox_nil_const())),
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

fn runtime_fn_coverage_hit(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_coverage_hit", sig)
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

fn runtime_fn_box_int(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_box_int", sig)
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

fn runtime_fn_value_deep_eq(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_value_deep_eq", sig)
}

fn runtime_fn_identity_eq(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_identity_eq", sig)
}

fn runtime_fn_str_intern_utf8(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty, ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_str_intern_utf8", sig)
}

fn runtime_fn_bytes_from_string(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_bytes_from_string", sig)
}

fn runtime_fn_bytes_from_list(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_bytes_from_list", sig)
}

fn runtime_fn_bytes_to_string(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_bytes_to_string", sig)
}

fn runtime_fn_bytes_to_list(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_bytes_to_list", sig)
}

fn runtime_fn_bytes_len(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_bytes_len", sig)
}

fn runtime_fn_fs_read_bytes(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_read_bytes", sig)
}

fn runtime_fn_fs_write_bytes(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_write_bytes", sig)
}

fn runtime_fn_fs_read_dir(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_read_dir", sig)
}

fn runtime_fn_fs_metadata(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_metadata", sig)
}

fn runtime_fn_fs_mkdir_all(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_mkdir_all", sig)
}

fn runtime_fn_fs_remove_file(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_remove_file", sig)
}

fn runtime_fn_fs_remove_dir_all(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_remove_dir_all", sig)
}

fn runtime_fn_fs_rename(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_rename", sig)
}

fn runtime_fn_fs_set_executable(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_fs_set_executable", sig)
}

fn runtime_fn_external_call(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
        ],
        &[types::I64],
    );
    runtime.get_func(module, "wr_external_call", sig)
}

fn runtime_fn_http_call(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
        ],
        &[types::I64],
    );
    runtime.get_func(module, "wr_http_call", sig)
}

fn runtime_fn_str_concat(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty, ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_str_concat", sig)
}

fn runtime_fn_str_concat_local(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty, ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_str_concat_local", sig)
}

fn runtime_fn_str_len(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_str_len", sig)
}

fn runtime_fn_list_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_list_new", sig)
}

fn runtime_fn_list_new_local(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_list_new_local", sig)
}

fn runtime_fn_list_set(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, ptr_ty, types::I64], &[]);
    runtime.get_func(module, "wr_list_set", sig)
}

fn runtime_fn_list_push(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_list_push_val", sig)
}

fn runtime_fn_map_get(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_map_get", sig)
}

fn runtime_fn_map_len(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_map_len", sig)
}

fn runtime_fn_map_set(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_map_set", sig)
}

fn runtime_fn_runtime_cpu_count(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_runtime_cpu_count", sig)
}

fn runtime_fn_reactor_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_reactor_new", sig)
}

fn runtime_fn_reactor_drop(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_reactor_drop", sig)
}

fn runtime_fn_reactor_register(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_reactor_register", sig)
}

fn runtime_fn_reactor_deregister(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_reactor_deregister", sig)
}

fn runtime_fn_reactor_arm_timer(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_reactor_arm_timer", sig)
}

fn runtime_fn_task_signal_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_task_signal_new", sig)
}

fn runtime_fn_task_signal_drop(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_task_signal_drop", sig)
}

fn runtime_fn_task_unpark_one(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_task_unpark_one", sig)
}

fn runtime_fn_task_unpark_all(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_task_unpark_all", sig)
}

fn runtime_fn_task_epoch(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_task_epoch", sig)
}

fn runtime_fn_atomic_i64_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_atomic_i64_new", sig)
}

fn runtime_fn_atomic_i64_drop(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_atomic_i64_drop", sig)
}

fn runtime_fn_atomic_i64_load(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_atomic_i64_load", sig)
}

fn runtime_fn_atomic_i64_store(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_atomic_i64_store", sig)
}

fn runtime_fn_atomic_i64_fetch_add(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_atomic_i64_fetch_add", sig)
}

fn runtime_fn_pool_size(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_pool_size", sig)
}

fn runtime_fn_pool_rr(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_pool_rr", sig)
}

fn runtime_fn_pool_queue_len(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_pool_queue_len", sig)
}

fn runtime_fn_actor_mailbox_len(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_mailbox_len", sig)
}

fn runtime_fn_actor_pause(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_pause", sig)
}

fn runtime_fn_actor_resume(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_resume", sig)
}

fn runtime_fn_actor_pause_wait(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_pause_wait", sig)
}

fn runtime_fn_actor_fire_burst_begin(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_fire_burst_begin", sig)
}

fn runtime_fn_actor_fire_burst_end(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_fire_burst_end", sig)
}

fn runtime_fn_actor_fire_burst_abort(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_fire_burst_abort", sig)
}

fn runtime_fn_metrics_get(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_metrics_get", sig)
}

fn runtime_fn_metrics_dropped_paused_id(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_metrics_dropped_paused_id", sig)
}

fn runtime_fn_metrics_messages_dropped_id(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_metrics_messages_dropped_id", sig)
}

fn runtime_fn_clock_ns(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_clock_ns", sig)
}

fn runtime_fn_sleep_ms(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_sleep_ms", sig)
}

fn runtime_fn_env_get(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_env_get", sig)
}

fn runtime_fn_env_set(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_env_set", sig)
}

fn runtime_fn_process_argv(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_process_argv", sig)
}

fn runtime_fn_process_cwd(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_process_cwd", sig)
}

fn runtime_fn_process_run(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_process_run", sig)
}

fn runtime_fn_process_exit(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_process_exit", sig)
}

fn runtime_fn_runtime_configure(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_runtime_configure", sig)
}

fn runtime_fn_map_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_map_new", sig)
}

fn runtime_fn_map_new_local(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_map_new_local", sig)
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

fn runtime_fn_result_err_unwrap(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_result_err_unwrap", sig)
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
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
        ],
        &[types::I64],
    );
    runtime.get_func(module, "wr_actor_spawn", sig)
}

fn runtime_fn_pool_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
        ],
        &[types::I64],
    );
    runtime.get_func(module, "wr_pool_new", sig)
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

fn runtime_fn_class_get_slot(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, ptr_ty, ptr_ty, ptr_ty], &[types::I64]);
    runtime.get_func(module, "wr_class_get_slot", sig)
}

fn runtime_fn_class_set_slot(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, ptr_ty, ptr_ty, ptr_ty, types::I64],
        &[],
    );
    runtime.get_func(module, "wr_class_set_slot", sig)
}

fn runtime_fn_print(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_print", sig)
}

fn runtime_fn_log(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_log", sig)
}

fn runtime_fn_log_configure(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_log_configure", sig)
}

fn runtime_fn_assert(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_assert", sig)
}

fn runtime_fn_assert_eq(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_assert_eq", sig)
}

fn runtime_fn_assert_value_equality(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_assert_value_equality", sig)
}

fn runtime_fn_assert_identity(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_assert_identity", sig)
}

fn runtime_fn_assert_err(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_assert_err", sig)
}

fn runtime_fn_type_id(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_type_id", sig)
}

#[cfg(test)]
fn runtime_numeric_symbol(op: crate::hir::BinaryOp) -> Option<&'static str> {
    match op {
        crate::hir::BinaryOp::Add => Some("wr_num_add"),
        crate::hir::BinaryOp::Sub => Some("wr_num_sub"),
        crate::hir::BinaryOp::Mul => Some("wr_num_mul"),
        crate::hir::BinaryOp::Div => Some("wr_num_div"),
        crate::hir::BinaryOp::Mod => Some("wr_num_mod"),
        crate::hir::BinaryOp::Lt => Some("wr_num_lt"),
        crate::hir::BinaryOp::Gt => Some("wr_num_gt"),
        crate::hir::BinaryOp::Le => Some("wr_num_le"),
        crate::hir::BinaryOp::Ge => Some("wr_num_ge"),
        _ => None,
    }
}

fn runtime_fn_numeric_binary(
    op: crate::hir::BinaryOp,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    match op {
        crate::hir::BinaryOp::Add => runtime_fn_num_add(module, runtime),
        crate::hir::BinaryOp::Sub => runtime_fn_num_sub(module, runtime),
        crate::hir::BinaryOp::Mul => runtime_fn_num_mul(module, runtime),
        crate::hir::BinaryOp::Div => runtime_fn_num_div(module, runtime),
        crate::hir::BinaryOp::Mod => runtime_fn_num_mod(module, runtime),
        crate::hir::BinaryOp::Lt => runtime_fn_num_lt(module, runtime),
        crate::hir::BinaryOp::Gt => runtime_fn_num_gt(module, runtime),
        crate::hir::BinaryOp::Le => runtime_fn_num_le(module, runtime),
        crate::hir::BinaryOp::Ge => runtime_fn_num_ge(module, runtime),
        _ => Err(CodegenError("unsupported numeric binary op".to_string())),
    }
}

fn runtime_fn_register_method(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, ptr_ty], &[]);
    runtime.get_func(module, "wr_register_method", sig)
}

fn runtime_fn_register_class(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty, ptr_ty, types::I64], &[]);
    runtime.get_func(module, "wr_register_class", sig)
}

fn runtime_fn_register_method_name(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[ptr_ty, ptr_ty, types::I64, types::I64], &[]);
    runtime.get_func(module, "wr_register_method_name", sig)
}

fn runtime_fn_runtime_init(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[]);
    runtime.get_func(module, "wr_runtime_init", sig)
}

fn runtime_fn_runtime_abi(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_runtime_abi", sig)
}

fn emit_runtime_init_and_check(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<(), CodegenError> {
    let init_id = runtime_fn_runtime_init(module, runtime)?;
    let init_callee = module.declare_func_in_func(init_id, builder.func);
    builder.ins().call(init_callee, &[]);

    let abi_id = runtime_fn_runtime_abi(module, runtime)?;
    let abi_callee = module.declare_func_in_func(abi_id, builder.func);
    let call = builder.ins().call(abi_callee, &[]);
    let runtime_abi = builder.inst_results(call)[0];
    let expected = builder.ins().iconst(types::I64, RUNTIME_ABI_VERSION);
    let ok = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::Equal,
        runtime_abi,
        expected,
    );
    let trap_block = builder.create_block();
    let cont_block = builder.create_block();
    builder.ins().brif(ok, cont_block, &[], trap_block, &[]);
    builder.switch_to_block(trap_block);
    builder
        .ins()
        .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
    builder.switch_to_block(cont_block);
    Ok(())
}

fn tag_int(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    value: cranelift_codegen::ir::Value,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let min = builder.ins().iconst(types::I64, NANBOX_INT_MIN);
    let max = builder.ins().iconst(types::I64, NANBOX_INT_MAX);
    let ge_min = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThanOrEqual,
        value,
        min,
    );
    let le_max = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::SignedLessThanOrEqual,
        value,
        max,
    );
    let in_range = builder.ins().band(ge_min, le_max);

    let fast_block = builder.create_block();
    let slow_block = builder.create_block();
    let done_block = builder.create_block();
    builder.append_block_param(done_block, types::I64);
    let done_param = builder.block_params(done_block)[0];

    builder
        .ins()
        .brif(in_range, fast_block, &[], slow_block, &[]);

    builder.switch_to_block(fast_block);
    let payload_mask = builder.ins().iconst(types::I64, NANBOX_PAYLOAD_MASK as i64);
    let payload = builder.ins().band(value, payload_mask);
    let base = builder.ins().iconst(
        types::I64,
        (NANBOX_QNAN | (NANBOX_TAG_INT << NANBOX_TAG_SHIFT)) as i64,
    );
    let boxed = builder.ins().bor(base, payload);
    builder.ins().jump(done_block, &[boxed]);

    builder.switch_to_block(slow_block);
    let func_id = runtime_fn_box_int(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    let call = builder.ins().call(callee, &[value]);
    let boxed = builder.inst_results(call)[0];
    builder.ins().jump(done_block, &[boxed]);

    builder.switch_to_block(done_block);
    Ok(done_param)
}

fn untag_int(
    builder: &mut FunctionBuilder,
    _module: &mut ObjectModule,
    _runtime: &mut RuntimeRegistry,
    value: cranelift_codegen::ir::Value,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let tag_mask = builder.ins().iconst(types::I64, NANBOX_TAG_MASK as i64);
    let tag_shift = builder.ins().iconst(types::I64, NANBOX_TAG_SHIFT as i64);
    let tag = builder.ins().band(value, tag_mask);
    let tag = builder.ins().ushr(tag, tag_shift);
    let tag_int = builder.ins().iconst(types::I64, NANBOX_TAG_INT as i64);
    let is_int = builder
        .ins()
        .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, tag, tag_int);
    let payload_mask = builder.ins().iconst(types::I64, NANBOX_PAYLOAD_MASK as i64);

    let imm_block = builder.create_block();
    let ptr_block = builder.create_block();
    let done_block = builder.create_block();
    builder.append_block_param(done_block, types::I64);
    let done_param = builder.block_params(done_block)[0];

    builder.ins().brif(is_int, imm_block, &[], ptr_block, &[]);

    builder.switch_to_block(imm_block);
    let payload = builder.ins().band(value, payload_mask);
    let sign_bit = builder
        .ins()
        .iconst(types::I64, 1i64 << (NANBOX_TAG_SHIFT - 1));
    let sign_set = builder.ins().band(payload, sign_bit);
    let has_sign = builder.ins().icmp_imm(
        cranelift_codegen::ir::condcodes::IntCC::NotEqual,
        sign_set,
        0,
    );
    let sign_mask = builder
        .ins()
        .iconst(types::I64, !NANBOX_PAYLOAD_MASK as i64);
    let signed = builder.ins().bor(payload, sign_mask);
    let unboxed = builder.ins().select(has_sign, signed, payload);
    builder.ins().jump(done_block, &[unboxed]);

    builder.switch_to_block(ptr_block);
    let ptr_payload = builder.ins().band(value, payload_mask);
    let boxed_val = builder
        .ins()
        .load(types::I64, MemFlags::new(), ptr_payload, 8);
    builder.ins().jump(done_block, &[boxed_val]);

    builder.switch_to_block(done_block);
    Ok(done_param)
}

fn tag_bool(
    builder: &mut FunctionBuilder,
    value: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let qnan = builder.ins().iconst(types::I64, NANBOX_QNAN as i64);
    let tag = builder
        .ins()
        .iconst(types::I64, (NANBOX_TAG_IMM << NANBOX_TAG_SHIFT) as i64);
    let base = builder.ins().bor(qnan, tag);
    let true_payload = builder.ins().iconst(types::I64, NANBOX_IMM_TRUE as i64);
    let false_payload = builder.ins().iconst(types::I64, NANBOX_IMM_FALSE as i64);
    let is_true =
        builder
            .ins()
            .icmp_imm(cranelift_codegen::ir::condcodes::IntCC::NotEqual, value, 0);
    let payload = builder.ins().select(is_true, true_payload, false_payload);
    builder.ins().bor(base, payload)
}

fn untag_bool(
    builder: &mut FunctionBuilder,
    value: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let true_val = builder.ins().iconst(types::I64, nanbox_bool_const(true));
    let is_true = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::Equal,
        value,
        true_val,
    );
    bool_to_int(builder, is_true)
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
    let register_class_id = runtime_fn_register_class(module, runtime)?;
    let register_class = module.declare_func_in_func(register_class_id, builder.func);
    let register_method_name_id = runtime_fn_register_method_name(module, runtime)?;
    let register_method_name = module.declare_func_in_func(register_method_name_id, builder.func);
    for class in classes {
        let (name_ptr, name_len) =
            lower_bytes_literal(builder, module, runtime, class.name.as_str())?;
        let class_id = builder.ins().iconst(types::I64, class.id.0 as i64);
        builder
            .ins()
            .call(register_class, &[name_ptr, name_len, class_id]);
        for method in &class.methods {
            let key = (class.name.clone(), method.name.clone());
            let wrapper_id = wrappers.get(&key).ok_or_else(|| {
                CodegenError(format!(
                    "missing wrapper for {}.{}",
                    class.name, method.name
                ))
            })?;
            let method_id_val = builder.ins().iconst(types::I64, method.id as i64);
            let func_ref = module.declare_func_in_func(*wrapper_id, builder.func);
            let func_ptr = builder.ins().func_addr(ptr_ty, func_ref);
            builder
                .ins()
                .call(callee, &[class_id, method_id_val, func_ptr]);
            let (method_name_ptr, method_name_len) =
                lower_bytes_literal(builder, module, runtime, method.name.as_str())?;
            builder.ins().call(
                register_method_name,
                &[method_name_ptr, method_name_len, class_id, method_id_val],
            );
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
    // String literals are hot in loops; don't allocate a temporary string just to intern it.
    let func_id = runtime_fn_str_intern_utf8(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    let call = builder.ins().call(callee, &[addr, len_val]);
    Ok(builder.inst_results(call)[0])
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
        MirType::Integer | MirType::Boolean | MirType::Nil | MirType::Unknown => Ok(types::I64),
        MirType::Float => Ok(types::I64),
        MirType::String
        | MirType::Named(_)
        | MirType::Actor(_)
        | MirType::Pending(_)
        | MirType::Result(_, _) => Ok(types::I64),
    }
}

#[cfg(test)]
mod tests {
    use super::{compile_to_object, runtime_numeric_symbol};
    use crate::hir::{BinaryOp, Literal};
    use crate::mir::ir::{
        BasicBlock, BlockId, CallKind, CallTarget, MirFunction, MirModule, MirType, Place, Rvalue,
        Stmt, Temp, TempId, Terminator, Value,
    };
    use rowan::TextRange;

    #[test]
    fn runtime_numeric_symbol_maps_supported_binary_ops() {
        assert_eq!(runtime_numeric_symbol(BinaryOp::Add), Some("wr_num_add"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Sub), Some("wr_num_sub"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Mul), Some("wr_num_mul"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Div), Some("wr_num_div"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Mod), Some("wr_num_mod"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Lt), Some("wr_num_lt"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Gt), Some("wr_num_gt"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Le), Some("wr_num_le"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Ge), Some("wr_num_ge"));
        assert_eq!(runtime_numeric_symbol(BinaryOp::Eq), None);
    }

    #[test]
    fn compile_to_object_supports_external_call_builtin() {
        let span = TextRange::new(0.into(), 0.into());
        let headers_temp = TempId(0);
        let call_temp = TempId(1);
        let func = MirFunction {
            name: "run".into(),
            params: Vec::new(),
            locals: Vec::new(),
            temps: vec![
                Temp {
                    ty: MirType::Unknown,
                },
                Temp {
                    ty: MirType::Unknown,
                },
            ],
            blocks: vec![BasicBlock {
                stmts: vec![
                    Stmt::Assign {
                        place: Place::Temp(headers_temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function("__wr_map_new".into()),
                            args: Vec::new(),
                        },
                        span,
                    },
                    Stmt::Assign {
                        place: Place::Temp(call_temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function("__wr_external_call".into()),
                            args: vec![
                                Value::Const(Literal::String("svc".into())),
                                Value::Const(Literal::String("ep".into())),
                                Value::Const(Literal::String("GET".into())),
                                Value::Const(Literal::String("https://example.test".into())),
                                Value::Temp(headers_temp),
                                Value::Const(Literal::String("body".into())),
                                Value::Const(Literal::Integer(100)),
                            ],
                        },
                        span,
                    },
                ],
                terminator: Terminator::Return {
                    value: Some(Value::Temp(call_temp)),
                    span,
                },
            }],
            entry: BlockId(0),
            suspendable: false,
        };
        let mir = MirModule {
            functions: vec![func],
            type_tags: Vec::new(),
            classes: Vec::new(),
        };
        let obj = compile_to_object(&mir).expect("compile object");
        assert!(!obj.is_empty(), "expected non-empty object bytes");
    }
}
