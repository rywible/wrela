use crate::hir::{CheckBinaryOp, CheckDagShapeFamily, CheckIrFunction, CheckValue, DecisionNode};
use crate::hir::{Objective, PoolSize};
use crate::mir::ir::{
    AllocKind, CallKind, CallTarget, MirFunction, MirModule, MirType, Place, PortableAbiType,
    PortableStructField, Rvalue, Stmt, Terminator, Value,
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
const RUNTIME_ABI_VERSION: i64 = 5;
const FNV1A_OFFSET_BASIS_64: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME_64: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug)]
pub struct CodegenError(pub String);

#[derive(Clone, Debug)]
struct PhiNode {
    place: Place,
    sources: Vec<(crate::mir::ir::BlockId, Value)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortableLaneKind {
    Bool,
    I32,
    U32,
    I64,
    U64,
    F32,
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
    let function_abis = collect_function_abis(mir);
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
            &function_abis,
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
    if cfg!(all(unix, not(target_os = "macos"))) {
        cmd.arg("-lm");
    }
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

pub fn compile_to_shared_library(mir: &MirModule, output: &Path) -> Result<(), CodegenError> {
    let obj = compile_to_object(mir)?;
    let obj_path = temp_object_path(output);
    fs::write(&obj_path, obj).map_err(|err| CodegenError(format!("write obj failed: {err}")))?;

    let dylib_ext = if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let output_path = output.with_extension(dylib_ext);

    let mut linker = linker_command()?;
    let cmd = &mut linker.cmd;
    cmd.arg("-shared")
        .arg("-o")
        .arg(&output_path)
        .arg(&obj_path);
    let runtime_lib = ensure_runtime_built()?;
    cmd.arg(runtime_lib);
    if cfg!(all(unix, not(target_os = "macos"))) {
        cmd.arg("-lm");
    }
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
        eprintln!("linker (shared): {:?}", cmd);
    }
    let output = cmd
        .output()
        .map_err(|err| linker_io_error(&linker.name, err))?;

    let _ = fs::remove_file(&obj_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.trim().is_empty() {
            "shared library linker failed".to_string()
        } else {
            format!("shared library linker failed: {}", stderr.trim())
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
    if let Ok(exe_path) = env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        // bin/wrela -> lib/libwrela_runtime.a
        let lib_path = exe_dir.parent().map(|p| p.join("lib").join(lib_name));
        if let Some(path) = lib_path
            && path.exists()
        {
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
    if cfg!(target_os = "macos")
        && let Some(path) = macos_clang_path()?
    {
        return Ok(LinkerCommand {
            cmd: Command::new(&path),
            name: path,
        });
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
    if newest_mtime_in_tree(&src_dir).is_none_or(|time| time > lib_time) {
        return true;
    }
    let manifest = runtime_root.join("Cargo.toml");
    if let Some(time) = mtime(&manifest)
        && time > lib_time
    {
        return true;
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

fn collect_function_abis(
    mir: &MirModule,
) -> HashMap<SmolStr, (Vec<PortableAbiType>, PortableAbiType)> {
    mir.functions
        .iter()
        .map(|func| {
            (
                func.name.clone(),
                (func.abi_params.clone(), func.abi_return.clone()),
            )
        })
        .collect()
}

fn function_signature(module: &ObjectModule, func: &MirFunction) -> Signature {
    let mut sig = module.make_signature();
    sig.call_conv = module.target_config().default_call_conv;
    if portable_abi_uses_sret(&func.abi_return) {
        sig.params
            .push(AbiParam::new(module.target_config().pointer_type()));
    }
    for param in &func.abi_params {
        for lane in portable_abi_param_types(param) {
            sig.params.push(AbiParam::new(lane));
        }
    }
    if portable_abi_is_legacy_value(&func.abi_return) {
        sig.returns.push(AbiParam::new(types::I64));
    } else if let Some(ret_ty) = portable_abi_scalar_type(&func.abi_return) {
        sig.returns.push(AbiParam::new(ret_ty));
    }
    sig
}

fn lower_function(
    func: &MirFunction,
    func_ids: &HashMap<SmolStr, cranelift_module::FuncId>,
    function_abis: &HashMap<SmolStr, (Vec<PortableAbiType>, PortableAbiType)>,
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
    let entry_params = builder.block_params(entry_block).to_vec();

    let mut locals = HashMap::new();
    for (idx, local) in func.locals.iter().enumerate() {
        let var = Variable::from_u32(idx as u32);
        builder.declare_var(var, ty_to_clif(&local.ty)?);
        locals.insert(idx, var);
    }

    let phi_offset = phi_map
        .get(func.entry.0)
        .map(|phis| phis.len())
        .unwrap_or(0);
    let mut entry_cursor = phi_offset;
    let return_ptr = if portable_abi_uses_sret(&func.abi_return) {
        let ptr = entry_params.get(entry_cursor).copied();
        entry_cursor += 1;
        ptr
    } else {
        None
    };

    for ((local_id, abi_ty), _param_idx) in func
        .params
        .iter()
        .zip(func.abi_params.iter())
        .zip(0..func.params.len())
    {
        let lane_count = portable_abi_lane_count(abi_ty);
        let lane_end = entry_cursor + lane_count;
        let param_lanes = &entry_params[entry_cursor..lane_end];
        let param_val = box_portable_param(&mut builder, module, runtime, abi_ty, param_lanes, 0)?;
        entry_cursor = lane_end;
        if let Some(var) = locals.get(&local_id.0) {
            builder.def_var(*var, param_val);
        }
    }

    let mut temps: HashMap<usize, cranelift_codegen::ir::Value> = HashMap::new();

    let block_order = std::iter::once(func.entry.0)
        .chain((0..func.blocks.len()).filter(|idx| *idx != func.entry.0));
    for block_idx in block_order {
        let block = &func.blocks[block_idx];
        let block_id = block_map[block_idx];
        if block_idx != func.entry.0 {
            builder.switch_to_block(block_id);
        }
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
                function_abis,
                module,
                runtime,
            )?;
        }
        lower_terminator(
            &block.terminator,
            &func.abi_return,
            return_ptr,
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
    function_abis: &HashMap<SmolStr, (Vec<PortableAbiType>, PortableAbiType)>,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<(), CodegenError> {
    match stmt {
        Stmt::Phi { .. } => {}
        Stmt::Assign { place, value, .. } => {
            let val = lower_rvalue(
                value,
                builder,
                locals,
                temps,
                locals_tys,
                temps_tys,
                func_ids,
                function_abis,
                module,
                runtime,
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
            // `fire` consumes a Pending value at MIR level. RC decrements are emitted explicitly
            // as `Stmt::RcDec` by MIR lowering/optimization; doing another dec here would
            // double-release fire temps.
            let _ = lower_value(pending, builder, locals, temps, module, runtime)?;
        }
        Stmt::ActorFire { target, args, .. } => {
            let (handle, method_id) = match target {
                CallTarget::Method {
                    receiver,
                    method_id,
                    ..
                } => {
                    let recv = lower_value(receiver, builder, locals, temps, module, runtime)?;
                    let mid = method_id.ok_or_else(|| {
                        CodegenError("missing method id for actor fire".to_string())
                    })?;
                    (recv, mid)
                }
                _ => return Err(CodegenError("unsupported actor fire target".to_string())),
            };
            let call_args: Vec<_> = args
                .iter()
                .map(|v| lower_value(v, builder, locals, temps, module, runtime))
                .collect::<Result<Vec<_>, _>>()?;
            let method_id_val = builder.ins().iconst(types::I64, method_id as i64);
            let func_id = match call_args.len() {
                0 => runtime_fn_actor_fire_0(module, runtime)?,
                1 => runtime_fn_actor_fire_1(module, runtime)?,
                2 => runtime_fn_actor_fire_2(module, runtime)?,
                _ => {
                    let ptr_ty = module.target_config().pointer_type();
                    let (args_ptr, args_len) = build_value_array(builder, ptr_ty, &call_args);
                    let f = runtime_fn_actor_fire(module, runtime)?;
                    let callee = module.declare_func_in_func(f, builder.func);
                    builder
                        .ins()
                        .call(callee, &[handle, method_id_val, args_len, args_ptr]);
                    return Ok(());
                }
            };
            let callee = module.declare_func_in_func(func_id, builder.func);
            let mut fire_args = vec![handle, method_id_val];
            fire_args.extend(call_args);
            builder.ins().call(callee, &fire_args);
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
    function_abis: &HashMap<SmolStr, (Vec<PortableAbiType>, PortableAbiType)>,
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
                        ty if is_mir_vector_type(&ty) => {
                            let minus_one = lower_value(
                                &Value::Const(crate::hir::Literal::Float(-1.0)),
                                builder,
                                locals,
                                temps,
                                module,
                                runtime,
                            )?;
                            let func_id = runtime_fn_vec_mul(module, runtime)?;
                            let callee = module.declare_func_in_func(func_id, builder.func);
                            let call = builder.ins().call(callee, &[v, minus_one]);
                            Ok(builder.inst_results(call)[0])
                        }
                        MirType::Mat3 => {
                            let minus_one = lower_value(
                                &Value::Const(crate::hir::Literal::Float(-1.0)),
                                builder,
                                locals,
                                temps,
                                module,
                                runtime,
                            )?;
                            let func_id = runtime_fn_mat3_mul_scalar(module, runtime)?;
                            let callee = module.declare_func_in_func(func_id, builder.func);
                            let call = builder.ins().call(callee, &[v, minus_one]);
                            Ok(builder.inst_results(call)[0])
                        }
                        ty if is_mir_matrix_type(&ty) => {
                            let minus_one = lower_value(
                                &Value::Const(crate::hir::Literal::Float(-1.0)),
                                builder,
                                locals,
                                temps,
                                module,
                                runtime,
                            )?;
                            let func_id = runtime_fn_mat4_mul_scalar(module, runtime)?;
                            let callee = module.declare_func_in_func(func_id, builder.func);
                            let call = builder.ins().call(callee, &[v, minus_one]);
                            Ok(builder.inst_results(call)[0])
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
                _ => Err(CodegenError(format!(
                    "unsupported unary op in codegen: {:?}",
                    op
                ))),
            }
        }
        Rvalue::Binary { op, lhs, rhs } => {
            let lhs_val = lower_value(lhs, builder, locals, temps, module, runtime)?;
            let rhs_val = lower_value(rhs, builder, locals, temps, module, runtime)?;
            let val = match op {
                crate::hir::BinaryOp::Add => {
                    let lty = mir_type_of_value(lhs, locals_tys, temps_tys);
                    let rty = mir_type_of_value(rhs, locals_tys, temps_tys);
                    if same_mir_vector_kind(&lty, &rty) {
                        let func_id = runtime_fn_vec_add(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Mat3) && matches!(rty, MirType::Mat3) {
                        let func_id = runtime_fn_mat3_add(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if same_mir_matrix_kind(&lty, &rty) {
                        let func_id = runtime_fn_mat4_add(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
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
                    if same_mir_vector_kind(&lty, &rty) {
                        let func_id = runtime_fn_vec_sub(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Mat3) && matches!(rty, MirType::Mat3) {
                        let func_id = runtime_fn_mat3_sub(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if same_mir_matrix_kind(&lty, &rty) {
                        let func_id = runtime_fn_mat4_sub(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
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
                    if same_mir_vector_kind(&lty, &rty)
                        || (is_mir_vector_type(&lty) && is_mir_scalar_numeric(&rty))
                        || (is_mir_scalar_numeric(&lty) && is_mir_vector_type(&rty))
                    {
                        let func_id = runtime_fn_vec_mul(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Mat3) && matches!(rty, MirType::Vec3) {
                        let func_id = runtime_fn_mat3_mul_vec3(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Mat4) && matches!(rty, MirType::Vec4) {
                        let func_id = runtime_fn_mat4_mul_vec4(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Mat3) && matches!(rty, MirType::Mat3) {
                        let func_id = runtime_fn_mat3_mul_mat3(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if same_mir_matrix_kind(&lty, &rty) {
                        let func_id = runtime_fn_mat4_mul_mat4(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Mat3) && is_mir_scalar_numeric(&rty) {
                        let func_id = runtime_fn_mat3_mul_scalar(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if is_mir_scalar_numeric(&lty) && matches!(rty, MirType::Mat3) {
                        let func_id = runtime_fn_mat3_mul_scalar(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[rhs_val, lhs_val]);
                        builder.inst_results(call)[0]
                    } else if is_mir_matrix_type(&lty) && is_mir_scalar_numeric(&rty) {
                        let func_id = runtime_fn_mat4_mul_scalar(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if is_mir_scalar_numeric(&lty) && is_mir_matrix_type(&rty) {
                        let func_id = runtime_fn_mat4_mul_scalar(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[rhs_val, lhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
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
                    if is_mir_vector_type(&lty) && is_mir_scalar_numeric(&rty) {
                        let func_id = runtime_fn_vec_div(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Mat3) && is_mir_scalar_numeric(&rty) {
                        let func_id = runtime_fn_mat3_div_scalar(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if is_mir_matrix_type(&lty) && is_mir_scalar_numeric(&rty) {
                        let func_id = runtime_fn_mat4_div_scalar(module, runtime)?;
                        let callee = module.declare_func_in_func(func_id, builder.func);
                        let call = builder.ins().call(callee, &[lhs_val, rhs_val]);
                        builder.inst_results(call)[0]
                    } else if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
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
                    let (func_id, user_abi) = match target {
                        CallTarget::Function(name) => {
                            if let Some(id) = func_ids.get(name).copied() {
                                (Some(id), function_abis.get(name).cloned())
                            } else {
                                (runtime_builtin_func_id(name, module, runtime)?, None)
                            }
                        }
                        CallTarget::Method {
                            receiver, method, ..
                        } => {
                            let recv =
                                lower_value(receiver, builder, locals, temps, module, runtime)?;
                            call_args.insert(0, recv);
                            (
                                func_ids.get(method).copied(),
                                function_abis.get(method).cloned(),
                            )
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
                        CallTarget::Indirect(_) => (None, None),
                    };
                    let func_id = func_id.ok_or_else(|| {
                        CodegenError(format!("unsupported call target: {}", target_name))
                    })?;
                    let callee = module.declare_func_in_func(func_id, builder.func);
                    if let Some((param_abis, ret_abi)) = user_abi {
                        emit_portable_function_call(
                            builder,
                            module,
                            runtime,
                            callee,
                            &call_args,
                            &param_abis,
                            &ret_abi,
                        )
                    } else {
                        let call_inst = builder.ins().call(callee, &call_args);
                        let results = builder.inst_results(call_inst);
                        Ok(results[0])
                    }
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
                    let method_id_val = builder.ins().iconst(types::I64, method_id as i64);
                    let (func_id, use_specialized) = match call_args.len() {
                        0 => (runtime_fn_actor_send_0(module, runtime)?, true),
                        1 => (runtime_fn_actor_send_1(module, runtime)?, true),
                        2 => (runtime_fn_actor_send_2(module, runtime)?, true),
                        _ => (runtime_fn_actor_send(module, runtime)?, false),
                    };
                    let callee = module.declare_func_in_func(func_id, builder.func);
                    let call_inst = if use_specialized {
                        let mut send_args = vec![handle, method_id_val];
                        send_args.extend(&call_args);
                        builder.ins().call(callee, &send_args)
                    } else {
                        let ptr_ty = module.target_config().pointer_type();
                        let (args_ptr, args_len) = build_value_array(builder, ptr_ty, &call_args);
                        builder
                            .ins()
                            .call(callee, &[handle, method_id_val, args_len, args_ptr])
                    };
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
                let func_id = runtime_fn_list_set_raw(module, runtime)?;
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

fn is_mir_scalar_numeric(ty: &MirType) -> bool {
    matches!(ty, MirType::Integer | MirType::Float)
}

fn is_mir_vector_type(ty: &MirType) -> bool {
    matches!(
        ty,
        MirType::Vec2 | MirType::Vec3 | MirType::Vec4 | MirType::Quat
    )
}

fn is_mir_matrix_type(ty: &MirType) -> bool {
    matches!(ty, MirType::Mat3 | MirType::Mat4)
}

fn same_mir_vector_kind(left: &MirType, right: &MirType) -> bool {
    matches!(
        (left, right),
        (MirType::Vec2, MirType::Vec2)
            | (MirType::Vec3, MirType::Vec3)
            | (MirType::Vec4, MirType::Vec4)
            | (MirType::Quat, MirType::Quat)
    )
}

fn same_mir_matrix_kind(left: &MirType, right: &MirType) -> bool {
    matches!(
        (left, right),
        (MirType::Mat3, MirType::Mat3) | (MirType::Mat4, MirType::Mat4)
    )
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

fn portable_abi_is_legacy_value(abi: &PortableAbiType) -> bool {
    matches!(abi, PortableAbiType::Value)
}

fn portable_abi_scalar_kind(abi: &PortableAbiType) -> Option<PortableLaneKind> {
    match abi {
        PortableAbiType::Bool => Some(PortableLaneKind::Bool),
        PortableAbiType::I32 => Some(PortableLaneKind::I32),
        PortableAbiType::U32 => Some(PortableLaneKind::U32),
        PortableAbiType::I64 => Some(PortableLaneKind::I64),
        PortableAbiType::U64 => Some(PortableLaneKind::U64),
        PortableAbiType::F32 => Some(PortableLaneKind::F32),
        _ => None,
    }
}

fn portable_lane_clif_type(kind: PortableLaneKind) -> cranelift_codegen::ir::Type {
    match kind {
        PortableLaneKind::Bool => types::I8,
        PortableLaneKind::I32 | PortableLaneKind::U32 => types::I32,
        PortableLaneKind::I64 | PortableLaneKind::U64 => types::I64,
        PortableLaneKind::F32 => types::F32,
    }
}

fn portable_abi_scalar_type(abi: &PortableAbiType) -> Option<cranelift_codegen::ir::Type> {
    portable_abi_scalar_kind(abi).map(portable_lane_clif_type)
}

fn portable_abi_uses_sret(abi: &PortableAbiType) -> bool {
    !portable_abi_is_legacy_value(abi) && portable_abi_scalar_kind(abi).is_none()
}

fn portable_abi_param_types(abi: &PortableAbiType) -> Vec<cranelift_codegen::ir::Type> {
    let mut out = Vec::with_capacity(portable_abi_lane_count(abi).max(1));
    portable_abi_collect_param_types(abi, &mut out);
    out
}

fn portable_abi_collect_param_types(
    abi: &PortableAbiType,
    out: &mut Vec<cranelift_codegen::ir::Type>,
) {
    if portable_abi_is_legacy_value(abi) {
        out.push(types::I64);
        return;
    }
    if let Some(scalar) = portable_abi_scalar_type(abi) {
        out.push(scalar);
        return;
    }
    match abi {
        PortableAbiType::Vec2 => out.extend([types::F32, types::F32]),
        PortableAbiType::Vec3 => out.extend([types::F32, types::F32, types::F32]),
        PortableAbiType::Vec4 | PortableAbiType::Quat => {
            out.extend([types::F32, types::F32, types::F32, types::F32])
        }
        PortableAbiType::Mat3 => out.extend([types::F32; 9]),
        PortableAbiType::Mat4 => out.extend([types::F32; 16]),
        PortableAbiType::Array(inner, len) => {
            for _ in 0..*len {
                portable_abi_collect_param_types(inner, out);
            }
        }
        PortableAbiType::Struct { fields, .. } => {
            for field in fields {
                portable_abi_collect_param_types(&field.ty, out);
            }
        }
        PortableAbiType::Value => out.push(types::I64),
        PortableAbiType::Bool
        | PortableAbiType::I32
        | PortableAbiType::U32
        | PortableAbiType::I64
        | PortableAbiType::U64
        | PortableAbiType::F32 => unreachable!(),
    }
}

fn portable_abi_lane_count(abi: &PortableAbiType) -> usize {
    match abi {
        PortableAbiType::Value
        | PortableAbiType::Bool
        | PortableAbiType::I32
        | PortableAbiType::U32
        | PortableAbiType::I64
        | PortableAbiType::U64
        | PortableAbiType::F32 => 1,
        PortableAbiType::Vec2 => 2,
        PortableAbiType::Vec3 => 3,
        PortableAbiType::Vec4 | PortableAbiType::Quat => 4,
        PortableAbiType::Mat3 => 9,
        PortableAbiType::Mat4 => 16,
        PortableAbiType::Array(inner, len) => portable_abi_lane_count(inner) * len,
        PortableAbiType::Struct { fields, .. } => fields
            .iter()
            .map(|field| portable_abi_lane_count(&field.ty))
            .sum(),
    }
}

fn portable_abi_layout(abi: &PortableAbiType) -> (u32, u32) {
    match abi {
        PortableAbiType::Bool => (1, 1),
        PortableAbiType::I32 | PortableAbiType::U32 | PortableAbiType::F32 => (4, 4),
        PortableAbiType::I64 | PortableAbiType::U64 => (8, 8),
        PortableAbiType::Vec2 => portable_fixed_array_layout(4, 4, 2),
        PortableAbiType::Vec3 => portable_fixed_array_layout(4, 4, 3),
        PortableAbiType::Vec4 | PortableAbiType::Quat => portable_fixed_array_layout(4, 4, 4),
        PortableAbiType::Mat3 => portable_fixed_array_layout(4, 4, 9),
        PortableAbiType::Mat4 => portable_fixed_array_layout(4, 4, 16),
        PortableAbiType::Array(inner, len) => {
            let (size, align) = portable_abi_layout(inner);
            portable_fixed_array_layout(size, align, *len)
        }
        PortableAbiType::Struct { fields, .. } => {
            let mut offset = 0;
            let mut max_align = 1;
            for field in fields {
                let (field_size, field_align) = portable_abi_layout(&field.ty);
                max_align = max_align.max(field_align);
                offset = align_to_u32(offset, field_align);
                offset += field_size;
            }
            (align_to_u32(offset, max_align).max(1), max_align.max(1))
        }
        PortableAbiType::Value => (8, 8),
    }
}

fn portable_fixed_array_layout(item_size: u32, item_align: u32, len: usize) -> (u32, u32) {
    let stride = align_to_u32(item_size, item_align);
    if len == 0 {
        (0, item_align.max(1))
    } else {
        (
            stride.saturating_mul(len.saturating_sub(1) as u32) + item_size,
            item_align.max(1),
        )
    }
}

fn align_to_u32(offset: u32, align: u32) -> u32 {
    if align <= 1 {
        return offset;
    }
    let rem = offset % align;
    if rem == 0 {
        offset
    } else {
        offset + (align - rem)
    }
}

fn align_shift_for_bytes(align: u32) -> u8 {
    align.max(1).trailing_zeros() as u8
}

fn field_offset(fields: &[PortableStructField], index: usize) -> u32 {
    let mut offset = 0;
    for field in fields.iter().take(index) {
        let (size, align) = portable_abi_layout(&field.ty);
        offset = align_to_u32(offset, align);
        offset += size;
    }
    if let Some(field) = fields.get(index) {
        let (_, align) = portable_abi_layout(&field.ty);
        align_to_u32(offset, align)
    } else {
        offset
    }
}

fn tagged_index_value(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    index: usize,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let raw = builder.ins().iconst(types::I64, index as i64);
    tag_int(builder, module, runtime, raw)
}

fn boxed_float_from_f32_lane(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    lane: cranelift_codegen::ir::Value,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let promoted = builder.ins().fpromote(types::F64, lane);
    let func_id = runtime_fn_box_float(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    let call = builder.ins().call(callee, &[promoted]);
    Ok(builder.inst_results(call)[0])
}

fn box_scalar_lane_to_value(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    kind: PortableLaneKind,
    lane: cranelift_codegen::ir::Value,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    match kind {
        PortableLaneKind::Bool => Ok(tag_bool(builder, lane)),
        PortableLaneKind::I32 => {
            let widened = builder.ins().sextend(types::I64, lane);
            tag_int(builder, module, runtime, widened)
        }
        PortableLaneKind::U32 => {
            let widened = builder.ins().uextend(types::I64, lane);
            tag_int(builder, module, runtime, widened)
        }
        PortableLaneKind::I64 | PortableLaneKind::U64 => tag_int(builder, module, runtime, lane),
        PortableLaneKind::F32 => boxed_float_from_f32_lane(builder, module, runtime, lane),
    }
}

fn lower_value_to_scalar_lane(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    abi: &PortableAbiType,
    value: cranelift_codegen::ir::Value,
    ty: cranelift_codegen::ir::Type,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let Some(kind) = portable_abi_scalar_kind(abi) else {
        return Err(CodegenError("expected scalar portable ABI".to_string()));
    };
    match kind {
        PortableLaneKind::Bool => {
            let raw = untag_bool(builder, value);
            Ok(if ty == types::I8 {
                builder.ins().ireduce(types::I8, raw)
            } else {
                raw
            })
        }
        PortableLaneKind::I32 => {
            let cast_id = runtime_fn_cast_i32(module, runtime)?;
            let cast_callee = module.declare_func_in_func(cast_id, builder.func);
            let cast_call = builder.ins().call(cast_callee, &[value]);
            let casted = builder.inst_results(cast_call)[0];
            let raw = untag_int(builder, module, runtime, casted)?;
            Ok(builder.ins().ireduce(ty, raw))
        }
        PortableLaneKind::U32 => {
            let cast_id = runtime_fn_cast_u32(module, runtime)?;
            let cast_callee = module.declare_func_in_func(cast_id, builder.func);
            let cast_call = builder.ins().call(cast_callee, &[value]);
            let casted = builder.inst_results(cast_call)[0];
            let raw = untag_int(builder, module, runtime, casted)?;
            Ok(builder.ins().ireduce(ty, raw))
        }
        PortableLaneKind::I64 | PortableLaneKind::U64 => untag_int(builder, module, runtime, value),
        PortableLaneKind::F32 => {
            let cast_id = runtime_fn_cast_f32(module, runtime)?;
            let cast_callee = module.declare_func_in_func(cast_id, builder.func);
            let cast_call = builder.ins().call(cast_callee, &[value]);
            let casted = builder.inst_results(cast_call)[0];
            let unbox_id = runtime_fn_unbox_float(module, runtime)?;
            let unbox_callee = module.declare_func_in_func(unbox_id, builder.func);
            let unbox_call = builder.ins().call(unbox_callee, &[casted]);
            let f64_value = builder.inst_results(unbox_call)[0];
            Ok(builder.ins().fdemote(types::F32, f64_value))
        }
    }
}

fn box_portable_param(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    abi: &PortableAbiType,
    lanes: &[cranelift_codegen::ir::Value],
    start: usize,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let (value, _) = box_portable_param_inner(builder, module, runtime, abi, lanes, start)?;
    Ok(value)
}

fn box_portable_param_inner(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    abi: &PortableAbiType,
    lanes: &[cranelift_codegen::ir::Value],
    start: usize,
) -> Result<(cranelift_codegen::ir::Value, usize), CodegenError> {
    if portable_abi_is_legacy_value(abi) {
        return Ok((lanes[start], start + 1));
    }
    if let Some(kind) = portable_abi_scalar_kind(abi) {
        return Ok((
            box_scalar_lane_to_value(builder, module, runtime, kind, lanes[start])?,
            start + 1,
        ));
    }

    match abi {
        PortableAbiType::Vec2
        | PortableAbiType::Vec3
        | PortableAbiType::Vec4
        | PortableAbiType::Quat => {
            let lane_count = portable_abi_lane_count(abi);
            let mut cursor = start;
            let mut values = Vec::with_capacity(lane_count);
            for _ in 0..lane_count {
                values.push(boxed_float_from_f32_lane(
                    builder,
                    module,
                    runtime,
                    lanes[cursor],
                )?);
                cursor += 1;
            }
            let func_id = match abi {
                PortableAbiType::Vec2 => runtime_fn_vec2_new(module, runtime)?,
                PortableAbiType::Vec3 => runtime_fn_vec3_new(module, runtime)?,
                PortableAbiType::Vec4 => runtime_fn_vec4_new(module, runtime)?,
                PortableAbiType::Quat => runtime_fn_quat_new(module, runtime)?,
                _ => unreachable!(),
            };
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &values);
            Ok((builder.inst_results(call)[0], cursor))
        }
        PortableAbiType::Mat3 => {
            let mut cursor = start;
            let mut cols = Vec::with_capacity(3);
            for _ in 0..3 {
                let (col, next) = box_portable_param_inner(
                    builder,
                    module,
                    runtime,
                    &PortableAbiType::Vec3,
                    lanes,
                    cursor,
                )?;
                cols.push(col);
                cursor = next;
            }
            let func_id = runtime_fn_mat3_from_columns(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &cols);
            Ok((builder.inst_results(call)[0], cursor))
        }
        PortableAbiType::Mat4 => {
            let mut cursor = start;
            let mut cols = Vec::with_capacity(4);
            for _ in 0..4 {
                let (col, next) = box_portable_param_inner(
                    builder,
                    module,
                    runtime,
                    &PortableAbiType::Vec4,
                    lanes,
                    cursor,
                )?;
                cols.push(col);
                cursor = next;
            }
            let func_id = runtime_fn_mat4_from_columns(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &cols);
            Ok((builder.inst_results(call)[0], cursor))
        }
        PortableAbiType::Array(inner, len) => {
            let ptr_ty = module.target_config().pointer_type();
            let len_val = builder.ins().iconst(ptr_ty, *len as i64);
            let func_id = runtime_fn_list_new_local(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[len_val]);
            let list = builder.inst_results(call)[0];
            let set_id = runtime_fn_list_set_raw(module, runtime)?;
            let set_callee = module.declare_func_in_func(set_id, builder.func);
            let mut cursor = start;
            for idx in 0..*len {
                let (item, next) =
                    box_portable_param_inner(builder, module, runtime, inner, lanes, cursor)?;
                cursor = next;
                let idx_val = builder.ins().iconst(ptr_ty, idx as i64);
                builder.ins().call(set_callee, &[list, idx_val, item]);
            }
            Ok((list, cursor))
        }
        PortableAbiType::Struct {
            class_id, fields, ..
        } => {
            let field_names: Vec<SmolStr> = fields.iter().map(|field| field.name.clone()).collect();
            let (names_ptr, lens_ptr, count_val) =
                build_bytes_arrays(builder, module, runtime, &field_names)?;
            let func_id = runtime_fn_class_new(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let class_id_val = builder.ins().iconst(types::I64, *class_id as i64);
            let call = builder
                .ins()
                .call(callee, &[class_id_val, names_ptr, lens_ptr, count_val]);
            let obj = builder.inst_results(call)[0];
            let set_id = runtime_fn_class_set_slot(module, runtime)?;
            let set_callee = module.declare_func_in_func(set_id, builder.func);
            let ptr_ty = module.target_config().pointer_type();
            let mut cursor = start;
            for (idx, field) in fields.iter().enumerate() {
                let (field_val, next) =
                    box_portable_param_inner(builder, module, runtime, &field.ty, lanes, cursor)?;
                cursor = next;
                let (name_ptr, len_val) =
                    lower_bytes_literal(builder, module, runtime, field.name.as_str())?;
                let slot_val = builder.ins().iconst(ptr_ty, idx as i64);
                builder
                    .ins()
                    .call(set_callee, &[obj, name_ptr, len_val, slot_val, field_val]);
            }
            Ok((obj, cursor))
        }
        PortableAbiType::Value
        | PortableAbiType::Bool
        | PortableAbiType::I32
        | PortableAbiType::U32
        | PortableAbiType::I64
        | PortableAbiType::U64
        | PortableAbiType::F32 => unreachable!(),
    }
}

fn emit_vec_component(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    value: cranelift_codegen::ir::Value,
    index: usize,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let func_id = runtime_fn_vec_component(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    let index_val = tagged_index_value(builder, module, runtime, index)?;
    let call = builder.ins().call(callee, &[value, index_val]);
    Ok(builder.inst_results(call)[0])
}

fn emit_mat_component(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    abi: &PortableAbiType,
    value: cranelift_codegen::ir::Value,
    index: usize,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let func_id = match abi {
        PortableAbiType::Mat3 => runtime_fn_mat3_component(module, runtime)?,
        PortableAbiType::Mat4 => runtime_fn_mat4_component(module, runtime)?,
        _ => return Err(CodegenError("expected matrix portable ABI".to_string())),
    };
    let callee = module.declare_func_in_func(func_id, builder.func);
    let index_val = tagged_index_value(builder, module, runtime, index)?;
    let call = builder.ins().call(callee, &[value, index_val]);
    Ok(builder.inst_results(call)[0])
}

fn emit_list_get_value(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    list: cranelift_codegen::ir::Value,
    index: usize,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let func_id = runtime_fn_list_get_val(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    let index_val = tagged_index_value(builder, module, runtime, index)?;
    let call = builder.ins().call(callee, &[list, index_val]);
    Ok(builder.inst_results(call)[0])
}

fn emit_class_get_value(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    object: cranelift_codegen::ir::Value,
    field: &PortableStructField,
    slot: usize,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let func_id = runtime_fn_class_get_slot(module, runtime)?;
    let callee = module.declare_func_in_func(func_id, builder.func);
    let (name_ptr, len_val) = lower_bytes_literal(builder, module, runtime, field.name.as_str())?;
    let slot_val = builder
        .ins()
        .iconst(module.target_config().pointer_type(), slot as i64);
    let call = builder
        .ins()
        .call(callee, &[object, name_ptr, len_val, slot_val]);
    Ok(builder.inst_results(call)[0])
}

fn append_portable_call_arg_lanes(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    abi: &PortableAbiType,
    value: cranelift_codegen::ir::Value,
    out: &mut Vec<cranelift_codegen::ir::Value>,
) -> Result<(), CodegenError> {
    if portable_abi_is_legacy_value(abi) {
        out.push(value);
        return Ok(());
    }
    if let Some(ty) = portable_abi_scalar_type(abi) {
        out.push(lower_value_to_scalar_lane(
            builder, module, runtime, abi, value, ty,
        )?);
        return Ok(());
    }
    match abi {
        PortableAbiType::Vec2
        | PortableAbiType::Vec3
        | PortableAbiType::Vec4
        | PortableAbiType::Quat => {
            for idx in 0..portable_abi_lane_count(abi) {
                let component = emit_vec_component(builder, module, runtime, value, idx)?;
                out.push(lower_value_to_scalar_lane(
                    builder,
                    module,
                    runtime,
                    &PortableAbiType::F32,
                    component,
                    types::F32,
                )?);
            }
        }
        PortableAbiType::Mat3 | PortableAbiType::Mat4 => {
            for idx in 0..portable_abi_lane_count(abi) {
                let component = emit_mat_component(builder, module, runtime, abi, value, idx)?;
                out.push(lower_value_to_scalar_lane(
                    builder,
                    module,
                    runtime,
                    &PortableAbiType::F32,
                    component,
                    types::F32,
                )?);
            }
        }
        PortableAbiType::Array(inner, len) => {
            for idx in 0..*len {
                let item = emit_list_get_value(builder, module, runtime, value, idx)?;
                append_portable_call_arg_lanes(builder, module, runtime, inner, item, out)?;
            }
        }
        PortableAbiType::Struct { fields, .. } => {
            for (idx, field) in fields.iter().enumerate() {
                let field_val = emit_class_get_value(builder, module, runtime, value, field, idx)?;
                append_portable_call_arg_lanes(
                    builder, module, runtime, &field.ty, field_val, out,
                )?;
            }
        }
        PortableAbiType::Value
        | PortableAbiType::Bool
        | PortableAbiType::I32
        | PortableAbiType::U32
        | PortableAbiType::I64
        | PortableAbiType::U64
        | PortableAbiType::F32 => {}
    }
    Ok(())
}

fn emit_portable_function_call(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    callee: cranelift_codegen::ir::FuncRef,
    call_args: &[cranelift_codegen::ir::Value],
    param_abis: &[PortableAbiType],
    ret_abi: &PortableAbiType,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let mut lowered = Vec::new();
    let out_ptr = if portable_abi_uses_sret(ret_abi) {
        let (size, align) = portable_abi_layout(ret_abi);
        let slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size.max(1),
            align_shift_for_bytes(align),
        ));
        let ptr = builder.ins().stack_addr(ptr_ty, slot, 0);
        lowered.push(ptr);
        Some(ptr)
    } else {
        None
    };

    for (arg, abi) in call_args.iter().zip(param_abis.iter()) {
        append_portable_call_arg_lanes(builder, module, runtime, abi, *arg, &mut lowered)?;
    }

    let call_inst = builder.ins().call(callee, &lowered);
    if portable_abi_is_legacy_value(ret_abi) {
        Ok(builder.inst_results(call_inst)[0])
    } else if let Some(kind) = portable_abi_scalar_kind(ret_abi) {
        let raw = builder.inst_results(call_inst)[0];
        box_scalar_lane_to_value(builder, module, runtime, kind, raw)
    } else if let Some(out_ptr) = out_ptr {
        load_portable_aggregate_from_memory(builder, module, runtime, out_ptr, ret_abi, 0)
    } else {
        Ok(builder.ins().iconst(types::I64, nanbox_nil_const()))
    }
}

fn load_scalar_from_memory(
    builder: &mut FunctionBuilder,
    ptr: cranelift_codegen::ir::Value,
    kind: PortableLaneKind,
    offset: u32,
) -> cranelift_codegen::ir::Value {
    builder.ins().load(
        portable_lane_clif_type(kind),
        MemFlags::new(),
        ptr,
        offset as i32,
    )
}

fn load_portable_aggregate_from_memory(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    ptr: cranelift_codegen::ir::Value,
    abi: &PortableAbiType,
    offset: u32,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    if let Some(kind) = portable_abi_scalar_kind(abi) {
        let lane = load_scalar_from_memory(builder, ptr, kind, offset);
        return box_scalar_lane_to_value(builder, module, runtime, kind, lane);
    }

    match abi {
        PortableAbiType::Vec2
        | PortableAbiType::Vec3
        | PortableAbiType::Vec4
        | PortableAbiType::Quat => {
            let lane_count = portable_abi_lane_count(abi);
            let mut lanes = Vec::with_capacity(lane_count);
            for idx in 0..lane_count {
                lanes.push(load_scalar_from_memory(
                    builder,
                    ptr,
                    PortableLaneKind::F32,
                    offset + (idx as u32 * 4),
                ));
            }
            box_portable_param(builder, module, runtime, abi, &lanes, 0)
        }
        PortableAbiType::Mat3 | PortableAbiType::Mat4 => {
            let lane_count = portable_abi_lane_count(abi);
            let mut lanes = Vec::with_capacity(lane_count);
            for idx in 0..lane_count {
                lanes.push(load_scalar_from_memory(
                    builder,
                    ptr,
                    PortableLaneKind::F32,
                    offset + (idx as u32 * 4),
                ));
            }
            box_portable_param(builder, module, runtime, abi, &lanes, 0)
        }
        PortableAbiType::Array(inner, len) => {
            let ptr_ty = module.target_config().pointer_type();
            let len_val = builder.ins().iconst(ptr_ty, *len as i64);
            let func_id = runtime_fn_list_new_local(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(callee, &[len_val]);
            let list = builder.inst_results(call)[0];
            let set_id = runtime_fn_list_set_raw(module, runtime)?;
            let set_callee = module.declare_func_in_func(set_id, builder.func);
            let (inner_size, inner_align) = portable_abi_layout(inner);
            let stride = align_to_u32(inner_size, inner_align);
            for idx in 0..*len {
                let item = load_portable_aggregate_from_memory(
                    builder,
                    module,
                    runtime,
                    ptr,
                    inner,
                    offset + (idx as u32 * stride),
                )?;
                let idx_val = builder.ins().iconst(ptr_ty, idx as i64);
                builder.ins().call(set_callee, &[list, idx_val, item]);
            }
            Ok(list)
        }
        PortableAbiType::Struct {
            class_id, fields, ..
        } => {
            let field_names: Vec<SmolStr> = fields.iter().map(|field| field.name.clone()).collect();
            let (names_ptr, lens_ptr, count_val) =
                build_bytes_arrays(builder, module, runtime, &field_names)?;
            let func_id = runtime_fn_class_new(module, runtime)?;
            let callee = module.declare_func_in_func(func_id, builder.func);
            let class_id_val = builder.ins().iconst(types::I64, *class_id as i64);
            let call = builder
                .ins()
                .call(callee, &[class_id_val, names_ptr, lens_ptr, count_val]);
            let obj = builder.inst_results(call)[0];
            let set_id = runtime_fn_class_set_slot(module, runtime)?;
            let set_callee = module.declare_func_in_func(set_id, builder.func);
            let ptr_ty = module.target_config().pointer_type();
            for (idx, field) in fields.iter().enumerate() {
                let field_val = load_portable_aggregate_from_memory(
                    builder,
                    module,
                    runtime,
                    ptr,
                    &field.ty,
                    offset + field_offset(fields, idx),
                )?;
                let (name_ptr, len_val) =
                    lower_bytes_literal(builder, module, runtime, field.name.as_str())?;
                let slot_val = builder.ins().iconst(ptr_ty, idx as i64);
                builder
                    .ins()
                    .call(set_callee, &[obj, name_ptr, len_val, slot_val, field_val]);
            }
            Ok(obj)
        }
        PortableAbiType::Value => {
            Ok(builder
                .ins()
                .load(types::I64, MemFlags::new(), ptr, offset as i32))
        }
        PortableAbiType::Bool
        | PortableAbiType::I32
        | PortableAbiType::U32
        | PortableAbiType::I64
        | PortableAbiType::U64
        | PortableAbiType::F32 => unreachable!(),
    }
}

fn store_scalar_to_memory(
    builder: &mut FunctionBuilder,
    ptr: cranelift_codegen::ir::Value,
    lane: cranelift_codegen::ir::Value,
    kind: PortableLaneKind,
    offset: u32,
) {
    builder
        .ins()
        .store(MemFlags::new(), lane, ptr, offset as i32);
    let _ = kind;
}

fn store_value_to_portable_aggregate(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    ptr: cranelift_codegen::ir::Value,
    abi: &PortableAbiType,
    value: cranelift_codegen::ir::Value,
    offset: u32,
) -> Result<(), CodegenError> {
    if let Some(kind) = portable_abi_scalar_kind(abi) {
        let lane = lower_value_to_scalar_lane(
            builder,
            module,
            runtime,
            abi,
            value,
            portable_lane_clif_type(kind),
        )?;
        store_scalar_to_memory(builder, ptr, lane, kind, offset);
        return Ok(());
    }

    match abi {
        PortableAbiType::Vec2
        | PortableAbiType::Vec3
        | PortableAbiType::Vec4
        | PortableAbiType::Quat => {
            for idx in 0..portable_abi_lane_count(abi) {
                let component = emit_vec_component(builder, module, runtime, value, idx)?;
                let lane = lower_value_to_scalar_lane(
                    builder,
                    module,
                    runtime,
                    &PortableAbiType::F32,
                    component,
                    types::F32,
                )?;
                store_scalar_to_memory(
                    builder,
                    ptr,
                    lane,
                    PortableLaneKind::F32,
                    offset + (idx as u32 * 4),
                );
            }
        }
        PortableAbiType::Mat3 | PortableAbiType::Mat4 => {
            for idx in 0..portable_abi_lane_count(abi) {
                let component = emit_mat_component(builder, module, runtime, abi, value, idx)?;
                let lane = lower_value_to_scalar_lane(
                    builder,
                    module,
                    runtime,
                    &PortableAbiType::F32,
                    component,
                    types::F32,
                )?;
                store_scalar_to_memory(
                    builder,
                    ptr,
                    lane,
                    PortableLaneKind::F32,
                    offset + (idx as u32 * 4),
                );
            }
        }
        PortableAbiType::Array(inner, len) => {
            let (inner_size, inner_align) = portable_abi_layout(inner);
            let stride = align_to_u32(inner_size, inner_align);
            for idx in 0..*len {
                let item = emit_list_get_value(builder, module, runtime, value, idx)?;
                store_value_to_portable_aggregate(
                    builder,
                    module,
                    runtime,
                    ptr,
                    inner,
                    item,
                    offset + (idx as u32 * stride),
                )?;
            }
        }
        PortableAbiType::Struct { fields, .. } => {
            for (idx, field) in fields.iter().enumerate() {
                let field_val = emit_class_get_value(builder, module, runtime, value, field, idx)?;
                store_value_to_portable_aggregate(
                    builder,
                    module,
                    runtime,
                    ptr,
                    &field.ty,
                    field_val,
                    offset + field_offset(fields, idx),
                )?;
            }
        }
        PortableAbiType::Value => {
            builder
                .ins()
                .store(MemFlags::new(), value, ptr, offset as i32);
        }
        PortableAbiType::Bool
        | PortableAbiType::I32
        | PortableAbiType::U32
        | PortableAbiType::I64
        | PortableAbiType::U64
        | PortableAbiType::F32 => unreachable!(),
    }
    Ok(())
}

fn lower_terminator(
    term: &Terminator,
    abi_return: &PortableAbiType,
    return_ptr: Option<cranelift_codegen::ir::Value>,
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
            if portable_abi_is_legacy_value(abi_return) {
                builder.ins().return_(&[ret]);
            } else if let Some(ret_ty) = portable_abi_scalar_type(abi_return) {
                let lane =
                    lower_value_to_scalar_lane(builder, _module, runtime, abi_return, ret, ret_ty)?;
                builder.ins().return_(&[lane]);
            } else if let Some(return_ptr) = return_ptr {
                store_value_to_portable_aggregate(
                    builder, _module, runtime, return_ptr, abi_return, ret, 0,
                )?;
                builder.ins().return_(&[]);
            } else {
                builder.ins().return_(&[]);
            }
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
            let val = builder.ins().iconst(types::I64, *v);
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

fn runtime_fn_symbol(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    symbol: &'static str,
    params: &[cranelift_codegen::ir::Type],
    returns: &[cranelift_codegen::ir::Type],
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, params, returns);
    runtime.get_func(module, symbol, sig)
}

fn runtime_fn_quat_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_quat_new",
        &[types::I64, types::I64, types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_vec2_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec2_new", sig)
}

fn runtime_fn_vec3_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec3_new", sig)
}

fn runtime_fn_vec4_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, types::I64, types::I64],
        &[types::I64],
    );
    runtime.get_func(module, "wr_vec4_new", sig)
}

fn runtime_fn_vec_component(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_component", sig)
}

fn runtime_fn_mat3_component(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat3_component",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_mat4_component(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat4_component",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_vec_add(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_add", sig)
}

fn runtime_fn_vec_sub(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_sub", sig)
}

fn runtime_fn_vec_mul(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_mul", sig)
}

fn runtime_fn_vec_div(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_div", sig)
}

fn runtime_fn_vec_dot(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_dot", sig)
}

fn runtime_fn_vec_length(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_length", sig)
}

fn runtime_fn_vec_normalize(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_normalize", sig)
}

fn runtime_fn_vec_cross(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_vec_cross", sig)
}

fn runtime_fn_mat3_identity(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(module, runtime, "wr_mat3_identity", &[], &[types::I64])
}

fn runtime_fn_mat3_from_columns(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat3_from_columns",
        &[types::I64, types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_mat4_identity(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_mat4_identity", sig)
}

fn runtime_fn_mat4_from_columns(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, types::I64, types::I64],
        &[types::I64],
    );
    runtime.get_func(module, "wr_mat4_from_columns", sig)
}

fn runtime_fn_mat4_mul_vec4(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_mat4_mul_vec4", sig)
}

fn runtime_fn_mat4_mul_mat4(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_mat4_mul_mat4", sig)
}

fn runtime_fn_mat4_add(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_mat4_add", sig)
}

fn runtime_fn_mat4_sub(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_mat4_sub", sig)
}

fn runtime_fn_mat4_mul_scalar(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_mat4_mul_scalar", sig)
}

fn runtime_fn_mat4_div_scalar(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_mat4_div_scalar", sig)
}

fn runtime_fn_mat3_mul_vec3(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat3_mul_vec3",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_mat3_mul_mat3(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat3_mul_mat3",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_mat3_add(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat3_add",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_mat3_sub(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat3_sub",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_mat3_mul_scalar(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat3_mul_scalar",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_mat3_div_scalar(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_mat3_div_scalar",
        &[types::I64, types::I64],
        &[types::I64],
    )
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

fn runtime_fn_approx_eq(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_approx_eq", sig)
}

fn runtime_fn_cast_f32(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(module, runtime, "wr_cast_f32", &[types::I64], &[types::I64])
}

fn runtime_fn_cast_i32(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(module, runtime, "wr_cast_i32", &[types::I64], &[types::I64])
}

fn runtime_fn_cast_u32(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(module, runtime, "wr_cast_u32", &[types::I64], &[types::I64])
}

fn runtime_fn_math_unary(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    symbol: &'static str,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(module, runtime, symbol, &[types::I64], &[types::I64])
}

fn runtime_fn_math_binary(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    symbol: &'static str,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        symbol,
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_math_ternary(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    symbol: &'static str,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        symbol,
        &[types::I64, types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_gpu_buffer_new(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_gpu_buffer_new",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_gpu_buffer_len(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_gpu_buffer_len",
        &[types::I64],
        &[types::I64],
    )
}

fn runtime_fn_gpu_buffer_get(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_gpu_buffer_get",
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_gpu_buffer_set(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_gpu_buffer_set",
        &[types::I64, types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_gpu_atomic_unary(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    symbol: &'static str,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(module, runtime, symbol, &[types::I64], &[types::I64])
}

fn runtime_fn_gpu_atomic_binary(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    symbol: &'static str,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        symbol,
        &[types::I64, types::I64],
        &[types::I64],
    )
}

fn runtime_fn_gpu_builtin_vector(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
    symbol: &'static str,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(module, runtime, symbol, &[], &[types::I64])
}

fn runtime_fn_gpu_dispatch_begin(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_gpu_dispatch_begin",
        &[types::I64; 7],
        &[types::I64],
    )
}

fn runtime_fn_gpu_dispatch_select_invocation(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(
        module,
        runtime,
        "wr_gpu_dispatch_select_invocation",
        &[types::I64],
        &[types::I64],
    )
}

fn runtime_fn_gpu_dispatch_end(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    runtime_fn_symbol(module, runtime, "wr_gpu_dispatch_end", &[], &[types::I64])
}

fn runtime_builtin_func_id(
    name: &SmolStr,
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<Option<cranelift_module::FuncId>, CodegenError> {
    let func_id = match name.as_str() {
        "__wr_print" => Some(runtime_fn_print(module, runtime)?),
        "assert" => Some(runtime_fn_assert(module, runtime)?),
        "assert_eq" => Some(runtime_fn_assert_eq(module, runtime)?),
        "value_deep_eq" => Some(runtime_fn_value_deep_eq(module, runtime)?),
        "identity_eq" => Some(runtime_fn_identity_eq(module, runtime)?),
        "approx_eq" => Some(runtime_fn_approx_eq(module, runtime)?),
        "__wr_vec_component" => Some(runtime_fn_vec_component(module, runtime)?),
        "vec2" => Some(runtime_fn_vec2_new(module, runtime)?),
        "vec3" => Some(runtime_fn_vec3_new(module, runtime)?),
        "vec4" => Some(runtime_fn_vec4_new(module, runtime)?),
        "quat" => Some(runtime_fn_quat_new(module, runtime)?),
        "mat3_identity" => Some(runtime_fn_mat3_identity(module, runtime)?),
        "mat3_cols" => Some(runtime_fn_mat3_from_columns(module, runtime)?),
        "mat4_identity" => Some(runtime_fn_mat4_identity(module, runtime)?),
        "mat4_cols" => Some(runtime_fn_mat4_from_columns(module, runtime)?),
        "dot" => Some(runtime_fn_vec_dot(module, runtime)?),
        "length" => Some(runtime_fn_vec_length(module, runtime)?),
        "normalize" => Some(runtime_fn_vec_normalize(module, runtime)?),
        "cross" => Some(runtime_fn_vec_cross(module, runtime)?),
        "min" => Some(runtime_fn_math_binary(module, runtime, "wr_vec_min")?),
        "max" => Some(runtime_fn_math_binary(module, runtime, "wr_vec_max")?),
        "clamp" => Some(runtime_fn_math_ternary(module, runtime, "wr_vec_clamp")?),
        "mix" => Some(runtime_fn_math_ternary(module, runtime, "wr_vec_mix")?),
        "abs" => Some(runtime_fn_math_unary(module, runtime, "wr_vec_abs")?),
        "sign" => Some(runtime_fn_math_unary(module, runtime, "wr_vec_sign")?),
        "floor" => Some(runtime_fn_math_unary(module, runtime, "wr_vec_floor")?),
        "ceil" => Some(runtime_fn_math_unary(module, runtime, "wr_vec_ceil")?),
        "fract" => Some(runtime_fn_math_unary(module, runtime, "wr_vec_fract")?),
        "sin" => Some(runtime_fn_math_unary(module, runtime, "wr_vec_sin")?),
        "cos" => Some(runtime_fn_math_unary(module, runtime, "wr_vec_cos")?),
        "sqrt" => Some(runtime_fn_math_unary(module, runtime, "wr_vec_sqrt")?),
        "pow" => Some(runtime_fn_math_binary(module, runtime, "wr_vec_pow")?),
        "distance" => Some(runtime_fn_math_binary(module, runtime, "wr_vec_distance")?),
        "reflect" => Some(runtime_fn_math_binary(module, runtime, "wr_vec_reflect")?),
        "gpu_buffer_new" => Some(runtime_fn_gpu_buffer_new(module, runtime)?),
        "gpu_buffer_len" => Some(runtime_fn_gpu_buffer_len(module, runtime)?),
        "gpu_buffer_get" => Some(runtime_fn_gpu_buffer_get(module, runtime)?),
        "gpu_buffer_set" => Some(runtime_fn_gpu_buffer_set(module, runtime)?),
        "gpu_atomic_i32_new" => Some(runtime_fn_gpu_atomic_unary(
            module,
            runtime,
            "wr_gpu_atomic_i32_new",
        )?),
        "gpu_atomic_i32_drop" => Some(runtime_fn_gpu_atomic_unary(
            module,
            runtime,
            "wr_gpu_atomic_i32_drop",
        )?),
        "gpu_atomic_i32_load" => Some(runtime_fn_gpu_atomic_unary(
            module,
            runtime,
            "wr_gpu_atomic_i32_load",
        )?),
        "gpu_atomic_i32_store" => Some(runtime_fn_gpu_atomic_binary(
            module,
            runtime,
            "wr_gpu_atomic_i32_store",
        )?),
        "gpu_atomic_i32_fetch_add" => Some(runtime_fn_gpu_atomic_binary(
            module,
            runtime,
            "wr_gpu_atomic_i32_fetch_add",
        )?),
        "gpu_atomic_u32_new" => Some(runtime_fn_gpu_atomic_unary(
            module,
            runtime,
            "wr_gpu_atomic_u32_new",
        )?),
        "gpu_atomic_u32_drop" => Some(runtime_fn_gpu_atomic_unary(
            module,
            runtime,
            "wr_gpu_atomic_u32_drop",
        )?),
        "gpu_atomic_u32_load" => Some(runtime_fn_gpu_atomic_unary(
            module,
            runtime,
            "wr_gpu_atomic_u32_load",
        )?),
        "gpu_atomic_u32_store" => Some(runtime_fn_gpu_atomic_binary(
            module,
            runtime,
            "wr_gpu_atomic_u32_store",
        )?),
        "gpu_atomic_u32_fetch_add" => Some(runtime_fn_gpu_atomic_binary(
            module,
            runtime,
            "wr_gpu_atomic_u32_fetch_add",
        )?),
        "global_invocation_id" => Some(runtime_fn_gpu_builtin_vector(
            module,
            runtime,
            "wr_gpu_global_invocation_id",
        )?),
        "local_invocation_id" => Some(runtime_fn_gpu_builtin_vector(
            module,
            runtime,
            "wr_gpu_local_invocation_id",
        )?),
        "workgroup_id" => Some(runtime_fn_gpu_builtin_vector(
            module,
            runtime,
            "wr_gpu_workgroup_id",
        )?),
        "num_workgroups" => Some(runtime_fn_gpu_builtin_vector(
            module,
            runtime,
            "wr_gpu_num_workgroups",
        )?),
        "workgroup_size" => Some(runtime_fn_gpu_builtin_vector(
            module,
            runtime,
            "wr_gpu_workgroup_size",
        )?),
        "gpu_schedule_deterministic" => Some(runtime_fn_symbol(
            module,
            runtime,
            "wr_gpu_schedule_deterministic",
            &[],
            &[types::I64],
        )?),
        "gpu_schedule_reverse" => Some(runtime_fn_symbol(
            module,
            runtime,
            "wr_gpu_schedule_reverse",
            &[],
            &[types::I64],
        )?),
        "gpu_schedule_shuffle" => Some(runtime_fn_symbol(
            module,
            runtime,
            "wr_gpu_schedule_shuffle",
            &[types::I64],
            &[types::I64],
        )?),
        "gpu_schedule_workgroup_reverse" => Some(runtime_fn_symbol(
            module,
            runtime,
            "wr_gpu_schedule_workgroup_reverse",
            &[],
            &[types::I64],
        )?),
        "gpu_schedule_workgroup_shuffle" => Some(runtime_fn_symbol(
            module,
            runtime,
            "wr_gpu_schedule_workgroup_shuffle",
            &[types::I64],
            &[types::I64],
        )?),
        "gpu_schedule_round_robin_workgroups" => Some(runtime_fn_symbol(
            module,
            runtime,
            "wr_gpu_schedule_round_robin_workgroups",
            &[],
            &[types::I64],
        )?),
        "__wr_gpu_dispatch_begin" => Some(runtime_fn_gpu_dispatch_begin(module, runtime)?),
        "__wr_gpu_dispatch_select_invocation" => {
            Some(runtime_fn_gpu_dispatch_select_invocation(module, runtime)?)
        }
        "__wr_gpu_dispatch_end" => Some(runtime_fn_gpu_dispatch_end(module, runtime)?),
        "f32" => Some(runtime_fn_cast_f32(module, runtime)?),
        "i32" => Some(runtime_fn_cast_i32(module, runtime)?),
        "u32" => Some(runtime_fn_cast_u32(module, runtime)?),
        "assert_value_equality" => Some(runtime_fn_assert_value_equality(module, runtime)?),
        "__wr_map_new" => Some(runtime_fn_map_new(module, runtime)?),
        "__wr_map_new_local" => Some(runtime_fn_map_new_local(module, runtime)?),
        "__wr_map_get" => Some(runtime_fn_map_get(module, runtime)?),
        "__wr_map_set" => Some(runtime_fn_map_set(module, runtime)?),
        "__wr_map_len" => Some(runtime_fn_map_len(module, runtime)?),
        "__wr_env_get" => Some(runtime_fn_env_get(module, runtime)?),
        "__wr_env_set" => Some(runtime_fn_env_set(module, runtime)?),
        "__wr_list_get" => Some(runtime_fn_list_get_val(module, runtime)?),
        "__wr_list_set" => Some(runtime_fn_list_set_val(module, runtime)?),
        "__wr_list_new" => Some(runtime_fn_list_new(module, runtime)?),
        "__wr_list_push" => Some(runtime_fn_list_push(module, runtime)?),
        "__wr_list_len" => Some(runtime_fn_list_len(module, runtime)?),
        "__wr_external_call" => Some(runtime_fn_external_call(module, runtime)?),
        "__wr_bytes_from_string" => Some(runtime_fn_bytes_from_string(module, runtime)?),
        "__wr_bytes_from_list" => Some(runtime_fn_bytes_from_list(module, runtime)?),
        "__wr_bytes_to_string" => Some(runtime_fn_bytes_to_string(module, runtime)?),
        "__wr_bytes_to_list" => Some(runtime_fn_bytes_to_list(module, runtime)?),
        "__wr_bytes_len" => Some(runtime_fn_bytes_len(module, runtime)?),
        _ => None,
    };
    Ok(func_id)
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

fn runtime_fn_web_parse_json_text(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_web_parse_json_text", sig)
}

fn runtime_fn_web_render_json_text(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_web_render_json_text", sig)
}

fn runtime_fn_auth_hash_password(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_auth_hash_password", sig)
}

fn runtime_fn_auth_verify_password_hash(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_auth_verify_password_hash", sig)
}

fn runtime_fn_auth_sign_jwt(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_auth_sign_jwt", sig)
}

fn runtime_fn_auth_verify_jwt(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_auth_verify_jwt", sig)
}

fn runtime_fn_auth_generate_secure_token(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_auth_generate_secure_token", sig)
}

fn runtime_fn_auth_render_jwks_document(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_auth_render_jwks_document", sig)
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

fn runtime_fn_list_set_raw(
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

fn runtime_fn_list_get_val(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_list_get_val", sig)
}

fn runtime_fn_list_len(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_list_len", sig)
}

fn runtime_fn_list_set_val(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_list_set_val", sig)
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

fn runtime_fn_metrics_web_writev_calls_id(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_metrics_web_writev_calls_id", sig)
}

fn runtime_fn_metrics_web_sendfile_calls_id(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[], &[types::I64]);
    runtime.get_func(module, "wr_metrics_web_sendfile_calls_id", sig)
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

fn runtime_fn_runtime_configure(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "wr_runtime_configure", sig)
}

fn runtime_fn_db_core_open(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_core_open", sig)
}

fn runtime_fn_db_core_close(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_core_close", sig)
}

fn runtime_fn_db_core_submit_batch(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, types::I64, types::I64, types::I64],
        &[types::I64],
    );
    runtime.get_func(module, "__wr_db_core_submit_batch", sig)
}

fn runtime_fn_db_core_read_point(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_core_read_point", sig)
}

fn runtime_fn_db_core_read_range(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, types::I64, types::I64, types::I64],
        &[types::I64],
    );
    runtime.get_func(module, "__wr_db_core_read_range", sig)
}

fn runtime_fn_db_core_txn_begin(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_core_txn_begin", sig)
}

fn runtime_fn_db_core_txn_prepare(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_core_txn_prepare", sig)
}

fn runtime_fn_db_core_txn_commit(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_core_txn_commit", sig)
}

fn runtime_fn_db_core_txn_abort(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_core_txn_abort", sig)
}

fn runtime_fn_db_admin_snapshot_start(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_admin_snapshot_start", sig)
}

fn runtime_fn_db_admin_snapshot_status(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_admin_snapshot_status", sig)
}

fn runtime_fn_db_admin_restore(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_admin_restore", sig)
}

fn runtime_fn_db_admin_checkpoint_create(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_admin_checkpoint_create", sig)
}

fn runtime_fn_db_admin_checkpoint_restore_latest(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_admin_checkpoint_restore_latest", sig)
}

fn runtime_fn_db_admin_schema_epoch_set(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_admin_schema_epoch_set", sig)
}

fn runtime_fn_db_admin_schema_set_all_voters_on_target_binary(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(
        module,
        "__wr_db_admin_schema_set_all_voters_on_target_binary",
        sig,
    )
}

fn runtime_fn_db_admin_autoscale_tick(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_admin_autoscale_tick", sig)
}

fn runtime_fn_db_admin_plan_rehome(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, types::I64, types::I64],
        &[types::I64],
    );
    runtime.get_func(module, "__wr_db_admin_plan_rehome", sig)
}

fn runtime_fn_db_admin_advance_rehome(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_admin_advance_rehome", sig)
}

fn runtime_fn_db_admin_promote_async_failover(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, types::I64, types::I64],
        &[types::I64],
    );
    runtime.get_func(module, "__wr_db_admin_promote_async_failover", sig)
}

fn runtime_fn_db_explain_checkpoint_count(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_checkpoint_count", sig)
}

fn runtime_fn_db_explain_schema_epoch_get(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_schema_epoch_get", sig)
}

fn runtime_fn_db_explain_health_has_checkpoint_or_schema_error(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(
        module,
        "__wr_db_explain_health_has_checkpoint_or_schema_error",
        sig,
    )
}

fn runtime_fn_db_explain_private_mesh_status(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_private_mesh_status", sig)
}

fn runtime_fn_db_explain_logical_shard_count(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_logical_shard_count", sig)
}

fn runtime_fn_db_explain_active_group_count(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_active_group_count", sig)
}

fn runtime_fn_db_explain_autoscale_status(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_autoscale_status", sig)
}

fn runtime_fn_db_explain_topology_status(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_topology_status", sig)
}

fn runtime_fn_db_explain_shard_map_epoch(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_shard_map_epoch", sig)
}

fn runtime_fn_db_explain_shard_for_key(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_shard_for_key", sig)
}

fn runtime_fn_db_explain_resolve_owner(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_resolve_owner", sig)
}

fn runtime_fn_db_explain_global_route_lookup(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "__wr_db_explain_global_route_lookup", sig)
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

fn runtime_fn_actor_send_0(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_send_0", sig)
}

fn runtime_fn_actor_send_1(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig =
        RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[types::I64]);
    runtime.get_func(module, "wr_actor_send_1", sig)
}

fn runtime_fn_actor_send_2(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, types::I64, types::I64],
        &[types::I64],
    );
    runtime.get_func(module, "wr_actor_send_2", sig)
}

fn runtime_fn_actor_fire(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let ptr_ty = module.target_config().pointer_type();
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, ptr_ty, ptr_ty], &[]);
    runtime.get_func(module, "wr_actor_fire", sig)
}

fn runtime_fn_actor_fire_0(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64], &[]);
    runtime.get_func(module, "wr_actor_fire_0", sig)
}

fn runtime_fn_actor_fire_1(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(module, &[types::I64, types::I64, types::I64], &[]);
    runtime.get_func(module, "wr_actor_fire_1", sig)
}

fn runtime_fn_actor_fire_2(
    module: &mut ObjectModule,
    runtime: &mut RuntimeRegistry,
) -> Result<cranelift_module::FuncId, CodegenError> {
    let sig = RuntimeRegistry::runtime_sig(
        module,
        &[types::I64, types::I64, types::I64, types::I64],
        &[],
    );
    runtime.get_func(module, "wr_actor_fire_2", sig)
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
        MirType::Vec2
        | MirType::Vec3
        | MirType::Vec4
        | MirType::Quat
        | MirType::Mat3
        | MirType::Mat4 => Ok(types::I64),
    }
}

#[cfg(test)]
mod tests {
    use super::{compile_to_object, runtime_numeric_symbol};
    use crate::hir::{BinaryOp, Literal};
    use crate::mir::ir::{
        BasicBlock, BlockId, CallKind, CallTarget, MirFunction, MirModule, MirType, Place,
        PortableAbiType, Rvalue, Stmt, Temp, TempId, Terminator, Value,
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
            abi_params: Vec::new(),
            abi_return: PortableAbiType::Value,
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
