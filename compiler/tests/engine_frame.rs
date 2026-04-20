use std::sync::{Arc, Mutex};
use wrela::engine_frame::{
    EngineFrameContext, EngineFrameError, EngineFrameReport, EngineFrameScheduler,
    EngineFutureReserveReport, EngineSubsystemDescriptor, EngineSubsystemKind,
    EngineSubsystemReport, EngineSubsystemWork,
};
use wrela::gpu_runtime::GpuRuntimeMetrics;

#[test]
fn engine_frame_report_round_trips_through_json() {
    let report = EngineFrameReport {
        scenario_id: "closure_fixture".to_string(),
        frame_index: 3,
        frame_wall_time_micros: 8_100,
        cpu_critical_path_micros: 5_400,
        gpu_critical_path_micros: Some(4_700),
        present_wait_micros: 120,
        gpu_wait_micros: 90,
        readback_wait_micros: 0,
        steady_state_fps: 123.4,
        gpu_runtime: GpuRuntimeMetrics {
            queue_submit_count: 1,
            readback_bytes: 0,
            upload_bytes: 128,
            ..GpuRuntimeMetrics::default()
        },
        subsystems: vec![
            EngineSubsystemReport {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".into(),
                work_items: 7,
                cpu_critical_path_micros: 4_000,
                gpu_critical_path_micros: Some(3_900),
                queue_submit_count: 1,
                hot_path_readback_bytes: 0,
                scene_reupload_bytes: 0,
                wait_time_micros: 80,
                notes: vec!["framegraph".to_string()],
            },
            EngineSubsystemReport {
                kind: EngineSubsystemKind::Collision,
                label: "collision".into(),
                work_items: 7200,
                cpu_critical_path_micros: 800,
                gpu_critical_path_micros: Some(600),
                queue_submit_count: 0,
                hot_path_readback_bytes: 0,
                scene_reupload_bytes: 0,
                wait_time_micros: 10,
                notes: vec!["compatibility-assembled".to_string()],
            },
        ],
        future_subsystem_reserve: EngineFutureReserveReport {
            reserved_micros: 1_000,
            remaining_micros: 230,
            exhausted: false,
        },
        active_degradations: vec!["enable_hit_compaction".to_string()],
        violations: Vec::new(),
    };

    let json = serde_json::to_string(&report).expect("serialize engine frame report");
    let round_trip: EngineFrameReport =
        serde_json::from_str(&json).expect("deserialize engine frame report");
    assert_eq!(round_trip, report);
}

#[test]
fn engine_frame_scheduler_runs_empty_frame() {
    let mut scheduler = EngineFrameScheduler::default();
    let mut subsystems: Vec<Box<dyn EngineSubsystemWork>> = Vec::new();
    let report = scheduler
        .run_frame("empty_fixture", 0, &mut subsystems)
        .expect("empty frame report");

    assert_eq!(report.scenario_id, "empty_fixture");
    assert_eq!(report.frame_index, 0);
    assert!(report.subsystems.is_empty());
    assert_eq!(report.future_subsystem_reserve.reserved_micros, 0);
}

