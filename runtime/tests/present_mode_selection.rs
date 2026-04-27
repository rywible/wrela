use wgpu::PresentMode;
use wrela_runtime::platform::surface::{
    PresentModeFallbackReason, RawPresentModePolicy, select_wgpu_present_mode,
};

#[test]
fn raw_present_mode_selection_matches_latency_policy() {
    let selection = select_wgpu_present_mode(
        RawPresentModePolicy::PreferMailboxThenVrrFifoThenFifo,
        &[PresentMode::Fifo, PresentMode::Mailbox],
    );
    assert_eq!(selection.mode, PresentMode::Mailbox);
    assert_eq!(selection.fallback_reason, None);
    assert_eq!(selection.finding_code, None);

    let selection = select_wgpu_present_mode(
        RawPresentModePolicy::PreferMailboxThenVrrFifoThenFifo,
        &[PresentMode::Fifo, PresentMode::FifoRelaxed],
    );
    assert_eq!(selection.mode, PresentMode::FifoRelaxed);
    assert_eq!(selection.fallback_reason, None);
    assert_eq!(selection.finding_code, None);

    let selection =
        select_wgpu_present_mode(RawPresentModePolicy::PreferMailboxThenVrrFifoThenFifo, &[]);
    assert_eq!(selection.mode, PresentMode::Fifo);
    assert_eq!(
        selection.fallback_reason,
        Some(PresentModeFallbackReason::MailboxAndVrrUnavailable)
    );
    assert_eq!(
        selection.finding_code,
        Some("presentation.fallback_to_vsync_fifo")
    );
}

#[test]
fn raw_mailbox_policy_reports_typed_vsync_fallbacks() {
    let selection = select_wgpu_present_mode(
        RawPresentModePolicy::Mailbox,
        &[PresentMode::FifoRelaxed, PresentMode::Fifo],
    );
    assert_eq!(selection.mode, PresentMode::FifoRelaxed);
    assert_eq!(
        selection.fallback_reason,
        Some(PresentModeFallbackReason::MailboxUnavailable)
    );
    assert_eq!(
        selection.finding_code,
        Some("presentation.fallback_to_vsync_fifo")
    );
}
