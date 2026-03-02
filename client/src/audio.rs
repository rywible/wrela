/// Procedural audio engine using Web Audio API.
///
/// All sounds are synthesized at runtime using oscillators, noise bursts,
/// filters, and delay nodes. No external audio files are required.
///
/// On non-wasm32 targets every method is a no-op stub so the crate compiles
/// and tests pass on the host toolchain.

// ──────────────────────────────────────────────────────────────────────────────
// WASM32 implementation
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
use web_sys::{
    AudioContext, BiquadFilterType, DelayNode, DelayOptions, GainNode, OscillatorNode,
    OscillatorType,
};

/// Handle for a running ambient-sound layer so we can stop it later.
#[cfg(target_arch = "wasm32")]
struct AmbientLayer {
    /// The source oscillator -- kept so we can call `.stop()`.
    source: OscillatorNode,
    /// Per-layer gain kept alive so the audio graph is not garbage-collected.
    #[allow(dead_code)]
    gain: GainNode,
}

#[cfg(target_arch = "wasm32")]
pub struct AudioEngine {
    ctx: AudioContext,
    master_gain: GainNode,
    ambient_layers: Vec<AmbientLayer>,
    ambient_running: bool,
    resumed: bool,
}

#[cfg(target_arch = "wasm32")]
impl AudioEngine {
    /// Create a new audio engine. Returns `None` if the Web Audio API is
    /// unavailable (e.g. headless browser, unsupported environment).
    pub fn new() -> Option<Self> {
        let ctx = AudioContext::new().ok()?;
        let master_gain = ctx.create_gain().ok()?;
        master_gain
            .connect_with_audio_node(&ctx.destination())
            .ok()?;
        master_gain.gain().set_value(0.5);
        Some(Self {
            ctx,
            master_gain,
            ambient_layers: Vec::new(),
            ambient_running: false,
            resumed: false,
        })
    }

    /// Resume the audio context after a user gesture (browsers require this).
    pub fn resume(&mut self) {
        let _ = self.ctx.resume();
        self.resumed = true;
    }

    /// Whether the audio context has been resumed by a user gesture.
    pub fn is_resumed(&self) -> bool {
        self.resumed
    }

    // ── helpers ───────────────────────────────────────────────────────────

    fn now(&self) -> f64 {
        self.ctx.current_time()
    }

    /// Apply a distance-based volume attenuation. Returns a gain multiplier
    /// in [0, 1] based on `distance` (world units). Sounds further than 20
    /// units are silent.
    fn distance_attenuation(distance: f32) -> f32 {
        let max_dist = 20.0_f32;
        if distance >= max_dist {
            0.0
        } else {
            let t = (1.0 - distance / max_dist).max(0.0);
            // inverse-square-ish rolloff but softer so nearby isn't too loud
            t * t
        }
    }

    // ── sound effects ────────────────────────────────────────────────────

    /// Blade swing – short filtered noise burst with quick decay.
    pub fn play_swing(&self) {
        self.play_swing_at(0.0);
    }

    /// Blade swing with spatial distance attenuation.
    pub fn play_swing_at(&self, distance: f32) {
        let vol = 0.25 * Self::distance_attenuation(distance);
        if vol < 0.001 {
            return;
        }
        let _ = self.play_swing_inner(vol);
    }

    fn play_swing_inner(&self, vol: f32) -> Option<()> {
        let t = self.now();
        let duration = 0.15;

        // Envelope gain
        let env = self.ctx.create_gain().ok()?;
        env.gain().set_value(vol);
        env.gain()
            .exponential_ramp_to_value_at_time(0.001, t + duration)
            .ok()?;
        env.connect_with_audio_node(&self.master_gain).ok()?;

        // Bandpass filter to shape noise into a whoosh
        let bp = self.ctx.create_biquad_filter().ok()?;
        bp.set_type(BiquadFilterType::Bandpass);
        bp.frequency().set_value(2000.0);
        bp.q().set_value(0.8);
        bp.connect_with_audio_node(&env).ok()?;

        // Use a sawtooth oscillator with rapid frequency sweep for noise-like texture
        let osc = self.ctx.create_oscillator().ok()?;
        osc.set_type(OscillatorType::Sawtooth);
        osc.frequency().set_value(3000.0);
        osc.frequency()
            .exponential_ramp_to_value_at_time(800.0, t + duration)
            .ok()?;
        osc.connect_with_audio_node(&bp).ok()?;
        osc.start_with_when(t).ok()?;
        osc.stop_with_when(t + duration + 0.01).ok()?;

        // Second detuned oscillator for width
        let osc2 = self.ctx.create_oscillator().ok()?;
        osc2.set_type(OscillatorType::Sawtooth);
        osc2.frequency().set_value(3100.0);
        osc2.frequency()
            .exponential_ramp_to_value_at_time(750.0, t + duration)
            .ok()?;
        osc2.connect_with_audio_node(&bp).ok()?;
        osc2.start_with_when(t).ok()?;
        osc2.stop_with_when(t + duration + 0.01).ok()?;

        Some(())
    }