#[test]
fn engine_frame_scheduler_orders_fake_subsystems_by_dependency() {
    #[derive(Clone)]
    struct FakeSubsystem {
        descriptor: EngineSubsystemDescriptor,
        log: Arc<Mutex<Vec<String>>>,
        cpu_micros: u128,
    }

    impl EngineSubsystemWork for FakeSubsystem {
        fn descriptor(&self) -> EngineSubsystemDescriptor {
            self.descriptor.clone()
        }

        fn prepare(&mut self, _ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> {
            self.log
                .lock()
                .expect("log")
                .push(format!("{}:prepare", self.descriptor.label));
            Ok(())
        }

        fn encode(&mut self, _ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> {
            self.log
                .lock()
                .expect("log")
                .push(format!("{}:encode", self.descriptor.label));
            Ok(())
        }

        fn finish(
            &mut self,
            _ctx: &mut EngineFrameContext,
        ) -> Result<EngineSubsystemReport, EngineFrameError> {
            self.log
                .lock()
                .expect("log")
                .push(format!("{}:finish", self.descriptor.label));
            Ok(EngineSubsystemReport {
                kind: self.descriptor.kind.clone(),
                label: self.descriptor.label.clone(),
                work_items: 1,
                cpu_critical_path_micros: self.cpu_micros,
                gpu_critical_path_micros: None,
                queue_submit_count: 0,
                hot_path_readback_bytes: 0,
                scene_reupload_bytes: 0,
                wait_time_micros: 0,
                notes: Vec::new(),
            })
        }
    }

    let log = Arc::new(Mutex::new(Vec::new()));
    let presentation = FakeSubsystem {
        descriptor: EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Presentation,
            label: "presentation".to_string(),
            runs_after: vec![],
            requires_gpu: false,
            allows_hot_path_readback: false,
        },
        log: Arc::clone(&log),
        cpu_micros: 10,
    };
    let collision = FakeSubsystem {
        descriptor: EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Collision,
            label: "collision".to_string(),
            runs_after: vec![EngineSubsystemKind::Presentation],
            requires_gpu: false,
            allows_hot_path_readback: false,
        },
        log: Arc::clone(&log),
        cpu_micros: 20,
    };

    let mut scheduler = EngineFrameScheduler::default();
    let mut subsystems: Vec<Box<dyn EngineSubsystemWork>> =
        vec![Box::new(collision), Box::new(presentation)];
    let report = scheduler
        .run_frame("dependency_fixture", 1, &mut subsystems)
        .expect("dependency-ordered frame");

    assert_eq!(report.subsystems.len(), 2);
    assert_eq!(report.subsystems[0].label, "presentation");
    assert_eq!(report.subsystems[1].label, "collision");
    assert_eq!(
        log.lock().expect("log").as_slice(),
        [
            "presentation:prepare",
            "presentation:encode",
            "presentation:finish",
            "collision:prepare",
            "collision:encode",
            "collision:finish",
        ]
    );
}

#[test]
fn engine_frame_scheduler_counts_contiguous_gpu_subsystems_separately_without_submit_proof() {
    #[derive(Clone)]
    struct FakeSubsystem {
        descriptor: EngineSubsystemDescriptor,
        queue_submit_count: u32,
    }

    impl EngineSubsystemWork for FakeSubsystem {
        fn descriptor(&self) -> EngineSubsystemDescriptor {
            self.descriptor.clone()
        }

        fn prepare(&mut self, _ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> {
            Ok(())
        }

        fn encode(&mut self, _ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> {
            Ok(())
        }

        fn finish(
            &mut self,
            _ctx: &mut EngineFrameContext,
        ) -> Result<EngineSubsystemReport, EngineFrameError> {
            Ok(EngineSubsystemReport {
                kind: self.descriptor.kind.clone(),
                label: self.descriptor.label.clone(),
                work_items: 1,
                cpu_critical_path_micros: 500,
                gpu_critical_path_micros: Some(250),
                queue_submit_count: self.queue_submit_count,
                hot_path_readback_bytes: 0,
                scene_reupload_bytes: 0,
                wait_time_micros: 0,
                notes: Vec::new(),
            })
        }
    }

    let mut scheduler = EngineFrameScheduler::default();
    let mut subsystems: Vec<Box<dyn EngineSubsystemWork>> = vec![
        Box::new(FakeSubsystem {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".to_string(),
                runs_after: vec![],
                requires_gpu: true,
                allows_hot_path_readback: false,
            },
            queue_submit_count: 1,
        }),
        Box::new(FakeSubsystem {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Collision,
                label: "collision".to_string(),
                runs_after: vec![EngineSubsystemKind::Presentation],
                requires_gpu: true,
                allows_hot_path_readback: false,
            },
            queue_submit_count: 1,
        }),
    ];

    let report = scheduler
        .run_frame("fusion_fixture", 0, &mut subsystems)
        .expect("fused frame report");

    assert_eq!(report.gpu_runtime.queue_submit_count, 2);
}

