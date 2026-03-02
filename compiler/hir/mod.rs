pub mod arena;
pub mod body;
pub mod checkir;
pub mod def;
pub mod lower;
pub mod naming;
pub mod semantic;
pub mod typeck;

pub use arena::*;
pub use body::*;
pub use checkir::*;
pub use def::*;
pub use naming::{NamingError, check_module as check_naming_module};
pub use semantic::*;
pub mod project;
pub mod render_shader_ir;
pub use typeck::{FunctionTypeInfo, Type, TypeError, TypeInfo};