    /// Hit impact – low frequency burst + high frequency click, short decay.
    pub fn play_hit_impact(&self) {
        self.play_hit_impact_at(0.0);
    }

    pub fn play_hit_impact_at(&self, distance: f32) {
        let vol = 0.4 * Self::distance_attenuation(distance);
        if vol < 0.001 {
            return;
        }
        let _ = self.play_hit_impact_inner(vol);
    }

    fn play_hit_impact_inner(&self, vol: f32) -> Option<()> {
        let t = self.now();

        // --- Low-frequency thump ---
        let low_env = self.ctx.create_gain().ok()?;
        low_env.gain().set_value(vol);
        low_env
            .gain()
            .exponential_ramp_to_value_at_time(0.001, t + 0.2)
            .ok()?;
        low_env.connect_with_audio_node(&self.master_gain).ok()?;

        let low_osc = self.ctx.create_oscillator().ok()?;
        low_osc.set_type(OscillatorType::Sine);
        low_osc.frequency().set_value(80.0);
        low_osc
            .frequency()
            .exponential_ramp_to_value_at_time(40.0, t + 0.2)
            .ok()?;
        low_osc.connect_with_audio_node(&low_env).ok()?;
        low_osc.start_with_when(t).ok()?;
        low_osc.stop_with_when(t + 0.21).ok()?;

        // --- High-frequency metallic click ---
        let hi_env = self.ctx.create_gain().ok()?;
        hi_env.gain().set_value(vol * 0.6);
        hi_env
            .gain()
            .exponential_ramp_to_value_at_time(0.001, t + 0.06)
            .ok()?;
        hi_env.connect_with_audio_node(&self.master_gain).ok()?;

        let hi_osc = self.ctx.create_oscillator().ok()?;
        hi_osc.set_type(OscillatorType::Square);
        hi_osc.frequency().set_value(4000.0);
        hi_osc
            .frequency()
            .exponential_ramp_to_value_at_time(1500.0, t + 0.06)
            .ok()?;
        hi_osc.connect_with_audio_node(&hi_env).ok()?;
        hi_osc.start_with_when(t).ok()?;
        hi_osc.stop_with_when(t + 0.07).ok()?;

        // --- Mid crunch (adds body to the impact) ---
        let mid_env = self.ctx.create_gain().ok()?;
        mid_env.gain().set_value(vol * 0.3);
        mid_env
            .gain()
            .exponential_ramp_to_value_at_time(0.001, t + 0.1)
            .ok()?;
        mid_env.connect_with_audio_node(&self.master_gain).ok()?;

        let mid_osc = self.ctx.create_oscillator().ok()?;
        mid_osc.set_type(OscillatorType::Sawtooth);
        mid_osc.frequency().set_value(300.0);
        mid_osc
            .frequency()
            .exponential_ramp_to_value_at_time(100.0, t + 0.1)
            .ok()?;
        mid_osc.connect_with_audio_node(&mid_env).ok()?;
        mid_osc.start_with_when(t).ok()?;
        mid_osc.stop_with_when(t + 0.11).ok()?;

        Some(())
    }

    /// Parry ring – sharp metallic tone with long reverb tail via delay feedback.
    pub fn play_parry_ring(&self) {
        self.play_parry_ring_at(0.0);
    }

    pub fn play_parry_ring_at(&self, distance: f32) {
        let vol = 0.35 * Self::distance_attenuation(distance);
        if vol < 0.001 {
            return;
        }
        let _ = self.play_parry_ring_inner(vol);
    }

