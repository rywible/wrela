use crate::kernel::ir::KernelModule;
use crate::query_exec::QueryExecContext;

#[derive(Debug, Clone)]
pub struct KernelProgram {
    pub module: KernelModule,
    pub query_exec: QueryExecContext,
}

impl KernelProgram {
    pub fn new(module: KernelModule, query_exec: QueryExecContext) -> Self {
        Self { module, query_exec }
    }

    pub fn module(&self) -> &KernelModule {
        &self.module
    }
}

impl std::ops::Deref for KernelProgram {
    type Target = KernelModule;

    fn deref(&self) -> &Self::Target {
        &self.module
    }
}
