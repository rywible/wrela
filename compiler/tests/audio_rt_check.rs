use wrela::audio_exec::rt_check::{AudioRtErrorKind, check_audio_rt_module};
use wrela::hir::lower::lower;
use wrela::parser::ast::{AstNode, Root};
use wrela::parser::parse;

fn check(input: &str) -> Vec<AudioRtErrorKind> {
    let root = Root::cast(parse(input)).expect("root");
    let module = lower(root);
    check_audio_rt_module(&module)
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

#[test]
fn audio_rt_rejects_allocation() {
    let errors = check(
        r#"
@audio_rt audio field Gain(sample: F32) -> F32 {
    scratch = [sample]
    return sample
}
"#,
    );
    assert!(errors.contains(&AudioRtErrorKind::Allocation));
}

#[test]
fn audio_rt_rejects_unbounded_loops_and_blocking_calls() {
    let errors = check(
        r#"
@audio_rt media field Occlusion(sample: F32) -> F32 {
    while sample > 0.0 {
        wait_for_io()
    }
    return sample
}
"#,
    );
    assert!(errors.contains(&AudioRtErrorKind::UnboundedLoop));
    assert!(errors.contains(&AudioRtErrorKind::BlockingEffect));
}

#[test]
fn audio_rt_rejects_non_audio_rt_callees() {
    let errors = check(
        r#"
fn helper(sample: F32) -> F32 {
    return sample
}

@audio_rt audio field Gain(sample: F32) -> F32 {
    return helper(sample)
}
"#,
    );
    assert!(errors.contains(&AudioRtErrorKind::NonAudioRtCallee));
}

#[test]
fn audio_rt_rejects_unbounded_result_propagation() {
    let errors = check(
        r#"
@audio_rt audio field Gain(sample: F32) -> Result[F32] {
    return try sample
}
"#,
    );
    assert!(errors.contains(&AudioRtErrorKind::UnboundedResultPropagation));
}

#[test]
fn audio_rt_accepts_bounded_numeric_audio_fields() {
    let errors = check(
        r#"
@audio_rt audio field Gain(sample: F32) -> F32 {
    return sample * 0.5
}
"#,
    );
    assert!(errors.is_empty(), "unexpected audio_rt errors: {errors:?}");
}