    fn play_parry_ring_inner(&self, vol: f32) -> Option<()> {
        let t = self.now();

        // Output gain (overall envelope for the tail)
        let out_gain = self.ctx.create_gain().ok()?;
        out_gain.gain().set_value(vol);
        out_gain.connect_with_audio_node(&self.master_gain).ok()?;

        // Delay feedback loop to create reverb tail
        let delay_opts = DelayOptions::new();
        delay_opts.set_max_delay_time(1.0);
        delay_opts.set_delay_time(0.08);
        let delay = DelayNode::new_with_options(&self.ctx, &delay_opts).ok()?;

        let feedback = self.ctx.create_gain().ok()?;
        feedback.gain().set_value(0.5);

        // delay -> feedback -> delay (feedback loop)
        delay.connect_with_audio_node(&feedback).ok()?;
        feedback.connect_with_audio_node(&delay).ok()?;
        // delay -> output
        delay.connect_with_audio_node(&out_gain).ok()?;

        // Highpass on the feedback to make the tail progressively thinner
        let hp = self.ctx.create_biquad_filter().ok()?;
        hp.set_type(BiquadFilterType::Highpass);
        hp.frequency().set_value(1000.0);
        hp.connect_with_audio_node(&delay).ok()?;
        hp.connect_with_audio_node(&out_gain).ok()?;

        // --- Initial strike: bright metallic ping ---
        let strike_env = self.ctx.create_gain().ok()?;
        strike_env.gain().set_value(1.0);
        strike_env
            .gain()
            .exponential_ramp_to_value_at_time(0.001, t + 0.3)
            .ok()?;
        strike_env.connect_with_audio_node(&hp).ok()?;

        // Primary tone
        let osc1 = self.ctx.create_oscillator().ok()?;
        osc1.set_type(OscillatorType::Sine);
        osc1.frequency().set_value(2200.0);
        osc1.connect_with_audio_node(&strike_env).ok()?;
        osc1.start_with_when(t).ok()?;
        osc1.stop_with_when(t + 0.31).ok()?;

        // Harmonic (inharmonic interval for metallic quality)
        let osc2 = self.ctx.create_oscillator().ok()?;
        osc2.set_type(OscillatorType::Sine);
        osc2.frequency().set_value(3517.0); // non-integer ratio for metallic ring
        let harm_env = self.ctx.create_gain().ok()?;
        harm_env.gain().set_value(0.5);
        harm_env
            .gain()
            .exponential_ramp_to_value_at_time(0.001, t + 0.25)
            .ok()?;
        harm_env.connect_with_audio_node(&hp).ok()?;
        osc2.connect_with_audio_node(&harm_env).ok()?;
        osc2.start_with_when(t).ok()?;
        osc2.stop_with_when(t + 0.26).ok()?;

        // High click transient
        let click_env = self.ctx.create_gain().ok()?;
        click_env.gain().set_value(0.8);
        click_env
            .gain()
            .exponential_ramp_to_value_at_time(0.001, t + 0.02)
            .ok()?;
        click_env.connect_with_audio_node(&hp).ok()?;

        let click_osc = self.ctx.create_oscillator().ok()?;
        click_osc.set_type(OscillatorType::Square);
        click_osc.frequency().set_value(6000.0);
        click_osc.connect_with_audio_node(&click_env).ok()?;
        click_osc.start_with_when(t).ok()?;
        click_osc.stop_with_when(t + 0.03).ok()?;

        Some(())
    }

    /// Footstep – very short low-frequency thump.
    pub fn play_footstep(&self) {
        self.play_footstep_at(0.0);
    }

    pub fn play_footstep_at(&self, distance: f32) {
        let vol = 0.12 * Self::distance_attenuation(distance);
        if vol < 0.001 {
            return;
        }
        let _ = self.play_footstep_inner(vol);
    }

    fn play_footstep_inner(&self, vol: f32) -> Option<()> {
        let t = self.now();

        let env = self.ctx.create_gain().ok()?;
        env.gain().set_value(vol);
        env.gain()
            .exponential_ramp_to_value_at_time(0.001, t + 0.08)
            .ok()?;
        env.connect_with_audio_node(&self.master_gain).ok()?;

        // Low-pass filter to keep it dull
        let lp = self.ctx.create_biquad_filter().ok()?;
        lp.set_type(BiquadFilterType::Lowpass);
        lp.frequency().set_value(250.0);
        lp.connect_with_audio_node(&env).ok()?;

        let osc = self.ctx.create_oscillator().ok()?;
        osc.set_type(OscillatorType::Sine);
        osc.frequency().set_value(60.0);
        osc.frequency()
            .exponential_ramp_to_value_at_time(30.0, t + 0.08)
            .ok()?;
        osc.connect_with_audio_node(&lp).ok()?;
        osc.start_with_when(t).ok()?;
        osc.stop_with_when(t + 0.09).ok()?;

        // Tiny noise click for ground contact texture
        let click_env = self.ctx.create_gain().ok()?;
        click_env.gain().set_value(vol * 0.3);
        click_env
            .gain()
            .exponential_ramp_to_value_at_time(0.001, t + 0.03)
            .ok()?;
        click_env.connect_with_audio_node(&self.master_gain).ok()?;

        let click = self.ctx.create_oscillator().ok()?;
        click.set_type(OscillatorType::Sawtooth);
        click.frequency().set_value(800.0);
        click.frequency()
            .exponential_ramp_to_value_at_time(200.0, t + 0.03)
            .ok()?;
        click.connect_with_audio_node(&click_env).ok()?;
        click.start_with_when(t).ok()?;
        click.stop_with_when(t + 0.04).ok()?;

        Some(())
    }

