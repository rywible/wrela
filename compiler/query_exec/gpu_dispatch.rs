use crate::gpu_runtime::{GpuPassProfiler, GpuRuntimeMetrics};
use crate::kernel::KernelValue;
use crate::kernel::ir::KernelBatchQueryPlan;
use crate::portable::PortableAbiType;
use crate::query_exec::cpu::QueryExecError;
use crate::query_exec::wgsl::{
    GpuDispatchRequest, NativeWgpuContext, ResidentBatchQuerySession,
    build_batch_request_for_shader_with_snapshot,
    build_batch_request_without_items_for_shader_with_snapshot, compile_batch_shader, encode_slice,
    normalized_dispatch_config, prepare_resident_batch_query,
};
use crate::query_exec::{QueryExecContext, QueryExecutionObservability};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct GpuQueryBufferHandle {
    pub buffer: wgpu::Buffer,
    pub size_bytes: u64,
    pub abi: Option<PortableAbiType>,
}

#[derive(Clone)]
pub(crate) struct GpuDispatchResult {
    pub values: GpuQueryBufferHandle,
    pub metrics: Option<GpuQueryBufferHandle>,
    pub item_count: u32,
}

#[derive(Clone)]
pub(crate) struct GpuQueryDispatcher {
    request: GpuDispatchRequest,
    session: ResidentBatchQuerySession,
    input_bytes: Option<Vec<u8>>,
    side_channel_bytes: Option<Vec<u8>>,
}

impl GpuQueryDispatcher {
    pub(crate) fn from_batch_plan(
        ctx: &QueryExecContext,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
    ) -> Result<Self, QueryExecError> {
        Self::from_batch_plan_with_candidate_spans(ctx, plan, args, Vec::new())
    }

    pub(crate) fn from_batch_plan_with_candidate_spans(
        ctx: &QueryExecContext,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
        candidate_spans: Vec<u32>,
    ) -> Result<Self, QueryExecError> {
        Self::from_batch_plan_with_candidate_spans_and_snapshot(
            ctx,
            None,
            plan,
            args,
            candidate_spans,
        )
    }

    pub(crate) fn from_batch_plan_with_candidate_spans_and_snapshot(
        ctx: &QueryExecContext,
        snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
        candidate_spans: Vec<u32>,
    ) -> Result<Self, QueryExecError> {
        let generated = compile_batch_shader(ctx, plan)?;
        let mut request = build_batch_request_for_shader_with_snapshot(ctx, snapshot, plan, args)?;
        request.candidate_spans = candidate_spans;
        let item_abi = generated.item_abi.clone();
        Self::from_request(item_abi, request, generated)
    }

    pub(crate) fn from_batch_plan_without_items(
        ctx: &QueryExecContext,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
        item_count: u32,
    ) -> Result<Self, QueryExecError> {
        Self::from_batch_plan_without_items_and_snapshot(ctx, None, plan, args, item_count)
    }

    pub(crate) fn from_batch_plan_without_items_and_snapshot(
        ctx: &QueryExecContext,
        snapshot: Option<&crate::world_identity::WorldSnapshotHandle>,
        plan: &KernelBatchQueryPlan,
        args: &[KernelValue],
        item_count: u32,
    ) -> Result<Self, QueryExecError> {
        let generated = compile_batch_shader(ctx, plan)?;
        let request = build_batch_request_without_items_for_shader_with_snapshot(
            ctx, snapshot, plan, args, item_count,
        )?;
        let item_abi = generated.item_abi.clone();
        Self::from_request(item_abi, request, generated)
    }

    fn from_request(
        item_abi: PortableAbiType,
        mut request: GpuDispatchRequest,
        generated: crate::query_exec::wgsl::GeneratedShaderModule,
    ) -> Result<Self, QueryExecError> {
        let input_bytes = (!request.items.is_empty())
            .then(|| encode_slice(&item_abi, &request.items))
            .transpose()?;
        let side_channel_bytes = if !request.candidate_spans.is_empty() {
            let values = request
                .candidate_spans
                .iter()
                .copied()
                .map(KernelValue::U32)
                .collect::<Vec<_>>();
            Some(encode_slice(&PortableAbiType::U32, &values)?)
        } else if !request.continuation_seeds.is_empty() {
            let values = request
                .continuation_seeds
                .iter()
                .copied()
                .map(KernelValue::U32)
                .collect::<Vec<_>>();
            Some(encode_slice(&PortableAbiType::U32, &values)?)
        } else {
            None
        };
        request.dispatch = normalized_dispatch_config(&request)?;
        let session = prepare_resident_batch_query(&generated, &request)?;
        Ok(Self {
            request,
            session,
            input_bytes,
            side_channel_bytes,
        })
    }

    pub(crate) fn native(&self) -> &Arc<NativeWgpuContext> {
        &self.session.native
    }

    pub(crate) fn input_buffer(&self) -> GpuQueryBufferHandle {
        GpuQueryBufferHandle {
            buffer: self.session.input_buffer.clone(),
            size_bytes: self.session.input_buffer_size,
            abi: None,
        }
    }

    pub(crate) fn dispatch_result(&self) -> GpuDispatchResult {
        GpuDispatchResult {
            values: GpuQueryBufferHandle {
                buffer: self.session.output_buffer.clone(),
                size_bytes: self.session.output_buffer_size,
                abi: Some(self.session.result_abi.clone()),
            },
            metrics: Some(GpuQueryBufferHandle {
                buffer: self.session.observability_buffer.clone(),
                size_bytes: self.session.observability_buffer_size,
                abi: None,
            }),
            item_count: self.session.item_count,
        }
    }

    pub(crate) fn item_count(&self) -> u32 {
        self.session.item_count
    }

    pub(crate) fn selected_workgroup_size(&self) -> u32 {
        self.session.selected_workgroup_size()
    }

    pub(crate) fn initial_gpu_runtime(&self) -> GpuRuntimeMetrics {
        self.session.initial_gpu_runtime()
    }

    pub(crate) fn initialize_dispatch_state(&self) -> Result<u64, QueryExecError> {
        self.session.initialize_dispatch_state_with_inputs(
            &self.request.dispatch,
            self.input_bytes.as_deref(),
            self.side_channel_bytes.as_deref(),
        )
    }

    pub(crate) fn encode_compute_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        profiler: &mut GpuPassProfiler,
    ) {
        self.session.encode_compute_pass(encoder, profiler);
    }

    pub(crate) fn decode_observability(
        &self,
        bytes: &[u8],
        gpu_runtime: GpuRuntimeMetrics,
    ) -> QueryExecutionObservability {
        self.session.decode_observability(bytes, gpu_runtime)
    }
}
