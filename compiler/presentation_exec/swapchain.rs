//! Swapchain integration surface for presentation framegraphs (RFC 0011 Phase 62.9).
//!
//! # Ownership and safety contract (RFC 0011 M5)
//!
//! [`SwapchainHandle::acquire`] hands the caller an [`AcquiredTexture`] that
//! **must** be returned to the swapchain via [`AcquiredTexture::present`] or
//! explicitly discarded via [`AcquiredTexture::discard`]. Dropping an
//! `AcquiredTexture` without calling either is a programming error: it would
//! leave the swapchain holding an outstanding image and the next `acquire`
//! call would deadlock or panic. The drop guard logs a debug-only assertion in
//! that case so misuse fails loudly in tests.
//!
//! Hosts share the swapchain through `DynSwapchainHandle` (`Arc`-wrapped) so
//! the handle is cheap to clone, but only **one** caller may have an
//! outstanding `AcquiredTexture` at a time. Callers that need to stage work
//! across multiple frames must serialise calls externally — typically through
//! the engine's frame-in-flight semaphore.

use std::sync::Arc;
use thiserror::Error;
use wgpu::SurfaceTexture;

/// Opaque swapchain control for `PresentationFramegraph`.
pub trait SwapchainHandle: Send + Sync {
    /// Block-acquire the next image. Implementations should provide back-
    /// pressure compatible with the host's frame-in-flight policy. The
    /// returned texture is owned by the caller until [`AcquiredTexture::present`]
    /// or [`AcquiredTexture::discard`] is called.
    fn acquire(&self) -> Result<AcquiredTexture, SwapchainError>;

    /// Internal: hand the texture back to the implementation for present.
    /// Callers should always go through [`AcquiredTexture::present`] instead
    /// of calling this directly so the drop guard stays accurate.
    fn submit_present(&self, texture: SurfaceTexture) -> Result<(), SwapchainError>;

    fn current_format(&self) -> wgpu::TextureFormat;

    fn current_extent(&self) -> wgpu::Extent3d;
}

#[derive(Debug, Error)]
pub enum SwapchainError {
    #[error("swapchain acquire failed: {0}")]
    Acquire(String),
    #[error("swapchain present failed: {0}")]
    Present(String),
}

pub type DynSwapchainHandle = Arc<dyn SwapchainHandle>;

/// Owned acquired image. The caller MUST call either
/// [`AcquiredTexture::present`] or [`AcquiredTexture::discard`] before this
/// value is dropped.
///
/// In debug builds dropping without doing either fires a `debug_assert!` so
/// the host fails loudly during testing. In release builds the panic is
/// elided so a buggy host doesn't take down end users — instead a frame is
/// silently lost, which is the same behaviour as a missed `present`.
pub struct AcquiredTexture {
    inner: Option<AcquiredInner>,
}

struct AcquiredInner {
    texture: SurfaceTexture,
    swapchain: Arc<dyn SwapchainHandle>,
}

impl std::fmt::Debug for AcquiredTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcquiredTexture")
            .field("presented_or_discarded", &self.inner.is_none())
            .finish()
    }
}

impl AcquiredTexture {
    /// Construct from a raw acquire result. Implementations of
    /// [`SwapchainHandle::acquire`] should be the only callers.
    pub fn new(texture: SurfaceTexture, swapchain: Arc<dyn SwapchainHandle>) -> Self {
        Self {
            inner: Some(AcquiredInner { texture, swapchain }),
        }
    }

    /// Borrow the underlying surface texture for use by the framegraph.
    pub fn texture(&self) -> &SurfaceTexture {
        &self
            .inner
            .as_ref()
            .expect("texture already consumed")
            .texture
    }

    /// Submit this texture for presentation. Always succeeds-or-errors; either
    /// way the texture is no longer owned by the caller.
    pub fn present(mut self) -> Result<(), SwapchainError> {
        let inner = self.inner.take().expect("texture already consumed");
        inner.swapchain.submit_present(inner.texture)
    }

    /// Drop the texture without presenting it. Use when an upstream error
    /// means the frame should not be shown but the swapchain still needs to
    /// advance its image queue.
    pub fn discard(mut self) {
        // Dropping the inner SurfaceTexture itself releases the image.
        let _ = self.inner.take();
    }
}

impl Drop for AcquiredTexture {
    fn drop(&mut self) {
        if self.inner.is_some() {
            debug_assert!(
                false,
                "AcquiredTexture dropped without present()/discard(); the swapchain will not advance"
            );
        }
    }
}