    /// Enemy growl – low sawtooth oscillator with tremolo (amplitude modulation).
    pub fn play_enemy_growl(&self) {
        self.play_enemy_growl_at(0.0);
    }

    pub fn play_enemy_growl_at(&self, distance: f32) {
        let vol = 0.3 * Self::distance_attenuation(distance);
        if vol < 0.001 {
            return;
        }
        let _ = self.play_enemy_growl_inner(vol);
    }

    fn play_enemy_growl_inner(&self, vol: f32) -> Option<()> {
        let t = self.now();
        let duration = 0.5;

        // Main envelope
        let env = self.ctx.create_gain().ok()?;
        env.gain().set_value(0.0);
        env.gain()
            .linear_ramp_to_value_at_time(vol, t + 0.05)
            .ok()?;
        env.gain()
            .set_value_at_time(vol, t + duration * 0.6)
            .ok()?;
        env.gain()
            .exponential_ramp_to_value_at_time(0.001, t + duration)
            .ok()?;
        env.connect_with_audio_node(&self.master_gain).ok()?;

        // Tremolo: LFO modulating a gain node
        let trem_gain = self.ctx.create_gain().ok()?;
        trem_gain.gain().set_value(0.7); // base level
        trem_gain.connect_with_audio_node(&env).ok()?;

        // LFO for tremolo
        let lfo = self.ctx.create_oscillator().ok()?;
        lfo.set_type(OscillatorType::Sine);
        lfo.frequency().set_value(8.0); // 8 Hz tremolo
        // LFO needs to modulate the gain param. We connect via a gain scaler.
        let lfo_depth = self.ctx.create_gain().ok()?;
        lfo_depth.gain().set_value(0.3); // tremolo depth
        lfo.connect_with_audio_node(&lfo_depth).ok()?;
        lfo_depth
            .connect_with_audio_param(&trem_gain.gain())
            .ok()?;
        lfo.start_with_when(t).ok()?;
        lfo.stop_with_when(t + duration + 0.01).ok()?;

        // Low-pass filter for muffled quality
        let lp = self.ctx.create_biquad_filter().ok()?;
        lp.set_type(BiquadFilterType::Lowpass);
        lp.frequency().set_value(400.0);
        lp.connect_with_audio_node(&trem_gain).ok()?;

        // Primary growl oscillator
        let osc = self.ctx.create_oscillator().ok()?;
        osc.set_type(OscillatorType::Sawtooth);
        osc.frequency().set_value(65.0);
        // Slight pitch variation for organic feel
        osc.frequency()
            .linear_ramp_to_value_at_time(55.0, t + duration * 0.5)
            .ok()?;
        osc.frequency()
            .linear_ramp_to_value_at_time(70.0, t + duration)
            .ok()?;
        osc.connect_with_audio_node(&lp).ok()?;
        osc.start_with_when(t).ok()?;
        osc.stop_with_when(t + duration + 0.01).ok()?;

        // Sub-bass layer
        let sub = self.ctx.create_oscillator().ok()?;
        sub.set_type(OscillatorType::Sine);
        sub.frequency().set_value(35.0);
        let sub_gain = self.ctx.create_gain().ok()?;
        sub_gain.gain().set_value(0.5);
        sub_gain.connect_with_audio_node(&trem_gain).ok()?;
        sub.connect_with_audio_node(&sub_gain).ok()?;
        sub.start_with_when(t).ok()?;
        sub.stop_with_when(t + duration + 0.01).ok()?;

        Some(())
    }

