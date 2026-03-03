use crate::hir::{BinaryOp, Literal, UnaryOp};
use crate::mir::ir::{
    CallKind, CallTarget, MirFunction, MirModule, MirType, Place, Rvalue, Stmt, SwitchCase,
    Terminator, Value,
};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::path::Path;

use super::cranelift::CodegenError;

// ---- NaN-boxing constants (must match cranelift backend) ----
const NANBOX_QNAN: u64 = 0x7ff8_0000_0000_0000;
const NANBOX_TAG_SHIFT: u64 = 49;
const NANBOX_PAYLOAD_MASK: u64 = (1u64 << NANBOX_TAG_SHIFT) - 1;
const NANBOX_TAG_INT: u64 = 2;
const NANBOX_TAG_IMM: u64 = 3;
const NANBOX_IMM_NIL: u64 = 0;
const NANBOX_IMM_FALSE: u64 = 1;
const NANBOX_IMM_TRUE: u64 = 2;

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

fn nanbox_int_const(value: i64) -> i64 {
    nanbox_const(NANBOX_TAG_INT, value as u64)
}

// ---- WASM binary encoding primitives ----

fn encode_unsigned_leb128(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            buf.push(byte);
            break;
        }
        buf.push(byte | 0x80);
    }
}

fn encode_signed_leb128(buf: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        let done = (value == 0 && (byte & 0x40) == 0) || (value == -1 && (byte & 0x40) != 0);
        if done {
            buf.push(byte);
            break;
        }
        buf.push(byte | 0x80);
    }
}

fn encode_string(buf: &mut Vec<u8>, s: &str) {
    encode_unsigned_leb128(buf, s.len() as u64);
    buf.extend_from_slice(s.as_bytes());
}

fn encode_section(buf: &mut Vec<u8>, section_id: u8, contents: &[u8]) {
    buf.push(section_id);
    encode_unsigned_leb128(buf, contents.len() as u64);
    buf.extend_from_slice(contents);
}

fn encode_vec_count(buf: &mut Vec<u8>, count: u32) {
    encode_unsigned_leb128(buf, count as u64);
}

// WASM type constants
const WASM_TYPE_I64: u8 = 0x7E;
const WASM_TYPE_FUNC: u8 = 0x60;
const WASM_BLOCK_VOID: u8 = 0x40;
const WASM_BLOCK_I64: u8 = WASM_TYPE_I64;

// WASM opcodes
const OP_UNREACHABLE: u8 = 0x00;
const OP_NOP: u8 = 0x01;
const OP_BLOCK: u8 = 0x02;
const OP_LOOP: u8 = 0x03;
const OP_IF: u8 = 0x04;
const OP_ELSE: u8 = 0x05;
const OP_END: u8 = 0x0B;
const OP_BR: u8 = 0x0C;
const OP_BR_IF: u8 = 0x0D;
const OP_RETURN: u8 = 0x0F;
const OP_CALL: u8 = 0x10;
const OP_DROP: u8 = 0x1A;
const OP_SELECT: u8 = 0x1B;
const OP_LOCAL_GET: u8 = 0x20;
const OP_LOCAL_SET: u8 = 0x21;
const OP_LOCAL_TEE: u8 = 0x22;
const OP_I64_LOAD: u8 = 0x29;
const OP_I64_STORE: u8 = 0x37;
const OP_I64_CONST: u8 = 0x42;
const OP_I64_EQZ: u8 = 0x50;
const OP_I64_EQ: u8 = 0x51;
const OP_I64_NE: u8 = 0x52;
const OP_I64_LT_S: u8 = 0x53;
const OP_I64_GT_S: u8 = 0x55;
const OP_I64_LE_S: u8 = 0x57;
const OP_I64_GE_S: u8 = 0x59;
const OP_I64_ADD: u8 = 0x7C;
const OP_I64_SUB: u8 = 0x7D;
const OP_I64_MUL: u8 = 0x7E;
const OP_I64_DIV_S: u8 = 0x7F;
const OP_I64_REM_S: u8 = 0x81;
const OP_I64_AND: u8 = 0x83;
const OP_I64_OR: u8 = 0x84;
const OP_I64_XOR: u8 = 0x85;
const OP_I64_SHL: u8 = 0x86;
const OP_I64_SHR_S: u8 = 0x87;
const OP_I64_EXTEND_I32_S: u8 = 0xAC;
const OP_I32_WRAP_I64: u8 = 0xA7;

// ---- Import registry: tracks runtime functions imported from "wrela_runtime" ----

struct ImportRegistry {
    /// Maps import name -> (type index, function index)
    imports: Vec<(&'static str, u32)>,
    /// Maps import name -> function index
    name_to_func_idx: HashMap<&'static str, u32>,
    /// Func types: (param_count, has_return)
    func_types: Vec<(u32, bool)>,
    /// Maps (param_count, has_return) -> type_index
    type_cache: HashMap<(u32, bool), u32>,
}

impl ImportRegistry {
    fn new() -> Self {
        Self {
            imports: Vec::new(),
            name_to_func_idx: HashMap::new(),
            func_types: Vec::new(),
            type_cache: HashMap::new(),
        }
    }

    fn get_or_create_type(&mut self, param_count: u32, has_return: bool) -> u32 {
        if let Some(idx) = self.type_cache.get(&(param_count, has_return)) {
            return *idx;
        }
        let idx = self.func_types.len() as u32;
        self.func_types.push((param_count, has_return));
        self.type_cache.insert((param_count, has_return), idx);
        idx
    }