#[test]
fn engine_frame_scheduler_records_budget_violations_and_reserve_accounting() {
    #[derive(Clone)]
    struct FakeSubsystem {
        descriptor: EngineSubsystemDescriptor,
        cpu_micros: u128,
        gpu_micros: Option<u128>,
        queue_submit_count: u32,
        hot_path_readback_bytes: u64,
        degradations: Vec<String>,
    }

    impl EngineSubsystemWork for FakeSubsystem {
        fn descriptor(&self) -> EngineSubsystemDescriptor {
            self.descriptor.clone()
        }

        fn prepare(&mut self, _ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> {
            Ok(())
        }

        fn encode(&mut self, _ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> {
            Ok(())
        }

        fn finish(
            &mut self,
            ctx: &mut EngineFrameContext,
        ) -> Result<EngineSubsystemReport, EngineFrameError> {
            for degradation in &self.degradations {
                if !ctx.active_degradations.contains(degradation) {
                    ctx.active_degradations.push(degradation.clone());
                }
            }
            Ok(EngineSubsystemReport {
                kind: self.descriptor.kind.clone(),
                label: self.descriptor.label.clone(),
                work_items: 1,
                cpu_critical_path_micros: self.cpu_micros,
                gpu_critical_path_micros: self.gpu_micros,
                queue_submit_count: self.queue_submit_count,
                hot_path_readback_bytes: self.hot_path_readback_bytes,
                scene_reupload_bytes: 0,
                wait_time_micros: 0,
                notes: Vec::new(),
            })
        }
    }

    let mut scheduler = EngineFrameScheduler {
        budget: Some(
            wrela::perf_target::PerfClosureProfile::canonical_1080p120().engine_frame_budget,
        ),
    };
    let mut subsystems: Vec<Box<dyn EngineSubsystemWork>> = vec![
        Box::new(FakeSubsystem {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::StateAdvance,
                label: "state_advance".to_string(),
                runs_after: vec![],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            cpu_micros: 500,
            gpu_micros: None,
            queue_submit_count: 0,
            hot_path_readback_bytes: 0,
            degradations: vec![],
        }),
        Box::new(FakeSubsystem {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: true,
                allows_hot_path_readback: false,
            },
            cpu_micros: 7_000,
            gpu_micros: Some(6_000),
            queue_submit_count: 1,
            hot_path_readback_bytes: 0,
            degradations: vec!["enable_hit_compaction".to_string()],
        }),
        Box::new(FakeSubsystem {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Collision,
                label: "collision".to_string(),
                runs_after: vec![EngineSubsystemKind::Presentation],
                requires_gpu: true,
                allows_hot_path_readback: false,
            },
            cpu_micros: 1_500,
            gpu_micros: Some(1_000),
            queue_submit_count: 2,
            hot_path_readback_bytes: 16,
            degradations: vec![],
        }),
    ];

    let report = scheduler
        .run_frame("budget_fixture", 0, &mut subsystems)
        .expect("budgeted frame report");

    assert_eq!(report.future_subsystem_reserve.reserved_micros, 1_000);
    assert!(report.future_subsystem_reserve.exhausted);
    assert!(report.future_subsystem_reserve.remaining_micros < 0);
    assert!(
        report
            .active_degradations
            .contains(&"enable_hit_compaction".to_string())
    );
    assert!(
        report
            .violations
            .contains(&"engine_frame_hot_path_readback_budget_exceeded".to_string())
    );
    assert!(
        report
            .violations
            .contains(&"engine_frame_queue_submit_budget_exceeded".to_string())
    );
    assert!(
        report
            .violations
            .contains(&"engine_frame_future_reserve_exhausted".to_string())
    );
}