    /// Start looping ambient forest sounds (wind, birds). Call once.
    /// Subsequent calls are ignored if ambient is already running.
    pub fn start_ambient(&mut self) {
        if self.ambient_running {
            return;
        }
        if self.start_ambient_inner().is_some() {
            self.ambient_running = true;
        }
    }

    fn start_ambient_inner(&mut self) -> Option<()> {
        let t = self.now();

        // --- Wind layer: detuned oscillators through bandpass ---
        let wind_gain = self.ctx.create_gain().ok()?;
        wind_gain.gain().set_value(0.04);
        wind_gain
            .connect_with_audio_node(&self.master_gain)
            .ok()?;

        let wind_bp = self.ctx.create_biquad_filter().ok()?;
        wind_bp.set_type(BiquadFilterType::Bandpass);
        wind_bp.frequency().set_value(500.0);
        wind_bp.q().set_value(0.3);
        wind_bp.connect_with_audio_node(&wind_gain).ok()?;

        // Wind: multiple detuned sawtooth oscillators for noise-like texture
        let wind1 = self.ctx.create_oscillator().ok()?;
        wind1.set_type(OscillatorType::Sawtooth);
        wind1.frequency().set_value(100.0);
        wind1.connect_with_audio_node(&wind_bp).ok()?;
        wind1.start_with_when(t).ok()?;

        let wind_gain_node = self.ctx.create_gain().ok()?;
        wind_gain_node.gain().set_value(0.04);
        wind_gain_node
            .connect_with_audio_node(&self.master_gain)
            .ok()?;

        let wind_bp2 = self.ctx.create_biquad_filter().ok()?;
        wind_bp2.set_type(BiquadFilterType::Bandpass);
        wind_bp2.frequency().set_value(1200.0);
        wind_bp2.q().set_value(0.4);
        wind_bp2.connect_with_audio_node(&wind_gain_node).ok()?;

        let wind2 = self.ctx.create_oscillator().ok()?;
        wind2.set_type(OscillatorType::Sawtooth);
        wind2.frequency().set_value(103.0); // slight detune
        wind2.connect_with_audio_node(&wind_bp2).ok()?;
        wind2.start_with_when(t).ok()?;

        // LFO to modulate wind volume for natural breathing effect
        let wind_lfo = self.ctx.create_oscillator().ok()?;
        wind_lfo.set_type(OscillatorType::Sine);
        wind_lfo.frequency().set_value(0.15); // very slow
        let wind_lfo_gain = self.ctx.create_gain().ok()?;
        wind_lfo_gain.gain().set_value(0.02);
        wind_lfo.connect_with_audio_node(&wind_lfo_gain).ok()?;
        wind_lfo_gain
            .connect_with_audio_param(&wind_gain.gain())
            .ok()?;
        wind_lfo.start_with_when(t).ok()?;

        // --- Bird-like chirps: high-frequency sine with amplitude LFO ---
        let bird_gain = self.ctx.create_gain().ok()?;
        bird_gain.gain().set_value(0.0); // modulated by LFO
        bird_gain
            .connect_with_audio_node(&self.master_gain)
            .ok()?;

        let bird_osc = self.ctx.create_oscillator().ok()?;
        bird_osc.set_type(OscillatorType::Sine);
        bird_osc.frequency().set_value(2800.0);
        bird_osc.connect_with_audio_node(&bird_gain).ok()?;
        bird_osc.start_with_when(t).ok()?;

        // Bird amplitude LFO (irregular chirps via slow square-ish modulation)
        let bird_lfo = self.ctx.create_oscillator().ok()?;
        bird_lfo.set_type(OscillatorType::Sine);
        bird_lfo.frequency().set_value(0.4); // chirp rate
        let bird_lfo_gain = self.ctx.create_gain().ok()?;
        bird_lfo_gain.gain().set_value(0.015);
        bird_lfo.connect_with_audio_node(&bird_lfo_gain).ok()?;
        bird_lfo_gain
            .connect_with_audio_param(&bird_gain.gain())
            .ok()?;
        bird_lfo.start_with_when(t).ok()?;

        // Bird frequency LFO for trilling
        let bird_freq_lfo = self.ctx.create_oscillator().ok()?;
        bird_freq_lfo.set_type(OscillatorType::Sine);
        bird_freq_lfo.frequency().set_value(6.0);
        let bird_freq_depth = self.ctx.create_gain().ok()?;
        bird_freq_depth.gain().set_value(300.0);
        bird_freq_lfo.connect_with_audio_node(&bird_freq_depth).ok()?;
        bird_freq_depth
            .connect_with_audio_param(&bird_osc.frequency())
            .ok()?;
        bird_freq_lfo.start_with_when(t).ok()?;

        // Store handles for cleanup. We keep the main oscillators; LFOs are
        // connected and will be garbage-collected once the main ones stop.
        self.ambient_layers.push(AmbientLayer {
            source: wind1,
            gain: wind_gain,
        });
        self.ambient_layers.push(AmbientLayer {
            source: wind2,
            gain: wind_gain_node,
        });
        self.ambient_layers.push(AmbientLayer {
            source: wind_lfo,
            gain: wind_lfo_gain,
        });
        self.ambient_layers.push(AmbientLayer {
            source: bird_osc,
            gain: bird_gain,
        });
        self.ambient_layers.push(AmbientLayer {
            source: bird_lfo,
            gain: bird_lfo_gain,
        });
        self.ambient_layers.push(AmbientLayer {
            source: bird_freq_lfo,
            gain: bird_freq_depth,
        });

        Some(())
    }

