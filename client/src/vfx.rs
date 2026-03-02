pub struct ScreenEffects {
    pub screen_shake_intensity: f32,
    pub screen_shake_decay: f32,
    pub hitstop_frames: u32,
    pub chromatic_aberration: f32,
    shake_offset: [f32; 2],
    shake_seed: u32,
}

fn pseudo_random(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
    (*seed >> 16) as f32 / 65536.0
}

impl ScreenEffects {
    pub fn new() -> Self {
        Self {
            screen_shake_intensity: 0.0,
            screen_shake_decay: 0.0,
            hitstop_frames: 0,
            chromatic_aberration: 0.0,
            shake_offset: [0.0, 0.0],
            shake_seed: 0xCAFE_BABE,
        }
    }

    pub fn trigger_screen_shake(&mut self, intensity: f32, decay: f32) {
        self.screen_shake_intensity = intensity;
        self.screen_shake_decay = decay;
    }

    pub fn trigger_hitstop(&mut self, frames: u32) {
        self.hitstop_frames = frames;
    }

    pub fn trigger_chromatic_aberration(&mut self, intensity: f32) {
        self.chromatic_aberration = intensity;
    }

    pub fn update(&mut self, dt: f32) -> bool {
        if self.screen_shake_intensity > 1e-4 {
            let rx = pseudo_random(&mut self.shake_seed) * 2.0 - 1.0;
            let ry = pseudo_random(&mut self.shake_seed) * 2.0 - 1.0;
            self.shake_offset = [
                rx * self.screen_shake_intensity,
                ry * self.screen_shake_intensity,
            ];
            self.screen_shake_intensity *= (-self.screen_shake_decay * dt).exp();
            if self.screen_shake_intensity < 1e-4 {
                self.screen_shake_intensity = 0.0;
                self.shake_offset = [0.0, 0.0];
            }
        } else {
            self.screen_shake_intensity = 0.0;
            self.shake_offset = [0.0, 0.0];
        }

        if self.hitstop_frames > 0 {
            self.hitstop_frames -= 1;
        }

        if self.chromatic_aberration > 1e-4 {
            self.chromatic_aberration *= (-5.0 * dt).exp();
            if self.chromatic_aberration < 1e-4 {
                self.chromatic_aberration = 0.0;
            }
        } else {
            self.chromatic_aberration = 0.0;
        }

        self.screen_shake_intensity > 0.0
            || self.hitstop_frames > 0
            || self.chromatic_aberration > 0.0
    }

    pub fn camera_offset(&self) -> [f32; 2] {
        self.shake_offset
    }

    pub fn should_freeze_animation(&self) -> bool {
        self.hitstop_frames > 0
    }
}

impl Default for ScreenEffects {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Resonance Visual Escalation
// ---------------------------------------------------------------------------

/// Render parameters driven by the resonance tier. The renderer reads these
/// each frame and applies them to the post-process pipeline.
#[derive(Clone, Debug)]
pub struct ResonanceRenderParams {
    /// Negative offset applied to the bloom brightness threshold.
    /// A more negative value means more of the scene blooms.
    pub bloom_threshold_offset: f32,
    /// Additive RGB tint applied to the bloom pass output.
    pub bloom_color_tint: [f32; 3],
    /// Chromatic aberration strength (0 = none, 1 = maximum).
    pub chromatic_aberration: f32,
    /// Color grading intensity toward cooler tones (0 = none, 1 = full shift).
    pub color_grade_intensity: f32,
    /// Multiplier on particle emission rates (1.0 = baseline).
    pub particle_intensity_multiplier: f32,
    /// Vignette overlay intensity (0 = none, 1 = full).
    pub vignette_intensity: f32,
    /// Motion blur hint strength (0 = none, 1 = maximum).
    pub motion_blur_intensity: f32,
    /// Desaturation on hit (0 = none, 1 = fully desaturated). Caller triggers
    /// this via [`ResonanceVfx::trigger_hit_desaturation`] and it decays over time.
    pub hit_desaturation: f32,
}

impl Default for ResonanceRenderParams {
    fn default() -> Self {
        Self {
            bloom_threshold_offset: 0.0,
            bloom_color_tint: [0.0, 0.0, 0.0],
            chromatic_aberration: 0.0,
            color_grade_intensity: 0.0,
            particle_intensity_multiplier: 1.0,
            vignette_intensity: 0.0,
            motion_blur_intensity: 0.0,
            hit_desaturation: 0.0,
        }
    }
}

/// Drives visual intensity based on the current resonance tier (0-4).
///
/// The `visual_tier` field smoothly interpolates toward the discrete
/// `current_tier` so that all derived render parameters transition without
/// popping. Call [`update`] once per frame, then read the output of
/// [`render_params`] to feed into the post-process pipeline.
pub struct ResonanceVfx {
    /// The discrete tier reported by the game logic (0-4).
    current_tier: i32,
    /// Smoothly interpolated value that chases `current_tier`.
    visual_tier: f32,
    /// Interpolation speed (units per second). Higher = snappier transitions.
    transition_speed: f32,
    /// Decaying hit desaturation (only active at tier 4).
    hit_desaturation: f32,
    /// Decay rate for hit desaturation (per second, exponential).
    hit_desat_decay: f32,
}

impl ResonanceVfx {
    pub fn new() -> Self {
        Self {
            current_tier: 0,
            visual_tier: 0.0,
            transition_speed: 3.0,
            hit_desaturation: 0.0,
            hit_desat_decay: 4.0,
        }
    }

