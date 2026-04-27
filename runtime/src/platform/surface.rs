//! Raw wgpu surface helpers (RFC 0011 Phase 64).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawPresentModePolicy {
    PreferMailboxThenVrrFifoThenFifo,
    Fifo,
    Mailbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentModeFallbackReason {
    MailboxUnavailable,
    MailboxAndVrrUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawPresentModeSelection {
    pub mode: wgpu::PresentMode,
    pub fallback_reason: Option<PresentModeFallbackReason>,
    pub finding_code: Option<&'static str>,
}

pub fn select_wgpu_present_mode(
    policy: RawPresentModePolicy,
    supported: &[wgpu::PresentMode],
) -> RawPresentModeSelection {
    let has_mailbox = supported.contains(&wgpu::PresentMode::Mailbox);
    let has_fifo_relaxed = supported.contains(&wgpu::PresentMode::FifoRelaxed);
    let selected = |mode, fallback_reason| RawPresentModeSelection {
        mode,
        fallback_reason,
        finding_code: fallback_reason.map(|_| "presentation.fallback_to_vsync_fifo"),
    };
    match policy {
        RawPresentModePolicy::Mailbox if has_mailbox => selected(wgpu::PresentMode::Mailbox, None),
        RawPresentModePolicy::Mailbox if has_fifo_relaxed => selected(
            wgpu::PresentMode::FifoRelaxed,
            Some(PresentModeFallbackReason::MailboxUnavailable),
        ),
        RawPresentModePolicy::Mailbox => selected(
            wgpu::PresentMode::Fifo,
            Some(PresentModeFallbackReason::MailboxUnavailable),
        ),
        RawPresentModePolicy::Fifo => selected(wgpu::PresentMode::Fifo, None),
        RawPresentModePolicy::PreferMailboxThenVrrFifoThenFifo if has_mailbox => {
            selected(wgpu::PresentMode::Mailbox, None)
        }
        RawPresentModePolicy::PreferMailboxThenVrrFifoThenFifo if has_fifo_relaxed => {
            selected(wgpu::PresentMode::FifoRelaxed, None)
        }
        RawPresentModePolicy::PreferMailboxThenVrrFifoThenFifo => selected(
            wgpu::PresentMode::Fifo,
            Some(PresentModeFallbackReason::MailboxAndVrrUnavailable),
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceExtent {
    pub width: u32,
    pub height: u32,
}