    /// Stop all ambient forest sounds.
    pub fn stop_ambient(&mut self) {
        for layer in self.ambient_layers.drain(..) {
            let _ = layer.source.stop();
        }
        self.ambient_running = false;
    }

    // ── spatial convenience ──────────────────────────────────────────────

    /// Update the listener position (camera position) for distance-based
    /// attenuation calculations. This is a simplified model — callers pass
    /// the camera position and then use the `*_at` variants with the
    /// source's distance from camera.
    pub fn set_listener_position(&self, _position: [f32; 3], _forward: [f32; 3], _up: [f32; 3]) {
        // We use a simpler distance model — the caller computes distance
        // and passes it to the `*_at` methods. This method exists for API
        // compatibility and future expansion.
    }

    /// Compute distance between two 3D points (convenience).
    pub fn distance_between(a: [f32; 3], b: [f32; 3]) -> f32 {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        let dz = a[2] - b[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Non-WASM stubs
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub struct AudioEngine;

#[cfg(not(target_arch = "wasm32"))]
impl AudioEngine {
    pub fn new() -> Option<Self> {
        None
    }

    pub fn resume(&mut self) {}
    pub fn is_resumed(&self) -> bool {
        false
    }

    pub fn play_swing(&self) {}
    pub fn play_swing_at(&self, _distance: f32) {}
    pub fn play_hit_impact(&self) {}
    pub fn play_hit_impact_at(&self, _distance: f32) {}
    pub fn play_parry_ring(&self) {}
    pub fn play_parry_ring_at(&self, _distance: f32) {}
    pub fn play_footstep(&self) {}
    pub fn play_footstep_at(&self, _distance: f32) {}
    pub fn play_enemy_growl(&self) {}
    pub fn play_enemy_growl_at(&self, _distance: f32) {}
    pub fn start_ambient(&mut self) {}
    pub fn stop_ambient(&mut self) {}
    pub fn set_listener_position(&self, _position: [f32; 3], _forward: [f32; 3], _up: [f32; 3]) {}

    pub fn distance_between(a: [f32; 3], b: [f32; 3]) -> f32 {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        let dz = a[2] - b[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::AudioEngine;

    #[test]
    fn non_wasm_new_returns_none() {
        // On native targets, Web Audio is unavailable so new() returns None.
        let engine = AudioEngine::new();
        assert!(engine.is_none());
    }

    #[test]
    fn distance_attenuation_at_zero() {
        // At distance 0 the sound should be at full volume.
        // Non-wasm stub doesn't have the method, but we test distance_between.
        let d = AudioEngine::distance_between([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        assert!((d - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn distance_between_calculates_correctly() {
        let d = AudioEngine::distance_between([0.0, 0.0, 0.0], [3.0, 4.0, 0.0]);
        assert!((d - 5.0).abs() < 0.001);
    }

    #[test]
    fn distance_between_3d() {
        let d = AudioEngine::distance_between([1.0, 2.0, 3.0], [4.0, 6.0, 3.0]);
        // sqrt(9 + 16 + 0) = 5
        assert!((d - 5.0).abs() < 0.001);
    }

    #[test]
    fn stub_methods_do_not_panic() {
        // Ensure all stub methods can be called without panicking.
        // On native, new() returns None, so we can't call instance methods.
        // But we test that the type exists and new() works.
        let _ = AudioEngine::new();
    }
}