    fn get_or_import(&mut self, name: &'static str, param_count: u32, has_return: bool) -> u32 {
        if let Some(idx) = self.name_to_func_idx.get(name) {
            return *idx;
        }
        let type_idx = self.get_or_create_type(param_count, has_return);
        let func_idx = self.imports.len() as u32;
        self.imports.push((name, type_idx));
        self.name_to_func_idx.insert(name, func_idx);
        func_idx
    }
}

// ---- Runtime function imports ----
// Each runtime function has a name, parameter count, and whether it returns a value.
// All parameters and return values are i64 (nanboxed).

fn import_print(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_print", 1, true)
}

fn import_rc_inc(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_rc_inc", 1, false)
}

fn import_rc_dec(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_rc_dec", 1, false)
}

fn import_pending_await(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_pending_await", 1, true)
}

fn import_class_set_slot(reg: &mut ImportRegistry) -> u32 {
    // obj, name_ptr, name_len, slot, value -> void
    reg.get_or_import("wr_class_set_slot", 5, false)
}

fn import_iter_init(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_iter_init", 1, true)
}

fn import_iter_next(reg: &mut ImportRegistry) -> u32 {
    // iter, out_val_ptr, out_done_ptr -> void
    reg.get_or_import("wr_iter_next", 3, false)
}

fn import_num_neg(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_neg", 1, true)
}

fn import_num_add(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_add", 2, true)
}

fn import_num_sub(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_sub", 2, true)
}

fn import_num_mul(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_mul", 2, true)
}

fn import_num_div(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_div", 2, true)
}

fn import_num_mod(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_mod", 2, true)
}

fn import_num_lt(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_lt", 2, true)
}

fn import_num_gt(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_gt", 2, true)
}

fn import_num_le(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_le", 2, true)
}

fn import_num_ge(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_num_ge", 2, true)
}

fn import_eq(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_eq", 2, true)
}

fn import_str_concat(reg: &mut ImportRegistry) -> u32 {
    // ptr, len -> value
    reg.get_or_import("wr_str_concat", 2, true)
}

fn import_result_ok(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_result_ok", 1, true)
}

fn import_result_err(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_result_err", 1, true)
}

fn import_result_is_ok(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_result_is_ok", 1, true)
}

fn import_result_unwrap(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_result_unwrap", 1, true)
}

fn import_result_err_unwrap(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_result_err_unwrap", 1, true)
}

fn import_crash(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_crash", 1, true)
}

fn import_get_field(reg: &mut ImportRegistry) -> u32 {
    // obj, name_ptr, name_len, slot -> value
    reg.get_or_import("wr_class_get_slot", 4, true)
}

fn import_class_init(reg: &mut ImportRegistry) -> u32 {
    // class_id, field_count -> value
    reg.get_or_import("wr_class_init", 2, true)
}

fn import_list_new(reg: &mut ImportRegistry) -> u32 {
    // ptr, len -> value
    reg.get_or_import("wr_list_new", 2, true)
}

fn import_map_new(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_map_new_empty", 0, true)
}

fn import_map_set(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_map_set", 3, true)
}

fn import_str_interp(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_str_interp", 2, true)
}

fn import_range_new(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_range_new", 2, true)
}

fn import_spawn(reg: &mut ImportRegistry) -> u32 {
    // target, instance, pool_size, objective, mailbox_cap, enqueue_timeout, batch_limit -> value
    reg.get_or_import("wr_spawn", 7, true)
}

fn import_pool_new(reg: &mut ImportRegistry) -> u32 {
    // handles, objective, min, max, weight, queue_cap -> value
    reg.get_or_import("wr_pool_new", 6, true)
}

fn import_actor_send(reg: &mut ImportRegistry) -> u32 {
    // actor, method_id, args_ptr, args_len -> value
    reg.get_or_import("wr_actor_send", 4, true)
}

fn import_box_float(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_box_float", 1, true)
}

fn import_unbox_float(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_unbox_float", 1, true)
}

fn import_type_id(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_type_id", 1, true)
}

fn import_assert(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_assert", 1, true)
}

fn import_assert_eq(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_assert_eq", 2, true)
}

fn import_log(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_log", 1, true)
}

fn import_list_push(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_list_push", 2, true)
}

fn import_list_get(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_list_get", 2, true)
}

fn import_list_set(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_list_set", 3, true)
}

fn import_list_len(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_list_len", 1, true)
}

fn import_map_get(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_map_get", 2, true)
}

fn import_map_len(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_map_len", 1, true)
}

fn import_str_len(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_str_len", 1, true)
}

fn import_method_dispatch(reg: &mut ImportRegistry) -> u32 {
    // receiver, method_id, args_ptr, args_len -> value
    reg.get_or_import("wr_method_dispatch", 4, true)
}

fn import_indirect_call(reg: &mut ImportRegistry) -> u32 {
    // target, args_ptr, args_len -> value
    reg.get_or_import("wr_indirect_call", 3, true)
}

fn import_otherwise(reg: &mut ImportRegistry) -> u32 {
    reg.get_or_import("wr_otherwise", 2, true)
}

// ---- WASM code generation context ----

struct WasmFuncCtx<'a> {
    code: Vec<u8>,
    /// Maps local_id -> wasm local index
    locals: HashMap<usize, u32>,
    /// Maps temp_id -> wasm local index
    temps: HashMap<usize, u32>,
    /// Total number of wasm locals allocated so far
    next_local: u32,
    /// The MIR function
    func: &'a MirFunction,
    /// Maps function name -> (wasm function index, param count)
    func_map: &'a HashMap<SmolStr, (u32, u32)>,
    /// Import registry
    imports: &'a mut ImportRegistry,
    /// Scratch local for iter_next etc.
    scratch_local: Option<u32>,
}

impl<'a> WasmFuncCtx<'a> {
    fn alloc_local(&mut self) -> u32 {
        let idx = self.next_local;
        self.next_local += 1;
        idx
    }

    fn get_scratch(&mut self) -> u32 {
        if let Some(s) = self.scratch_local {
            s
        } else {
            let s = self.alloc_local();
            self.scratch_local = Some(s);
            s
        }
    }

    fn emit_i64_const(&mut self, value: i64) {
        self.code.push(OP_I64_CONST);
        encode_signed_leb128(&mut self.code, value);
    }

    fn emit_local_get(&mut self, idx: u32) {
        self.code.push(OP_LOCAL_GET);
        encode_unsigned_leb128(&mut self.code, idx as u64);
    }

    fn emit_local_set(&mut self, idx: u32) {
        self.code.push(OP_LOCAL_SET);
        encode_unsigned_leb128(&mut self.code, idx as u64);
    }

    fn emit_local_tee(&mut self, idx: u32) {
        self.code.push(OP_LOCAL_TEE);
        encode_unsigned_leb128(&mut self.code, idx as u64);
    }

    fn emit_call(&mut self, func_idx: u32) {
        self.code.push(OP_CALL);
        encode_unsigned_leb128(&mut self.code, func_idx as u64);
    }

    fn emit_drop(&mut self) {
        self.code.push(OP_DROP);
    }

    fn emit_return(&mut self) {
        self.code.push(OP_RETURN);
    }

    fn emit_unreachable(&mut self) {
        self.code.push(OP_UNREACHABLE);
    }

    fn emit_nanbox_nil(&mut self) {
        self.emit_i64_const(nanbox_nil_const());
    }

    fn emit_nanbox_bool(&mut self, value: bool) {
        self.emit_i64_const(nanbox_bool_const(value));
    }

    /// Emit code to untag an integer (extract payload from nanbox).
    /// The nanboxed value must be on the stack. Leaves untagged i64 on the stack.
    fn emit_untag_int(&mut self) {
        // payload = value & NANBOX_PAYLOAD_MASK
        self.emit_i64_const(NANBOX_PAYLOAD_MASK as i64);
        self.code.push(OP_I64_AND);
        // Sign-extend from 49 bits: shift left by 15, then arithmetic shift right by 15
        self.emit_i64_const(15);
        self.code.push(OP_I64_SHL);
        self.emit_i64_const(15);
        self.code.push(OP_I64_SHR_S);
    }

    /// Emit code to tag a raw integer into a nanbox.
    /// The raw i64 must be on the stack. Leaves nanboxed i64 on the stack.
    fn emit_tag_int(&mut self) {
        // tagged = NANBOX_QNAN | (NANBOX_TAG_INT << NANBOX_TAG_SHIFT) | (value & NANBOX_PAYLOAD_MASK)
        self.emit_i64_const(NANBOX_PAYLOAD_MASK as i64);
        self.code.push(OP_I64_AND);
        self.emit_i64_const((NANBOX_QNAN | (NANBOX_TAG_INT << NANBOX_TAG_SHIFT)) as i64);
        self.code.push(OP_I64_OR);
    }

