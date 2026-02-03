use crate::hir::arena::{Arena, Idx};
use crate::hir::body::{Body, Literal, UseName};
use rowan::TextRange;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub functions: Arena<Function>,
    pub classes: Arena<Class>,
    pub enums: Arena<Enum>,
    pub interfaces: Arena<Interface>,
    pub uses: Vec<UseStmt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Function,
    Method,
    Derived,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub kind: FunctionKind,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeRef>,
    pub body: Option<Body>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub type_params: Vec<SmolStr>,
    pub fields: Vec<Field>,
    pub methods: Vec<Idx<Function>>,
    pub implements: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub type_params: Vec<SmolStr>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Interface {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub type_params: Vec<SmolStr>,
    pub methods: Vec<InterfaceMethod>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceMethod {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseStmt {
    pub names: Vec<UseName>,
    pub module: SmolStr,
    pub module_span: Option<TextRange>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub ty: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub visibility: Visibility,
    pub ty: Option<TypeRef>,
    pub mutable: bool,
    pub default: Option<FieldDefault>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldDefault {
    Literal(Literal),
    List(Vec<FieldDefault>),
    Map(Vec<(FieldDefault, FieldDefault)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub name: SmolStr,
    pub name_span: Option<TextRange>,
    pub args: Vec<TypeRef>,
}
