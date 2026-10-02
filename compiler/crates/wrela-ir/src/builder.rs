//! Building a [`Function`] statement by statement, with nested blocks as closures.

use std::mem;

use crate::module::{Block, Expr, Function, Local, Param, ParamMode, Place, Stmt};
use crate::{LocalId, TypeId, ValueId};

/// Builds one function. It doesn't check anything: [`crate::validate`] does, once the function
/// is in a module.
#[derive(Debug)]
pub struct FunctionBuilder {
    name: String,
    params: Vec<Param>,
    ret: TypeId,
    locals: Vec<Local>,
    values: Vec<TypeId>,
    current: Vec<Stmt>,
}

fn next_id(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

impl FunctionBuilder {
    pub fn new(name: impl Into<String>, ret: TypeId) -> Self {
        FunctionBuilder {
            name: name.into(),
            params: Vec::new(),
            ret,
            locals: Vec::new(),
            values: Vec::new(),
            current: Vec::new(),
        }
    }

    /// Adds a parameter and returns the place that reads (or, for `InOut`, writes) it.
    pub fn param(&mut self, name: impl Into<String>, ty: TypeId, mode: ParamMode) -> Place {
        let index = next_id(self.params.len());
        self.params.push(Param {
            name: name.into(),
            ty,
            mode,
        });
        Place::param(index)
    }

    pub fn local(&mut self, name: impl Into<String>, ty: TypeId) -> LocalId {
        let id = LocalId(next_id(self.locals.len()));
        self.locals.push(Local {
            name: name.into(),
            ty,
        });
        id
    }

    /// Defines a value of type `ty` as `expr`, in the current block.
    pub fn value(&mut self, ty: TypeId, expr: Expr) -> ValueId {
        let id = ValueId(next_id(self.values.len()));
        self.values.push(ty);
        self.current.push(Stmt::Let(id, expr));
        id
    }

    pub fn emit(&mut self, stmt: Stmt) {
        self.current.push(stmt);
    }

    pub fn store(&mut self, place: Place, value: ValueId) {
        self.emit(Stmt::Store(place, value));
    }

    pub fn ret(&mut self, value: Option<ValueId>) {
        self.emit(Stmt::Return(value));
    }

    /// Builds a block: whatever `build` emits goes into it rather than the current block.
    pub fn block(&mut self, build: impl FnOnce(&mut Self)) -> Block {
        let outer = mem::take(&mut self.current);
        build(self);
        Block(mem::replace(&mut self.current, outer))
    }

    pub fn if_else(
        &mut self,
        cond: ValueId,
        then: impl FnOnce(&mut Self),
        otherwise: impl FnOnce(&mut Self),
    ) {
        let then = self.block(then);
        let otherwise = self.block(otherwise);
        self.emit(Stmt::If {
            cond,
            then,
            otherwise,
        });
    }

    pub fn loop_with(&mut self, body: impl FnOnce(&mut Self), continuing: impl FnOnce(&mut Self)) {
        let body = self.block(body);
        let continuing = self.block(continuing);
        self.emit(Stmt::Loop { body, continuing });
    }

    pub fn finish(self) -> Function {
        Function {
            name: self.name,
            params: self.params,
            ret: self.ret,
            locals: self.locals,
            values: self.values,
            body: Block(self.current),
        }
    }
}