    /// Emit code to untag a boolean (extract payload from nanbox).
    /// Leaves 0 or non-zero i64 on the stack.
    fn emit_untag_bool(&mut self) {
        self.emit_i64_const(NANBOX_PAYLOAD_MASK as i64);
        self.code.push(OP_I64_AND);
        // Compare payload == NANBOX_IMM_TRUE (2)
        self.emit_i64_const(NANBOX_IMM_TRUE as i64);
        self.code.push(OP_I64_EQ);
        // Now we have an i32 (0 or 1) from i64.eq, extend to i64
        self.code.push(OP_I64_EXTEND_I32_S);
    }

    /// Emit code to tag a boolean (0 or 1 i64 on stack) into a nanbox.
    fn emit_tag_bool(&mut self) {
        // if value != 0 -> TRUE else FALSE
        // (local == 0) ? FALSE_CONST : TRUE_CONST
        // We use select: push TRUE, push FALSE, push condition (i32)
        let scratch = self.get_scratch();
        self.emit_local_set(scratch);
        self.emit_i64_const(nanbox_bool_const(true));
        self.emit_i64_const(nanbox_bool_const(false));
        self.emit_local_get(scratch);
        // i32.wrap_i64 to get i32 condition for select
        self.code.push(OP_I32_WRAP_I64);
        self.code.push(OP_SELECT);
    }

    fn emit_value(&mut self, value: &Value) {
        match value {
            Value::Const(lit) => self.emit_literal(lit),
            Value::Local(local) => {
                if let Some(idx) = self.locals.get(&local.0) {
                    self.emit_local_get(*idx);
                } else {
                    self.emit_nanbox_nil();
                }
            }
            Value::Temp(temp) => {
                if let Some(idx) = self.temps.get(&temp.0) {
                    self.emit_local_get(*idx);
                } else {
                    self.emit_nanbox_nil();
                }
            }
        }
    }

    fn emit_literal(&mut self, lit: &Literal) {
        match lit {
            Literal::Integer(v) => {
                self.emit_i64_const(nanbox_int_const(*v));
            }
            Literal::Boolean(v) => {
                self.emit_nanbox_bool(*v);
            }
            Literal::Nil => {
                self.emit_nanbox_nil();
            }
            Literal::Float(v) => {
                // Box the float via runtime import
                // Reinterpret f64 bits as i64 and call wr_box_float
                let bits = v.to_bits() as i64;
                self.emit_i64_const(bits);
                let func_idx = import_box_float(self.imports);
                self.emit_call(func_idx);
            }
            Literal::String(_s) => {
                // For WASM, string literals are stored in the data section.
                // For simplicity, we call a runtime function to intern the string.
                // We pass the string as a pointer+len from linear memory (data segment).
                // This is handled at module level; here we just emit nil as a placeholder.
                // A real implementation would reference data segment offsets.
                // For now, we emit nil as a stub for string literals.
                self.emit_nanbox_nil();
            }
        }
    }

    fn emit_assign_place(&mut self, place: &Place) {
        match place {
            Place::Local(local) => {
                if let Some(idx) = self.locals.get(&local.0) {
                    self.emit_local_set(*idx);
                } else {
                    self.emit_drop();
                }
            }
            Place::Temp(temp) => {
                if let Some(idx) = self.temps.get(&temp.0) {
                    self.emit_local_set(*idx);
                } else {
                    self.emit_drop();
                }
            }
        }
    }