    /// Call once per frame. `current_tier` is the discrete 0-4 tier from game
    /// logic; `dt` is the frame delta in seconds.
    pub fn update(&mut self, current_tier: i32, dt: f32) {
        self.current_tier = current_tier.clamp(0, 4);
        let target = self.current_tier as f32;

        // Exponential ease toward target — never overshoots, never pops.
        let diff = target - self.visual_tier;
        self.visual_tier += diff * (1.0 - (-self.transition_speed * dt).exp());

        // Snap when extremely close to avoid asymptotic crawl.
        if (self.visual_tier - target).abs() < 1e-4 {
            self.visual_tier = target;
        }

        // Decay hit desaturation.
        if self.hit_desaturation > 1e-4 {
            self.hit_desaturation *= (-self.hit_desat_decay * dt).exp();
            if self.hit_desaturation < 1e-4 {
                self.hit_desaturation = 0.0;
            }
        }
    }

    /// Trigger a screen desaturation flash (tier 4 hit effect).
    /// `intensity` is 0..1 where 1 = fully desaturated.
    pub fn trigger_hit_desaturation(&mut self, intensity: f32) {
        self.hit_desaturation = intensity.clamp(0.0, 1.0);
    }

    /// The current smoothly-interpolated visual tier (0.0 - 4.0).
    pub fn visual_tier(&self) -> f32 {
        self.visual_tier
    }

    /// The discrete tier last set via [`update`].
    pub fn current_tier(&self) -> i32 {
        self.current_tier
    }

