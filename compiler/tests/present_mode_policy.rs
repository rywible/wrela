use wrela::engine_frame::{PresentModePolicy, ResolvedPresentMode};

#[test]
fn present_mode_prefers_mailbox_then_vrr_then_fifo() {
    let policy = PresentModePolicy::PreferMailboxThenVrrFifoThenFifo;

    let mailbox = policy.select(true, true);
    assert_eq!(mailbox.mode, ResolvedPresentMode::Mailbox);
    assert!(mailbox.findings.is_empty());

    let vrr = policy.select(false, true);
    assert_eq!(vrr.mode, ResolvedPresentMode::FifoRelaxed);
    assert!(vrr.findings.is_empty());

    let fifo = policy.select(false, false);
    assert_eq!(fifo.mode, ResolvedPresentMode::Fifo);
    assert_eq!(fifo.findings, ["presentation.fallback_to_vsync_fifo"]);
}

#[test]
fn explicit_fifo_is_not_a_fallback_finding() {
    let selected = PresentModePolicy::Fifo.select(false, false);
    assert_eq!(selected.mode, ResolvedPresentMode::Fifo);
    assert!(selected.findings.is_empty());
}