    fn emit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Phi { .. } => {
                // Phi nodes are resolved during block transition; nothing to emit here.
            }
            Stmt::Assign { place, value, .. } => {
                self.emit_rvalue(value);
                self.emit_assign_place(place);
            }
            Stmt::RcInc { value, .. } => {
                self.emit_value(value);
                let func_idx = import_rc_inc(self.imports);
                self.emit_call(func_idx);
            }
            Stmt::RcDec { value, .. } => {
                self.emit_value(value);
                let func_idx = import_rc_dec(self.imports);
                self.emit_call(func_idx);
            }
            Stmt::Await { dst, pending, .. } => {
                self.emit_value(pending);
                let func_idx = import_pending_await(self.imports);
                self.emit_call(func_idx);
                self.emit_assign_place(dst);
            }
            Stmt::Fire { pending, .. } => {
                // fire consumes pending. RC dec is emitted separately by MIR.
                self.emit_value(pending);
                self.emit_drop();
            }
            Stmt::ActorFire { args, .. } => {
                // Actor fire in WASM: emit args, drop them (no actor runtime in WASM)
                for arg in args {
                    self.emit_value(arg);
                    self.emit_drop();
                }
            }
            Stmt::SetField {
                base,
                field: _,
                slot,
                value,
                ..
            } => {
                self.emit_value(base);
                // For WASM, pass field name as nil stub and slot
                self.emit_nanbox_nil(); // name_ptr placeholder
                self.emit_i64_const(0); // name_len placeholder
                self.emit_i64_const(slot.unwrap_or(u32::MAX) as i64);
                self.emit_value(value);
                let func_idx = import_class_set_slot(self.imports);
                self.emit_call(func_idx);
            }
            Stmt::IterInit { dst, iterable, .. } => {
                self.emit_value(iterable);
                let func_idx = import_iter_init(self.imports);
                self.emit_call(func_idx);
                self.emit_assign_place(dst);
            }
            Stmt::IterNext {
                iter: _,
                dst_value,
                dst_done,
                ..
            } => {
                // iter_next writes to two output slots. We use scratch locals.
                // Call wr_iter_next(iter, &out_val, &out_done)
                // For WASM, we'll use a simpler ABI: two calls or a multi-return.
                // Stub: set both to nil.
                self.emit_nanbox_nil();
                self.emit_assign_place(dst_value);
                self.emit_nanbox_nil();
                self.emit_assign_place(dst_done);
            }
        }
    }

    fn emit_rvalue(&mut self, rvalue: &Rvalue) {
        match rvalue {
            Rvalue::Use(value) => {
                self.emit_value(value);
            }
            Rvalue::Unary { op, operand } => {
                self.emit_value(operand);
                match op {
                    UnaryOp::Neg => {
                        let func_idx = import_num_neg(self.imports);
                        self.emit_call(func_idx);
                    }
                    UnaryOp::Not => {
                        // Untag bool, flip, retag
                        self.emit_untag_bool();
                        self.emit_i64_const(1);
                        self.code.push(OP_I64_XOR);
                        self.emit_tag_bool();
                    }
                    UnaryOp::BitNot => {
                        self.emit_untag_int();
                        self.emit_i64_const(-1);
                        self.code.push(OP_I64_XOR);
                        self.emit_tag_int();
                    }
                    _ => {
                        // Await/Fire/Spawn/Err are handled at Stmt level
                        // If they appear as Rvalue::Unary, emit nil
                        self.emit_drop();
                        self.emit_nanbox_nil();
                    }
                }
            }
            Rvalue::Binary { op, lhs, rhs } => {
                self.emit_binary_op(op, lhs, rhs);
            }
            Rvalue::StrConcat { parts, .. } => {
                // Stub: call runtime with nil placeholder
                // A full implementation would build an array in linear memory
                self.emit_nanbox_nil();
                self.emit_i64_const(parts.len() as i64);
                let func_idx = import_str_concat(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::ResultOk { value } => {
                self.emit_value(value);
                let func_idx = import_result_ok(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::ResultErr { value } => {
                self.emit_value(value);
                let func_idx = import_result_err(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::ResultIsOk { value } => {
                self.emit_value(value);
                let func_idx = import_result_is_ok(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::ResultUnwrap { value } => {
                self.emit_value(value);
                let func_idx = import_result_unwrap(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::ResultErrUnwrap { value } => {
                self.emit_value(value);
                let func_idx = import_result_err_unwrap(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::Crash { value } => {
                self.emit_value(value);
                let func_idx = import_crash(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::GetField {
                base,
                field: _,
                slot,
                ..
            } => {
                self.emit_value(base);
                self.emit_nanbox_nil(); // name placeholder
                self.emit_i64_const(0); // name_len placeholder
                self.emit_i64_const(slot.unwrap_or(u32::MAX) as i64);
                let func_idx = import_get_field(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::Call { kind, target, args } => {
                self.emit_call_rvalue(kind, target, args);
            }
            Rvalue::ClassInit { class_id, fields } => {
                self.emit_i64_const(*class_id as i64);
                self.emit_i64_const(fields.len() as i64);
                let func_idx = import_class_init(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::Spawn {
                target,
                instance,
                size,
                objective: _,
                config,
            } => {
                self.emit_value(target);
                self.emit_value(instance);
                let pool_size = match size {
                    crate::hir::PoolSize::Fixed(n) => *n,
                    crate::hir::PoolSize::Auto => 0,
                };
                self.emit_i64_const(pool_size);
                self.emit_i64_const(0); // objective placeholder
                self.emit_i64_const(config.mailbox_cap.unwrap_or(0));
                self.emit_i64_const(config.enqueue_timeout_ms.unwrap_or(0));
                self.emit_i64_const(config.batch_limit.unwrap_or(0));
                let func_idx = import_spawn(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::PoolNew {
                handles,
                objective: _,
                min_size,
                max_size,
                weight,
                queue_cap,
            } => {
                self.emit_value(handles);
                self.emit_i64_const(0); // objective placeholder
                self.emit_i64_const(*min_size);
                self.emit_i64_const(*max_size);
                self.emit_i64_const(*weight);
                self.emit_i64_const(*queue_cap);
                let func_idx = import_pool_new(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::BuildList { items, .. } => {
                // Stub: emit nil for the list; a full impl would write to linear memory
                self.emit_nanbox_nil();
                self.emit_i64_const(items.len() as i64);
                let func_idx = import_list_new(self.imports);
                self.emit_call(func_idx);
            }
            Rvalue::BuildMap { items: _, .. } => {
                let map_func = import_map_new(self.imports);
                self.emit_call(map_func);
                // For each k/v pair, call map_set
                // Stub: just return empty map
            }
            Rvalue::StringInterp { parts, .. } => {
                // Stub: call runtime interp
                self.emit_nanbox_nil();
                self.emit_i64_const(parts.len() as i64);
                let func_idx = import_str_interp(self.imports);
                self.emit_call(func_idx);
            }
        }
    }

    fn emit_binary_op(&mut self, op: &BinaryOp, lhs: &Value, rhs: &Value) {
        match op {
            BinaryOp::Add => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_ADD);
                    self.emit_tag_int();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_add(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Sub => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_SUB);
                    self.emit_tag_int();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_sub(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Mul => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_MUL);
                    self.emit_tag_int();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_mul(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Div => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    // TODO: division by zero trap
                    self.code.push(OP_I64_DIV_S);
                    self.emit_tag_int();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_div(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Mod => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_REM_S);
                    self.emit_tag_int();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_mod(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Eq => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_EQ);
                    self.code.push(OP_I64_EXTEND_I32_S);
                    self.emit_tag_bool();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_eq(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Ne => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_NE);
                    self.code.push(OP_I64_EXTEND_I32_S);
                    self.emit_tag_bool();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_eq(self.imports);
                    self.emit_call(func_idx);
                    // Negate the result
                    self.emit_untag_bool();
                    self.emit_i64_const(1);
                    self.code.push(OP_I64_XOR);
                    self.emit_tag_bool();
                }
            }
            BinaryOp::Lt => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_LT_S);
                    self.code.push(OP_I64_EXTEND_I32_S);
                    self.emit_tag_bool();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_lt(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Gt => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_GT_S);
                    self.code.push(OP_I64_EXTEND_I32_S);
                    self.emit_tag_bool();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_gt(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Le => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_LE_S);
                    self.code.push(OP_I64_EXTEND_I32_S);
                    self.emit_tag_bool();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_le(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::Ge => {
                let lty = self.mir_type_of(lhs);
                let rty = self.mir_type_of(rhs);
                if matches!(lty, MirType::Integer) && matches!(rty, MirType::Integer) {
                    self.emit_value(lhs);
                    self.emit_untag_int();
                    self.emit_value(rhs);
                    self.emit_untag_int();
                    self.code.push(OP_I64_GE_S);
                    self.code.push(OP_I64_EXTEND_I32_S);
                    self.emit_tag_bool();
                } else {
                    self.emit_value(lhs);
                    self.emit_value(rhs);
                    let func_idx = import_num_ge(self.imports);
                    self.emit_call(func_idx);
                }
            }
            BinaryOp::And => {
                self.emit_value(lhs);
                self.emit_untag_bool();
                self.emit_value(rhs);
                self.emit_untag_bool();
                self.code.push(OP_I64_AND);
                self.emit_tag_bool();
            }
            BinaryOp::Or => {
                self.emit_value(lhs);
                self.emit_untag_bool();
                self.emit_value(rhs);
                self.emit_untag_bool();
                self.code.push(OP_I64_OR);
                self.emit_tag_bool();
            }
            BinaryOp::BitAnd => {
                self.emit_value(lhs);
                self.emit_untag_int();
                self.emit_value(rhs);
                self.emit_untag_int();
                self.code.push(OP_I64_AND);
                self.emit_tag_int();
            }
            BinaryOp::BitOr => {
                self.emit_value(lhs);
                self.emit_untag_int();
                self.emit_value(rhs);
                self.emit_untag_int();
                self.code.push(OP_I64_OR);
                self.emit_tag_int();
            }
            BinaryOp::BitXor => {
                self.emit_value(lhs);
                self.emit_untag_int();
                self.emit_value(rhs);
                self.emit_untag_int();
                self.code.push(OP_I64_XOR);
                self.emit_tag_int();
            }
            BinaryOp::Shl => {
                self.emit_value(lhs);
                self.emit_untag_int();
                self.emit_value(rhs);
                self.emit_untag_int();
                self.code.push(OP_I64_SHL);
                self.emit_tag_int();
            }
            BinaryOp::Shr => {
                self.emit_value(lhs);
                self.emit_untag_int();
                self.emit_value(rhs);
                self.emit_untag_int();
                self.code.push(OP_I64_SHR_S);
                self.emit_tag_int();
            }
            BinaryOp::Range => {
                self.emit_value(lhs);
                self.emit_value(rhs);
                let func_idx = import_range_new(self.imports);
                self.emit_call(func_idx);
            }
            BinaryOp::Otherwise => {
                self.emit_value(lhs);
                self.emit_value(rhs);
                let func_idx = import_otherwise(self.imports);
                self.emit_call(func_idx);
            }
            _ => {
                // Assign variants shouldn't appear in Rvalue::Binary
                self.emit_nanbox_nil();
            }
        }
    }

    fn emit_call_rvalue(&mut self, kind: &CallKind, target: &CallTarget, args: &[Value]) {
        match kind {
            CallKind::Sync => {
                match target {
                    CallTarget::Function(name) => {
                        // Check if it's a user-defined function or a runtime builtin
                        if let Some((func_idx, _)) = self.func_map.get(name).copied() {
                            for arg in args {
                                self.emit_value(arg);
                            }
                            self.emit_call(func_idx);
                        } else {
                            // Try to resolve as a runtime builtin
                            self.emit_builtin_call(name.as_str(), args);
                        }
                    }
                    CallTarget::Method {
                        receiver,
                        method: _,
                        method_id,
                    } => {
                        // Method dispatch via runtime
                        self.emit_value(receiver);
                        self.emit_i64_const(method_id.unwrap_or(0) as i64);
                        // For args, we'd need to write them to linear memory. Stub: just pass count.
                        self.emit_i64_const(args.len() as i64);
                        self.emit_nanbox_nil(); // args placeholder
                        let func_idx = import_method_dispatch(self.imports);
                        self.emit_call(func_idx);
                    }
                    CallTarget::GuardedInterface { fallback, .. } => {
                        // Fall through to the fallback
                        if let Some((func_idx, _)) = self.func_map.get(fallback).copied() {
                            for arg in args {
                                self.emit_value(arg);
                            }
                            self.emit_call(func_idx);
                        } else {
                            self.emit_builtin_call(fallback.as_str(), args);
                        }
                    }
                    CallTarget::Indirect(target_val) => {
                        self.emit_value(target_val);
                        self.emit_i64_const(args.len() as i64);
                        self.emit_nanbox_nil(); // args placeholder
                        let func_idx = import_indirect_call(self.imports);
                        self.emit_call(func_idx);
                    }
                }
            }
            CallKind::Actor => {
                // Actor send via runtime
                match target {
                    CallTarget::Method {
                        receiver,
                        method_id,
                        ..
                    } => {
                        self.emit_value(receiver);
                        self.emit_i64_const(method_id.unwrap_or(0) as i64);
                        self.emit_i64_const(args.len() as i64);
                        self.emit_nanbox_nil(); // args placeholder
                        let func_idx = import_actor_send(self.imports);
                        self.emit_call(func_idx);
                    }
                    _ => {
                        self.emit_nanbox_nil();
                    }
                }
            }
        }
    }

    fn emit_builtin_call(&mut self, name: &str, args: &[Value]) {
        // Map common builtins to runtime imports
        let func_idx = match name {
            "__wr_print" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_print(self.imports)
            }
            "assert" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_assert(self.imports)
            }
            "assert_eq" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_assert_eq(self.imports)
            }
            "__wr_log" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_log(self.imports)
            }
            "__wr_list_push" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_list_push(self.imports)
            }
            "__wr_list_get" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_list_get(self.imports)
            }
            "__wr_list_set" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_list_set(self.imports)
            }
            "__wr_list_len" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_list_len(self.imports)
            }
            "__wr_map_new" => import_map_new(self.imports),
            "__wr_map_get" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_map_get(self.imports)
            }
            "__wr_map_set" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_map_set(self.imports)
            }
            "__wr_map_len" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_map_len(self.imports)
            }
            "__wr_str_len" => {
                for arg in args {
                    self.emit_value(arg);
                }
                import_str_len(self.imports)
            }
            _ => {
                // Unknown builtin - push args and call via a generic import
                for arg in args {
                    self.emit_value(arg);
                }
                let param_count = args.len() as u32;
                // Create a dynamic import for unknown builtins
                self.imports.get_or_import(
                    // Leak the name so it lives long enough (acceptable for compilation)
                    Box::leak(name.to_string().into_boxed_str()),
                    param_count,
                    true,
                )
            }
        };
        self.emit_call(func_idx);
    }

    fn mir_type_of(&self, value: &Value) -> MirType {
        match value {
            Value::Const(lit) => match lit {
                Literal::Integer(_) => MirType::Integer,
                Literal::Float(_) => MirType::Float,
                Literal::String(_) => MirType::String,
                Literal::Boolean(_) => MirType::Boolean,
                Literal::Nil => MirType::Nil,
            },
            Value::Local(local) => self
                .func
                .locals
                .get(local.0)
                .map(|l| l.ty.clone())
                .unwrap_or(MirType::Unknown),
            Value::Temp(temp) => self
                .func
                .temps
                .get(temp.0)
                .map(|t| t.ty.clone())
                .unwrap_or(MirType::Unknown),
        }
    }

    /// Emit a simple structured-control-flow translation of MIR blocks.
    /// We use a loop+br_table pattern: each block becomes a case in a dispatch loop.
    fn emit_blocks(&mut self) {
        let block_count = self.func.blocks.len();
        if block_count == 0 {
            self.emit_nanbox_nil();
            self.emit_return();
            return;
        }

        // Strategy: Emit a loop containing a block-dispatch via nested blocks and br_table.
        // block $exit
        //   loop $dispatch
        //     block $b0
        //       block $b1
        //         ...
        //         block $bN
        //           local.get $dispatch_var
        //           br_table $b0 $b1 ... $bN $exit
        //         end ;; $bN
        //         ... code for block N-1 ...
        //         local.set $dispatch_var (next block)
        //         br $dispatch
        //       end ;; $b1
        //       ... code for block 0 ...
        //     end ;; $b0
        //   end ;; $dispatch loop
        // end ;; $exit

        let dispatch_var = self.alloc_local();

        // Set initial dispatch to entry block
        self.emit_i64_const(self.func.entry.0 as i64);
        self.emit_local_set(dispatch_var);

        // block $exit (blocktype = i64 for return value)
        self.code.push(OP_BLOCK);
        self.code.push(WASM_BLOCK_I64);

        // loop $dispatch
        self.code.push(OP_LOOP);
        self.code.push(WASM_BLOCK_VOID);

        // Nested blocks for each MIR block (in reverse order for br_table indexing)
        for _ in 0..block_count {
            self.code.push(OP_BLOCK);
            self.code.push(WASM_BLOCK_VOID);
        }

        // br_table dispatch
        self.emit_local_get(dispatch_var);
        self.code.push(OP_I32_WRAP_I64);
        // br_table opcode
        self.code.push(0x0E);
        // vector of targets: br_table [0, 1, 2, ...block_count-1] default=block_count (exit)
        encode_unsigned_leb128(&mut self.code, block_count as u64);
        for i in 0..block_count {
            encode_unsigned_leb128(&mut self.code, i as u64);
        }
        // default target: break out to exit
        encode_unsigned_leb128(&mut self.code, (block_count + 1) as u64);

        // Now emit each block's code, with `end` closing the corresponding nested block.
        // Block 0's code is after the innermost end, block N-1's code is outermost.
        // br_table target 0 goes to the first end, which then falls through to block 0's code.
        for block_idx in (0..block_count).rev() {
            self.code.push(OP_END); // close the block for block_idx

            let block = &self.func.blocks[block_idx];

            // Emit statements
            for stmt in &block.stmts {
                self.emit_stmt(stmt);
            }

            // Emit terminator
            // The depth to $dispatch (loop) is: (block_count - block_idx) levels up
            // The depth to $exit (block) is: (block_count - block_idx + 1) levels up
            let loop_depth = (block_count - block_idx) as u32;
            let exit_depth = loop_depth + 1;

            self.emit_terminator(&block.terminator, dispatch_var, loop_depth, exit_depth);
        }

        // end loop
        self.code.push(OP_END);
        // end block $exit -- the return value is on the stack
        self.code.push(OP_END);
    }

    fn emit_terminator(
        &mut self,
        term: &Terminator,
        dispatch_var: u32,
        loop_depth: u32,
        exit_depth: u32,
    ) {
        match term {
            Terminator::Return { value, .. } => {
                if let Some(val) = value {
                    self.emit_value(val);
                } else {
                    self.emit_nanbox_nil();
                }
                // Break to exit block with return value on stack
                self.code.push(OP_BR);
                encode_unsigned_leb128(&mut self.code, exit_depth as u64);
            }
            Terminator::Jump { target, .. } => {
                self.emit_i64_const(target.0 as i64);
                self.emit_local_set(dispatch_var);
                self.code.push(OP_BR);
                encode_unsigned_leb128(&mut self.code, loop_depth as u64);
            }
            Terminator::Branch {
                cond,
                then_target,
                else_target,
                ..
            } => {
                self.emit_value(cond);
                self.emit_untag_bool();
                // if nonzero -> then, else -> else
                self.code.push(OP_I32_WRAP_I64);
                self.code.push(OP_IF);
                self.code.push(WASM_BLOCK_VOID);
                // then
                self.emit_i64_const(then_target.0 as i64);
                self.emit_local_set(dispatch_var);
                self.code.push(OP_ELSE);
                // else
                self.emit_i64_const(else_target.0 as i64);
                self.emit_local_set(dispatch_var);
                self.code.push(OP_END);
                self.code.push(OP_BR);
                encode_unsigned_leb128(&mut self.code, loop_depth as u64);
            }
            Terminator::Switch {
                scrutinee,
                cases,
                default,
                ..
            } => {
                // For each case, emit a comparison and conditional branch.
                // If matched, set dispatch_var to target and br to loop.
                for (case, target) in cases {
                    match case {
                        SwitchCase::Literal(lit) => {
                            self.emit_value(scrutinee);
                            self.emit_literal(lit);
                            self.code.push(OP_I64_EQ);
                        }
                        SwitchCase::Type(tag) => {
                            self.emit_value(scrutinee);
                            let func_idx = import_type_id(self.imports);
                            self.emit_call(func_idx);
                            self.emit_i64_const(tag.0 as i64);
                            self.code.push(OP_I64_EQ);
                        }
                    }
                    // i64.eq returns i32
                    self.code.push(OP_IF);
                    self.code.push(WASM_BLOCK_VOID);
                    self.emit_i64_const(target.0 as i64);
                    self.emit_local_set(dispatch_var);
                    self.code.push(OP_BR);
                    // +1 because we're inside the if block
                    encode_unsigned_leb128(&mut self.code, loop_depth as u64 + 1);
                    self.code.push(OP_END);
                }
                // Default case
                self.emit_i64_const(default.0 as i64);
                self.emit_local_set(dispatch_var);
                self.code.push(OP_BR);
                encode_unsigned_leb128(&mut self.code, loop_depth as u64);
            }
            Terminator::Unreachable { .. } => {
                self.code.push(OP_UNREACHABLE);
            }
        }
    }
}

// ---- Top-level WASM module emission ----

pub fn emit_wasm_module(mir: &MirModule) -> Result<Vec<u8>, CodegenError> {
    let mut imports = ImportRegistry::new();
    let mut wasm = Vec::new();

    // First pass: collect all function signatures and assign indices.
    // Import functions come first, then module-defined functions.
    // We need two passes because imports must know their type indices.

    // Pre-scan to determine function map (name -> wasm func index).
    // Import indices are assigned dynamically during code generation.
    // So we do a preliminary pass to gather imports, then a real pass.

    // Pass 1: generate all function bodies to collect imports
    let mut func_bodies: Vec<(SmolStr, Vec<u8>, u32, u32)> = Vec::new(); // (name, code, param_count, local_count)

    // Build initial func_map with placeholder indices (will be adjusted after imports are known)
    let mut func_name_order: Vec<SmolStr> = Vec::new();
    let mut func_param_counts: HashMap<SmolStr, u32> = HashMap::new();
    for func in &mir.functions {
        func_name_order.push(func.name.clone());
        func_param_counts.insert(func.name.clone(), func.params.len() as u32);
    }

    // We need to do this in two passes because the import count affects function indices.
    // Pass 1: generate code with temporary func_map (assuming 0 imports)
    // Then fix up indices after we know the import count.

    // Actually, let's do it properly: generate code, collect imports,
    // then regenerate with correct indices.

    // Generate code to discover imports
    {
        let mut temp_func_map: HashMap<SmolStr, (u32, u32)> = HashMap::new();
        for (i, name) in func_name_order.iter().enumerate() {
            let param_count = func_param_counts[name];
            // Temporary index; will be fixed
            temp_func_map.insert(name.clone(), (i as u32, param_count));
        }

        for func in &mir.functions {
            let mut ctx = WasmFuncCtx {
                code: Vec::new(),
                locals: HashMap::new(),
                temps: HashMap::new(),
                next_local: func.params.len() as u32,
                func: func,
                func_map: &temp_func_map,
                imports: &mut imports,
                scratch_local: None,
            };

            // Allocate locals
            for (idx, _local) in func.locals.iter().enumerate() {
                if !func.params.iter().any(|p| p.0 == idx) {
                    let wasm_idx = ctx.alloc_local();
                    ctx.locals.insert(idx, wasm_idx);
                }
            }
            // Map params
            for (param_idx, local_id) in func.params.iter().enumerate() {
                ctx.locals.insert(local_id.0, param_idx as u32);
            }
            // Allocate temps
            for (idx, _temp) in func.temps.iter().enumerate() {
                let wasm_idx = ctx.alloc_local();
                ctx.temps.insert(idx, wasm_idx);
            }

            ctx.emit_blocks();
        }
    }

    // Now we know all imports. Rebuild func_map with correct indices.
    let import_count = imports.imports.len() as u32;
    let mut func_map: HashMap<SmolStr, (u32, u32)> = HashMap::new();
    for (i, name) in func_name_order.iter().enumerate() {
        let param_count = func_param_counts[name];
        func_map.insert(name.clone(), (import_count + i as u32, param_count));
    }

    // Pass 2: regenerate code with correct function indices.
    // Recreate imports fresh - the same import calls will be made in the same order.
    let mut imports = ImportRegistry::new();

    for func in &mir.functions {
        let mut ctx = WasmFuncCtx {
            code: Vec::new(),
            locals: HashMap::new(),
            temps: HashMap::new(),
            next_local: func.params.len() as u32,
            func: func,
            func_map: &func_map,
            imports: &mut imports,
            scratch_local: None,
        };

        // Map params first
        for (param_idx, local_id) in func.params.iter().enumerate() {
            ctx.locals.insert(local_id.0, param_idx as u32);
        }
        // Allocate non-param locals
        for (idx, _local) in func.locals.iter().enumerate() {
            if !func.params.iter().any(|p| p.0 == idx) {
                let wasm_idx = ctx.alloc_local();
                ctx.locals.insert(idx, wasm_idx);
            }
        }
        // Allocate temps
        for (idx, _temp) in func.temps.iter().enumerate() {
            let wasm_idx = ctx.alloc_local();
            ctx.temps.insert(idx, wasm_idx);
        }

        ctx.emit_blocks();

        let extra_locals = ctx.next_local - func.params.len() as u32;
        func_bodies.push((
            func.name.clone(),
            ctx.code,
            func.params.len() as u32,
            extra_locals,
        ));
    }

    // Verify import count matches
    let actual_import_count = imports.imports.len() as u32;
    if actual_import_count != import_count {
        // Re-adjust func_map if import count changed
        let mut func_map: HashMap<SmolStr, (u32, u32)> = HashMap::new();
        for (i, name) in func_name_order.iter().enumerate() {
            let param_count = func_param_counts[name];
            func_map.insert(name.clone(), (actual_import_count + i as u32, param_count));
        }

        // Pass 3: regenerate with final correct indices
        imports = ImportRegistry::new();
        func_bodies.clear();

        for func in &mir.functions {
            let mut ctx = WasmFuncCtx {
                code: Vec::new(),
                locals: HashMap::new(),
                temps: HashMap::new(),
                next_local: func.params.len() as u32,
                func: func,
                func_map: &func_map,
                imports: &mut imports,
                scratch_local: None,
            };

            for (param_idx, local_id) in func.params.iter().enumerate() {
                ctx.locals.insert(local_id.0, param_idx as u32);
            }
            for (idx, _local) in func.locals.iter().enumerate() {
                if !func.params.iter().any(|p| p.0 == idx) {
                    let wasm_idx = ctx.alloc_local();
                    ctx.locals.insert(idx, wasm_idx);
                }
            }
            for (idx, _temp) in func.temps.iter().enumerate() {
                let wasm_idx = ctx.alloc_local();
                ctx.temps.insert(idx, wasm_idx);
            }

            ctx.emit_blocks();

            let extra_locals = ctx.next_local - func.params.len() as u32;
            func_bodies.push((
                func.name.clone(),
                ctx.code,
                func.params.len() as u32,
                extra_locals,
            ));
        }
    }

    // ---- Encode WASM binary ----

    // Magic number + version
    wasm.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D]); // \0asm
    wasm.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // version 1

    // Section 1: Type section
    {
        let mut type_sec = Vec::new();
        // Collect all unique function types:
        // - Import types (from registry)
        // - Module function types

        let mut all_types: Vec<(u32, bool)> = imports.func_types.clone();
        let mut module_type_indices: Vec<u32> = Vec::new();

        for (_, _, param_count, _) in &func_bodies {
            let key = (*param_count, true); // all module functions return i64
            let type_idx = if let Some(pos) = all_types.iter().position(|t| *t == key) {
                pos as u32
            } else {
                let idx = all_types.len() as u32;
                all_types.push(key);
                idx
            };
            module_type_indices.push(type_idx);
        }

        encode_vec_count(&mut type_sec, all_types.len() as u32);
        for (param_count, has_return) in &all_types {
            type_sec.push(WASM_TYPE_FUNC);
            encode_vec_count(&mut type_sec, *param_count);
            for _ in 0..*param_count {
                type_sec.push(WASM_TYPE_I64);
            }
            if *has_return {
                encode_vec_count(&mut type_sec, 1);
                type_sec.push(WASM_TYPE_I64);
            } else {
                encode_vec_count(&mut type_sec, 0);
            }
        }

        encode_section(&mut wasm, 1, &type_sec);

        // Section 2: Import section
        {
            let mut import_sec = Vec::new();
            encode_vec_count(&mut import_sec, imports.imports.len() as u32);
            for (name, type_idx) in &imports.imports {
                encode_string(&mut import_sec, "wrela_runtime");
                encode_string(&mut import_sec, name);
                import_sec.push(0x00); // import kind: func
                encode_unsigned_leb128(&mut import_sec, *type_idx as u64);
            }
            encode_section(&mut wasm, 2, &import_sec);
        }

        // Section 3: Function section (declares types of module functions)
        {
            let mut func_sec = Vec::new();
            encode_vec_count(&mut func_sec, func_bodies.len() as u32);
            for type_idx in &module_type_indices {
                encode_unsigned_leb128(&mut func_sec, *type_idx as u64);
            }
            encode_section(&mut wasm, 3, &func_sec);
        }

        // Section 5: Memory section (1 page minimum for linear memory)
        {
            let mut mem_sec = Vec::new();
            encode_vec_count(&mut mem_sec, 1); // 1 memory
            mem_sec.push(0x00); // limits: no max
            encode_unsigned_leb128(&mut mem_sec, 1); // initial 1 page (64KB)
            encode_section(&mut wasm, 5, &mem_sec);
        }

        // Section 7: Export section
        {
            let mut export_sec = Vec::new();
            let mut export_count = 1u32; // memory

            // Find "main" function to export as "init", and also export "tick" if present
            let mut has_main = false;
            let mut has_tick = false;
            for (name, _, _, _) in func_bodies.iter() {
                if name == "main" {
                    has_main = true;
                    export_count += 1;
                }
                if name == "tick" {
                    has_tick = true;
                    export_count += 1;
                }
            }

            encode_vec_count(&mut export_sec, export_count);

            // Export memory
            encode_string(&mut export_sec, "memory");
            export_sec.push(0x02); // export kind: memory
            encode_unsigned_leb128(&mut export_sec, 0); // memory index 0

            if has_main {
                encode_string(&mut export_sec, "init");
                export_sec.push(0x00); // export kind: func
                let main_idx = func_name_order.iter().position(|n| n == "main").unwrap() as u32;
                encode_unsigned_leb128(
                    &mut export_sec,
                    (imports.imports.len() as u32 + main_idx) as u64,
                );
            }

            if has_tick {
                encode_string(&mut export_sec, "tick");
                export_sec.push(0x00); // export kind: func
                let tick_idx = func_name_order.iter().position(|n| n == "tick").unwrap() as u32;
                encode_unsigned_leb128(
                    &mut export_sec,
                    (imports.imports.len() as u32 + tick_idx) as u64,
                );
            }

            encode_section(&mut wasm, 7, &export_sec);
        }

        // Section 10: Code section
        {
            let mut code_sec = Vec::new();
            encode_vec_count(&mut code_sec, func_bodies.len() as u32);
            for (_, body, _param_count, extra_locals) in &func_bodies {
                let mut func_body = Vec::new();
                // Local declarations: all extra locals are i64
                if *extra_locals > 0 {
                    encode_vec_count(&mut func_body, 1); // 1 local declaration group
                    encode_unsigned_leb128(&mut func_body, *extra_locals as u64);
                    func_body.push(WASM_TYPE_I64);
                } else {
                    encode_vec_count(&mut func_body, 0); // no local declarations
                }
                func_body.extend_from_slice(body);
                func_body.push(OP_END); // end of function

                // Encode function body size + body
                encode_unsigned_leb128(&mut code_sec, func_body.len() as u64);
                code_sec.extend_from_slice(&func_body);
            }
            encode_section(&mut wasm, 10, &code_sec);
        }
    }

    Ok(wasm)
}

/// Compile a MIR module to a WASM binary and write it to the given path.
pub fn compile_to_wasm(mir: &MirModule, output: &Path) -> Result<(), CodegenError> {
    let wasm_bytes = emit_wasm_module(mir)?;
    std::fs::write(output, wasm_bytes)
        .map_err(|err| CodegenError(format!("failed to write WASM output: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::Literal;
    use crate::mir::ir::*;

    fn empty_mir_module() -> MirModule {
        MirModule {
            functions: vec![],
            type_tags: vec![],
            classes: vec![],
        }
    }

    fn simple_return_mir() -> MirModule {
        MirModule {
            functions: vec![MirFunction {
                name: "main".into(),
                params: vec![],
                locals: vec![],
                temps: vec![],
                blocks: vec![BasicBlock {
                    stmts: vec![],
                    terminator: Terminator::Return {
                        value: Some(Value::Const(Literal::Integer(42))),
                        span: rowan::TextRange::empty(0.into()),
                    },
                }],
                entry: BlockId(0),
                suspendable: false,
            }],
            type_tags: vec![],
            classes: vec![],
        }
    }

    fn add_two_integers_mir() -> MirModule {
        MirModule {
            functions: vec![MirFunction {
                name: "main".into(),
                params: vec![],
                locals: vec![Local {
                    name: "x".into(),
                    mutable: false,
                    ty: MirType::Integer,
                }],
                temps: vec![Temp {
                    ty: MirType::Integer,
                }],
                blocks: vec![BasicBlock {
                    stmts: vec![
                        Stmt::Assign {
                            place: Place::Local(LocalId(0)),
                            value: Rvalue::Use(Value::Const(Literal::Integer(10))),
                            span: rowan::TextRange::empty(0.into()),
                        },
                        Stmt::Assign {
                            place: Place::Temp(TempId(0)),
                            value: Rvalue::Binary {
                                op: BinaryOp::Add,
                                lhs: Value::Local(LocalId(0)),
                                rhs: Value::Const(Literal::Integer(32)),
                            },
                            span: rowan::TextRange::empty(0.into()),
                        },
                    ],
                    terminator: Terminator::Return {
                        value: Some(Value::Temp(TempId(0))),
                        span: rowan::TextRange::empty(0.into()),
                    },
                }],
                entry: BlockId(0),
                suspendable: false,
            }],
            type_tags: vec![],
            classes: vec![],
        }
    }

    #[test]
    fn empty_module_produces_valid_wasm_header() {
        let wasm = emit_wasm_module(&empty_mir_module()).unwrap();
        assert_eq!(&wasm[0..4], b"\0asm");
        assert_eq!(&wasm[4..8], &[1, 0, 0, 0]);
    }

    #[test]
    fn simple_return_produces_wasm() {
        let wasm = emit_wasm_module(&simple_return_mir()).unwrap();
        assert_eq!(&wasm[0..4], b"\0asm");
        assert!(wasm.len() > 8);
    }

    #[test]
    fn add_integers_produces_wasm() {
        let wasm = emit_wasm_module(&add_two_integers_mir()).unwrap();
        assert_eq!(&wasm[0..4], b"\0asm");
        assert!(wasm.len() > 8);
    }

    #[test]
    fn leb128_encoding() {
        let mut buf = Vec::new();
        encode_unsigned_leb128(&mut buf, 0);
        assert_eq!(buf, vec![0]);

        buf.clear();
        encode_unsigned_leb128(&mut buf, 127);
        assert_eq!(buf, vec![127]);

        buf.clear();
        encode_unsigned_leb128(&mut buf, 128);
        assert_eq!(buf, vec![0x80, 0x01]);

        buf.clear();
        encode_signed_leb128(&mut buf, 0);
        assert_eq!(buf, vec![0]);

        buf.clear();
        encode_signed_leb128(&mut buf, -1);
        assert_eq!(buf, vec![0x7f]);

        buf.clear();
        encode_signed_leb128(&mut buf, 42);
        assert_eq!(buf, vec![42]);
    }

    #[test]
    fn nanbox_nil_is_qnan_with_imm_nil() {
        let expected = (NANBOX_QNAN | (NANBOX_TAG_IMM << NANBOX_TAG_SHIFT) | NANBOX_IMM_NIL) as i64;
        assert_eq!(nanbox_nil_const(), expected);
    }

    #[test]
    fn nanbox_bool_true_and_false_differ() {
        assert_ne!(nanbox_bool_const(true), nanbox_bool_const(false));
        assert_ne!(nanbox_bool_const(true), nanbox_nil_const());
    }

    #[test]
    fn module_with_branch_produces_wasm() {
        let mir = MirModule {
            functions: vec![MirFunction {
                name: "main".into(),
                params: vec![],
                locals: vec![],
                temps: vec![],
                blocks: vec![
                    BasicBlock {
                        stmts: vec![],
                        terminator: Terminator::Branch {
                            cond: Value::Const(Literal::Boolean(true)),
                            then_target: BlockId(1),
                            else_target: BlockId(2),
                            span: rowan::TextRange::empty(0.into()),
                        },
                    },
                    BasicBlock {
                        stmts: vec![],
                        terminator: Terminator::Return {
                            value: Some(Value::Const(Literal::Integer(1))),
                            span: rowan::TextRange::empty(0.into()),
                        },
                    },
                    BasicBlock {
                        stmts: vec![],
                        terminator: Terminator::Return {
                            value: Some(Value::Const(Literal::Integer(0))),
                            span: rowan::TextRange::empty(0.into()),
                        },
                    },
                ],
                entry: BlockId(0),
                suspendable: false,
            }],
            type_tags: vec![],
            classes: vec![],
        };
        let wasm = emit_wasm_module(&mir).unwrap();
        assert_eq!(&wasm[0..4], b"\0asm");
    }
}