    /// Compute render parameters for the current visual tier.
    ///
    /// All values interpolate linearly between tier keyframes so that
    /// transitions are seamless.
    pub fn render_params(&self) -> ResonanceRenderParams {
        let t = self.visual_tier;

        // Each tier defines a keyframe. We lerp between adjacent keyframes
        // based on the fractional part of visual_tier.
        //
        // Tier 0: Baseline — no effect
        // Tier 1: Subtle blue bloom tint, slightly lowered bloom threshold
        // Tier 2: Stronger blue bloom, faint aura (particles), threshold lower
        // Tier 3: Strong bloom, cool color grading, motion blur hint
        // Tier 4: Max bloom, chromatic aberration, intense grading, vignette

        // -- bloom_threshold_offset (more negative = more bloom) --
        let bloom_threshold_offset = lerp_keyframes(t, &[
            0.0,    // tier 0
            -0.10,  // tier 1
            -0.25,  // tier 2
            -0.45,  // tier 3
            -0.65,  // tier 4
        ]);

        // -- bloom_color_tint (R, G, B additive) --
        // Blue channel ramps up; slight green at high tiers for cyan accent.
        let bloom_tint_r = lerp_keyframes(t, &[0.0, 0.0, 0.0, 0.0, 0.02]);
        let bloom_tint_g = lerp_keyframes(t, &[0.0, 0.01, 0.02, 0.04, 0.06]);
        let bloom_tint_b = lerp_keyframes(t, &[0.0, 0.04, 0.10, 0.18, 0.28]);

        // -- chromatic_aberration --
        let chromatic_aberration = lerp_keyframes(t, &[
            0.0,   // tier 0
            0.0,   // tier 1
            0.0,   // tier 2
            0.02,  // tier 3 — barely perceptible
            0.25,  // tier 4 — strong
        ]);

        // -- color_grade_intensity (cool shift) --
        let color_grade_intensity = lerp_keyframes(t, &[
            0.0,   // tier 0
            0.0,   // tier 1
            0.05,  // tier 2
            0.25,  // tier 3
            0.55,  // tier 4
        ]);

        // -- particle_intensity_multiplier --
        let particle_intensity_multiplier = lerp_keyframes(t, &[
            1.0,   // tier 0
            1.0,   // tier 1
            1.5,   // tier 2 — faint aura
            2.5,   // tier 3
            4.0,   // tier 4
        ]);

        // -- vignette_intensity --
        let vignette_intensity = lerp_keyframes(t, &[
            0.0,   // tier 0
            0.0,   // tier 1
            0.0,   // tier 2
            0.08,  // tier 3 — subtle
            0.20,  // tier 4
        ]);

        // -- motion_blur_intensity --
        let motion_blur_intensity = lerp_keyframes(t, &[
            0.0,   // tier 0
            0.0,   // tier 1
            0.0,   // tier 2
            0.15,  // tier 3
            0.30,  // tier 4
        ]);

        // -- hit_desaturation (only meaningful at tier 4, but we expose whatever
        //    was triggered so the renderer can decide) --
        let hit_desaturation = self.hit_desaturation;

        ResonanceRenderParams {
            bloom_threshold_offset,
            bloom_color_tint: [bloom_tint_r, bloom_tint_g, bloom_tint_b],
            chromatic_aberration,
            color_grade_intensity,
            particle_intensity_multiplier,
            vignette_intensity,
            motion_blur_intensity,
            hit_desaturation,
        }
    }
}

impl Default for ResonanceVfx {
    fn default() -> Self {
        Self::new()
    }
}

/// Linearly interpolate a value across tier keyframes.
///
/// `keyframes` must have exactly 5 entries (one per tier 0-4).
/// `t` is the continuous visual tier (0.0 - 4.0).
fn lerp_keyframes(t: f32, keyframes: &[f32; 5]) -> f32 {
    let t = t.clamp(0.0, 4.0);
    let lo = (t.floor() as usize).min(3);
    let hi = lo + 1;
    let frac = t - lo as f32;
    keyframes[lo] + (keyframes[hi] - keyframes[lo]) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_shake_triggers_and_decays() {
        let mut fx = ScreenEffects::new();
        assert_eq!(fx.screen_shake_intensity, 0.0);

        fx.trigger_screen_shake(10.0, 5.0);
        assert_eq!(fx.screen_shake_intensity, 10.0);

        fx.update(0.1);
        assert!(fx.screen_shake_intensity < 10.0);
        assert!(fx.screen_shake_intensity > 0.0);

        for _ in 0..200 {
            fx.update(0.1);
        }
        assert_eq!(fx.screen_shake_intensity, 0.0);
    }

    #[test]
    fn hitstop_triggers_and_decrements() {
        let mut fx = ScreenEffects::new();
        fx.trigger_hitstop(5);
        assert_eq!(fx.hitstop_frames, 5);
        assert!(fx.should_freeze_animation());

        fx.update(0.016);
        assert_eq!(fx.hitstop_frames, 4);

        for _ in 0..4 {
            fx.update(0.016);
        }
        assert_eq!(fx.hitstop_frames, 0);
        assert!(!fx.should_freeze_animation());
    }

    #[test]
    fn camera_offset_nonzero_during_shake() {
        let mut fx = ScreenEffects::new();
        fx.trigger_screen_shake(20.0, 3.0);
        fx.update(0.016);
        let offset = fx.camera_offset();
        assert!(
            offset[0].abs() > 0.0 || offset[1].abs() > 0.0,
            "offset should be non-zero during active shake"
        );
    }

    #[test]
    fn should_freeze_animation_true_during_hitstop() {
        let mut fx = ScreenEffects::new();
        assert!(!fx.should_freeze_animation());
        fx.trigger_hitstop(3);
        assert!(fx.should_freeze_animation());
    }

    #[test]
    fn chromatic_aberration_decays_to_zero() {
        let mut fx = ScreenEffects::new();
        fx.trigger_chromatic_aberration(1.0);
        assert!(fx.chromatic_aberration > 0.0);

        for _ in 0..200 {
            fx.update(0.05);
        }
        assert_eq!(fx.chromatic_aberration, 0.0);
    }

    #[test]
    fn update_returns_false_when_no_effects_active() {
        let mut fx = ScreenEffects::new();
        let active = fx.update(0.016);
        assert!(!active);
    }

    #[test]
    fn update_returns_true_when_effects_active() {
        let mut fx = ScreenEffects::new();
        fx.trigger_screen_shake(5.0, 2.0);
        let active = fx.update(0.016);
        assert!(active);
    }

    #[test]
    fn camera_offset_zero_when_no_shake() {
        let fx = ScreenEffects::new();
        let offset = fx.camera_offset();
        assert_eq!(offset, [0.0, 0.0]);
    }

    #[test]
    fn default_impl_matches_new() {
        let a = ScreenEffects::new();
        let b = ScreenEffects::default();
        assert_eq!(a.screen_shake_intensity, b.screen_shake_intensity);
        assert_eq!(a.hitstop_frames, b.hitstop_frames);
        assert_eq!(a.chromatic_aberration, b.chromatic_aberration);
    }

    // -----------------------------------------------------------------------
    // ResonanceVfx tests
    // -----------------------------------------------------------------------

    #[test]
    fn resonance_vfx_tier_0_is_neutral() {
        let mut vfx = ResonanceVfx::new();
        // Run a few frames at tier 0 so visual_tier settles.
        for _ in 0..60 {
            vfx.update(0, 0.016);
        }
        let p = vfx.render_params();
        assert_eq!(p.bloom_threshold_offset, 0.0);
        assert_eq!(p.bloom_color_tint, [0.0, 0.0, 0.0]);
        assert_eq!(p.chromatic_aberration, 0.0);
        assert_eq!(p.color_grade_intensity, 0.0);
        assert_eq!(p.particle_intensity_multiplier, 1.0);
        assert_eq!(p.vignette_intensity, 0.0);
        assert_eq!(p.motion_blur_intensity, 0.0);
        assert_eq!(p.hit_desaturation, 0.0);
    }

    #[test]
    fn resonance_vfx_tier_4_has_max_effects() {
        let mut vfx = ResonanceVfx::new();
        // Run many frames at tier 4 to reach steady state.
        for _ in 0..300 {
            vfx.update(4, 0.016);
        }
        let p = vfx.render_params();
        // All effects should be at their tier-4 keyframe values.
        assert!((p.bloom_threshold_offset - (-0.65)).abs() < 1e-3);
        assert!((p.bloom_color_tint[2] - 0.28).abs() < 1e-3);
        assert!((p.chromatic_aberration - 0.25).abs() < 1e-3);
        assert!((p.color_grade_intensity - 0.55).abs() < 1e-3);
        assert!((p.particle_intensity_multiplier - 4.0).abs() < 1e-3);
        assert!((p.vignette_intensity - 0.20).abs() < 1e-3);
        assert!((p.motion_blur_intensity - 0.30).abs() < 1e-3);
    }

    #[test]
    fn resonance_vfx_smooth_transition_no_pop() {
        let mut vfx = ResonanceVfx::new();
        // Jump from tier 0 to tier 4 — the visual tier should NOT jump.
        vfx.update(4, 0.016);
        assert!(
            vfx.visual_tier() < 1.0,
            "visual_tier should not jump immediately to 4; got {}",
            vfx.visual_tier()
        );

        // After several frames it should approach but still be below 4.
        for _ in 0..10 {
            vfx.update(4, 0.016);
        }
        assert!(vfx.visual_tier() > 0.0);
        assert!(vfx.visual_tier() < 4.0);
    }

    #[test]
    fn resonance_vfx_settles_at_target() {
        let mut vfx = ResonanceVfx::new();
        for _ in 0..600 {
            vfx.update(3, 0.016);
        }
        assert!(
            (vfx.visual_tier() - 3.0).abs() < 1e-3,
            "visual_tier should settle at 3.0; got {}",
            vfx.visual_tier()
        );
    }

    #[test]
    fn resonance_vfx_transitions_down_smoothly() {
        let mut vfx = ResonanceVfx::new();
        // Settle at tier 4.
        for _ in 0..600 {
            vfx.update(4, 0.016);
        }
        assert!((vfx.visual_tier() - 4.0).abs() < 1e-3);

        // Drop to tier 1 — should not jump.
        vfx.update(1, 0.016);
        assert!(
            vfx.visual_tier() > 2.0,
            "should not pop down immediately; got {}",
            vfx.visual_tier()
        );

        // Eventually settle.
        for _ in 0..600 {
            vfx.update(1, 0.016);
        }
        assert!((vfx.visual_tier() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn resonance_vfx_intermediate_values_interpolate() {
        let mut vfx = ResonanceVfx::new();
        // Settle at tier 2.
        for _ in 0..600 {
            vfx.update(2, 0.016);
        }
        let p = vfx.render_params();
        // Tier 2 keyframe values.
        assert!((p.bloom_threshold_offset - (-0.25)).abs() < 1e-3);
        assert!((p.particle_intensity_multiplier - 1.5).abs() < 1e-3);
    }

    #[test]
    fn resonance_vfx_fractional_tier_lerps_between_keyframes() {
        // Manually construct a VFX state with visual_tier between two tiers.
        let mut vfx = ResonanceVfx::new();
        // Force visual_tier to 1.5 (between tier 1 and tier 2).
        vfx.visual_tier = 1.5;
        vfx.current_tier = 2;
        let p = vfx.render_params();

        // bloom_threshold_offset: tier1=-0.10, tier2=-0.25 => midpoint = -0.175
        assert!(
            (p.bloom_threshold_offset - (-0.175)).abs() < 1e-3,
            "expected -0.175, got {}",
            p.bloom_threshold_offset
        );

        // particle_intensity_multiplier: tier1=1.0, tier2=1.5 => midpoint = 1.25
        assert!(
            (p.particle_intensity_multiplier - 1.25).abs() < 1e-3,
            "expected 1.25, got {}",
            p.particle_intensity_multiplier
        );
    }

    #[test]
    fn resonance_vfx_clamps_tier_input() {
        let mut vfx = ResonanceVfx::new();
        vfx.update(-1, 0.016);
        assert_eq!(vfx.current_tier(), 0);
        vfx.update(10, 0.016);
        assert_eq!(vfx.current_tier(), 4);
    }

    #[test]
    fn resonance_vfx_hit_desaturation_decays() {
        let mut vfx = ResonanceVfx::new();
        vfx.trigger_hit_desaturation(1.0);
        let p0 = vfx.render_params();
        assert!((p0.hit_desaturation - 1.0).abs() < 1e-3);

        // Advance several frames — it should decay.
        for _ in 0..120 {
            vfx.update(4, 0.016);
        }
        let p1 = vfx.render_params();
        assert!(
            p1.hit_desaturation < 0.01,
            "hit_desaturation should have decayed; got {}",
            p1.hit_desaturation
        );
    }

    #[test]
    fn resonance_vfx_hit_desaturation_clamps_input() {
        let mut vfx = ResonanceVfx::new();
        vfx.trigger_hit_desaturation(5.0);
        assert!((vfx.render_params().hit_desaturation - 1.0).abs() < 1e-3);
        vfx.trigger_hit_desaturation(-2.0);
        assert_eq!(vfx.render_params().hit_desaturation, 0.0);
    }

    #[test]
    fn resonance_vfx_default_matches_new() {
        let a = ResonanceVfx::new();
        let b = ResonanceVfx::default();
        assert_eq!(a.visual_tier(), b.visual_tier());
        assert_eq!(a.current_tier(), b.current_tier());
    }

    #[test]
    fn resonance_render_params_default_is_neutral() {
        let p = ResonanceRenderParams::default();
        assert_eq!(p.bloom_threshold_offset, 0.0);
        assert_eq!(p.bloom_color_tint, [0.0, 0.0, 0.0]);
        assert_eq!(p.chromatic_aberration, 0.0);
        assert_eq!(p.color_grade_intensity, 0.0);
        assert_eq!(p.particle_intensity_multiplier, 1.0);
        assert_eq!(p.vignette_intensity, 0.0);
        assert_eq!(p.motion_blur_intensity, 0.0);
        assert_eq!(p.hit_desaturation, 0.0);
    }

    #[test]
    fn lerp_keyframes_at_exact_tiers() {
        let kf = [10.0, 20.0, 30.0, 40.0, 50.0];
        assert!((lerp_keyframes(0.0, &kf) - 10.0).abs() < 1e-6);
        assert!((lerp_keyframes(1.0, &kf) - 20.0).abs() < 1e-6);
        assert!((lerp_keyframes(2.0, &kf) - 30.0).abs() < 1e-6);
        assert!((lerp_keyframes(3.0, &kf) - 40.0).abs() < 1e-6);
        assert!((lerp_keyframes(4.0, &kf) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_keyframes_midpoints() {
        let kf = [0.0, 10.0, 20.0, 30.0, 40.0];
        assert!((lerp_keyframes(0.5, &kf) - 5.0).abs() < 1e-6);
        assert!((lerp_keyframes(2.5, &kf) - 25.0).abs() < 1e-6);
    }

    #[test]
    fn lerp_keyframes_clamps_out_of_range() {
        let kf = [0.0, 10.0, 20.0, 30.0, 40.0];
        // Below 0 should clamp to tier 0.
        assert!((lerp_keyframes(-1.0, &kf) - 0.0).abs() < 1e-6);
        // Above 4 should clamp to tier 4.
        assert!((lerp_keyframes(5.0, &kf) - 40.0).abs() < 1e-6);
    }

    #[test]
    fn resonance_vfx_tier1_has_blue_tint_and_lowered_threshold() {
        let mut vfx = ResonanceVfx::new();
        for _ in 0..600 {
            vfx.update(1, 0.016);
        }
        let p = vfx.render_params();
        // Tier 1: bloom tint blue = 0.04, threshold offset = -0.10
        assert!((p.bloom_color_tint[2] - 0.04).abs() < 1e-3);
        assert!((p.bloom_threshold_offset - (-0.10)).abs() < 1e-3);
        // No chromatic aberration or motion blur at tier 1.
        assert!(p.chromatic_aberration < 1e-3);
        assert!(p.motion_blur_intensity < 1e-3);
    }

    #[test]
    fn resonance_vfx_tier3_has_motion_blur_and_color_grade() {
        let mut vfx = ResonanceVfx::new();
        for _ in 0..600 {
            vfx.update(3, 0.016);
        }
        let p = vfx.render_params();
        assert!((p.motion_blur_intensity - 0.15).abs() < 1e-3);
        assert!((p.color_grade_intensity - 0.25).abs() < 1e-3);
    }

    #[test]
    fn resonance_vfx_monotonic_escalation() {
        // Every tier should have effects >= the previous tier.
        let mut params_per_tier = Vec::new();
        for tier in 0..=4 {
            let mut vfx = ResonanceVfx::new();
            for _ in 0..600 {
                vfx.update(tier, 0.016);
            }
            params_per_tier.push(vfx.render_params());
        }
        for i in 1..5 {
            let prev = &params_per_tier[i - 1];
            let curr = &params_per_tier[i];
            assert!(
                curr.bloom_threshold_offset <= prev.bloom_threshold_offset,
                "tier {} bloom_threshold_offset ({}) should be <= tier {} ({})",
                i, curr.bloom_threshold_offset, i - 1, prev.bloom_threshold_offset
            );
            assert!(
                curr.bloom_color_tint[2] >= prev.bloom_color_tint[2],
                "tier {} bloom blue tint ({}) should be >= tier {} ({})",
                i, curr.bloom_color_tint[2], i - 1, prev.bloom_color_tint[2]
            );
            assert!(
                curr.particle_intensity_multiplier >= prev.particle_intensity_multiplier,
                "tier {} particle mult ({}) should be >= tier {} ({})",
                i, curr.particle_intensity_multiplier, i - 1, prev.particle_intensity_multiplier
            );
        }
    }
}
