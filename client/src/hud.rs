/// HTML overlay HUD for the souls-like combat game.
///
/// Creates DOM elements once at initialization and updates their values each
/// frame.  All styling uses inline CSS for a dark, minimalist souls-like
/// aesthetic.
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlElement};

/// Snapshot of game state values the HUD needs each frame.
pub(crate) struct HudState {
    pub player_health_ratio: f32,
    pub player_stamina_ratio: f32,
    pub enemy_health_ratio: f32,
    pub enemy_alive: bool,
    pub resonance_tier: i32,
    pub resonance: i32,
    pub kills: i32,
    pub player_dead: bool,
}

/// Holds references to every HUD DOM element so we only query once.
pub(crate) struct Hud {
    // Container
    _container: HtmlElement,

    // Player health
    player_hp_fill: HtmlElement,
    player_hp_text: HtmlElement,

    // Player stamina
    stamina_fill: HtmlElement,

    // Enemy health
    enemy_bar: HtmlElement,
    enemy_hp_fill: HtmlElement,
    enemy_hp_text: HtmlElement,

    // Resonance
    res_label: HtmlElement,
    res_indicator: HtmlElement,
    res_fill: HtmlElement,

    // Kill counter
    kill_text: HtmlElement,

    // Death screen
    death_overlay: HtmlElement,

    // Controls help
    controls_help: HtmlElement,
    controls_visible: bool,
    controls_start_ms: Option<f64>,
}

// ────────────────────────────────────────────────────────────────────
// CSS keyframes injected once
// ────────────────────────────────────────────────────────────────────
const GLOW_KEYFRAMES: &str = r#"
@keyframes hud-pulse {
    0%   { opacity: 0.7; filter: brightness(1.0); }
    50%  { opacity: 1.0; filter: brightness(1.5); }
    100% { opacity: 0.7; filter: brightness(1.0); }
}
@keyframes hud-pulse-fast {
    0%   { opacity: 0.6; filter: brightness(1.0); }
    50%  { opacity: 1.0; filter: brightness(1.8); }
    100% { opacity: 0.6; filter: brightness(1.0); }
}
@keyframes death-fade {
    0%   { opacity: 0; }
    100% { opacity: 1; }
}
"#;

impl Hud {
    /// Create the HUD overlay.  Appends elements to the canvas's parent or
    /// `<body>`.
    pub fn create(document: &Document) -> Result<Self, String> {
        // Inject keyframes stylesheet
        inject_keyframes(document)?;

        let body = document
            .body()
            .ok_or_else(|| "document.body unavailable".to_string())?;

        // Find the canvas parent — we want to position the overlay relative to it.
        let parent: HtmlElement = match document.get_element_by_id("game-canvas") {
            Some(canvas_el) => canvas_el
                .parent_element()
                .and_then(|p| p.dyn_into::<HtmlElement>().ok())
                .unwrap_or_else(|| body.clone()),
            None => body.clone(),
        };

        // Make sure the parent has position:relative so the overlay stacks
        let _ = parent.style().set_property("position", "relative");

        // ── Main container ──────────────────────────────────────────
        let container = create_div(document, "wrela-hud")?;
        set_styles(&container, &[
            ("position", "absolute"),
            ("top", "0"),
            ("left", "0"),
            ("width", "100%"),
            ("height", "100%"),
            ("pointer-events", "none"),
            ("font-family", "'Segoe UI', 'Helvetica Neue', Arial, sans-serif"),
            ("font-size", "13px"),
            ("color", "#d4c8b8"),
            ("z-index", "100"),
            ("user-select", "none"),
            ("overflow", "hidden"),
        ]);
        parent
            .append_child(&container)
            .map_err(|_| "failed to append HUD container".to_string())?;

        // ── Player health bar (bottom-left) ─────────────────────────
        let player_hp_group = create_div(document, "wrela-hp")?;
        set_styles(&player_hp_group, &[
            ("position", "absolute"),
            ("bottom", "60px"),
            ("left", "28px"),
            ("width", "220px"),
        ]);

        let player_hp_text = create_div(document, "wrela-hp-text")?;
        set_styles(&player_hp_text, &[
            ("font-size", "11px"),
            ("margin-bottom", "3px"),
            ("letter-spacing", "1px"),
            ("text-transform", "uppercase"),
            ("color", "#a89888"),
        ]);
        player_hp_text
            .set_inner_html("HP");

        let player_hp_track = create_bar_track(document, "wrela-hp-track")?;
        let player_hp_fill = create_bar_fill(document, "wrela-hp-fill", "#8b2020")?;
        player_hp_track
            .append_child(&player_hp_fill)
            .map_err(|_| "append hp fill".to_string())?;

        player_hp_group
            .append_child(&player_hp_text)
            .map_err(|_| "append hp text".to_string())?;
        player_hp_group
            .append_child(&player_hp_track)
            .map_err(|_| "append hp track".to_string())?;
        container
            .append_child(&player_hp_group)
            .map_err(|_| "append hp group".to_string())?;

        // ── Player stamina bar (below health) ───────────────────────
        let stamina_group = create_div(document, "wrela-stam")?;
        set_styles(&stamina_group, &[
            ("position", "absolute"),
            ("bottom", "38px"),
            ("left", "28px"),
            ("width", "180px"),
        ]);

        let stamina_label = create_div(document, "wrela-stam-label")?;
        set_styles(&stamina_label, &[
            ("font-size", "10px"),
            ("margin-bottom", "2px"),
            ("letter-spacing", "1px"),
            ("text-transform", "uppercase"),
            ("color", "#708878"),
        ]);
        stamina_label.set_inner_html("STAMINA");

        let stamina_track = create_bar_track(document, "wrela-stam-track")?;
        let stamina_fill = create_bar_fill(document, "wrela-stam-fill", "#2a7858")?;
        stamina_track
            .append_child(&stamina_fill)
            .map_err(|_| "append stam fill".to_string())?;

        stamina_group
            .append_child(&stamina_label)
            .map_err(|_| "append stam label".to_string())?;
        stamina_group
            .append_child(&stamina_track)
            .map_err(|_| "append stam track".to_string())?;
        container
            .append_child(&stamina_group)
            .map_err(|_| "append stam group".to_string())?;

        // ── Enemy health bar (top-center) ───────────────────────────
        let enemy_bar = create_div(document, "wrela-enemy")?;
        set_styles(&enemy_bar, &[
            ("position", "absolute"),
            ("top", "28px"),
            ("left", "50%"),
            ("transform", "translateX(-50%)"),
            ("width", "340px"),
            ("text-align", "center"),
        ]);

        let enemy_name = create_div(document, "wrela-enemy-name")?;
        set_styles(&enemy_name, &[
            ("font-size", "12px"),
            ("letter-spacing", "2px"),
            ("text-transform", "uppercase"),
            ("color", "#c85050"),
            ("margin-bottom", "4px"),
        ]);
        enemy_name.set_inner_html("ROT STALKER");

        let enemy_hp_text = create_div(document, "wrela-enemy-hp-text")?;
        set_styles(&enemy_hp_text, &[
            ("font-size", "10px"),
            ("color", "#a08080"),
            ("margin-bottom", "3px"),
        ]);

        let enemy_track = create_bar_track(document, "wrela-enemy-track")?;
        set_styles(&enemy_track, &[("height", "5px")]);
        let enemy_hp_fill = create_bar_fill(document, "wrela-enemy-fill", "#a03030")?;
        enemy_track
            .append_child(&enemy_hp_fill)
            .map_err(|_| "append enemy fill".to_string())?;

        enemy_bar
            .append_child(&enemy_name)
            .map_err(|_| "append enemy name".to_string())?;
        enemy_bar
            .append_child(&enemy_hp_text)
            .map_err(|_| "append enemy hp text".to_string())?;
        enemy_bar
            .append_child(&enemy_track)
            .map_err(|_| "append enemy track".to_string())?;
        container
            .append_child(&enemy_bar)
            .map_err(|_| "append enemy bar".to_string())?;

        // ── Resonance indicator (bottom-right) ──────────────────────
        let res_group = create_div(document, "wrela-res")?;
        set_styles(&res_group, &[
            ("position", "absolute"),
            ("bottom", "40px"),
            ("right", "28px"),
            ("text-align", "right"),
            ("width", "140px"),
        ]);

        let res_label = create_div(document, "wrela-res-label")?;
        set_styles(&res_label, &[
            ("font-size", "11px"),
            ("letter-spacing", "2px"),
            ("text-transform", "uppercase"),
            ("margin-bottom", "4px"),
        ]);

        let res_track = create_div(document, "wrela-res-track")?;
        set_styles(&res_track, &[
            ("width", "100%"),
            ("height", "3px"),
            ("background", "rgba(255,255,255,0.08)"),
            ("border-radius", "2px"),
            ("overflow", "hidden"),
            ("margin-bottom", "4px"),
        ]);

        let res_fill = create_div(document, "wrela-res-fill")?;
        set_styles(&res_fill, &[
            ("height", "100%"),
            ("width", "0%"),
            ("border-radius", "2px"),
            ("transition", "width 0.15s ease-out"),
        ]);
        res_track
            .append_child(&res_fill)
            .map_err(|_| "append res fill".to_string())?;

        let res_indicator = create_div(document, "wrela-res-ind")?;
        set_styles(&res_indicator, &[
            ("width", "8px"),
            ("height", "8px"),
            ("border-radius", "50%"),
            ("display", "inline-block"),
            ("margin-left", "6px"),
            ("vertical-align", "middle"),
        ]);

        res_group
            .append_child(&res_label)
            .map_err(|_| "append res label".to_string())?;
        res_group
            .append_child(&res_indicator)
            .map_err(|_| "append res indicator".to_string())?;
        res_group
            .append_child(&res_track)
            .map_err(|_| "append res track".to_string())?;
        container
            .append_child(&res_group)
            .map_err(|_| "append res group".to_string())?;

        // ── Kill counter (top-right) ────────────────────────────────
        let kill_text = create_div(document, "wrela-kills")?;
        set_styles(&kill_text, &[
            ("position", "absolute"),
            ("top", "28px"),
            ("right", "28px"),
            ("font-size", "14px"),
            ("letter-spacing", "1px"),
            ("color", "#c8b898"),
        ]);
        kill_text.set_inner_html("KILLS: 0");
        container
            .append_child(&kill_text)
            .map_err(|_| "append kills".to_string())?;

        // ── Death screen overlay ────────────────────────────────────
        let death_overlay = create_div(document, "wrela-death")?;
        set_styles(&death_overlay, &[
            ("position", "absolute"),
            ("top", "0"),
            ("left", "0"),
            ("width", "100%"),
            ("height", "100%"),
            ("display", "none"),
            ("flex-direction", "column"),
            ("align-items", "center"),
            ("justify-content", "center"),
            ("background", "rgba(0, 0, 0, 0.65)"),
            ("animation", "death-fade 1.5s ease-in forwards"),
        ]);

        let death_text = create_div(document, "wrela-death-text")?;
        set_styles(&death_text, &[
            ("font-size", "48px"),
            ("font-weight", "300"),
            ("letter-spacing", "12px"),
            ("color", "#8b2020"),
            ("text-transform", "uppercase"),
            ("text-shadow", "0 0 30px rgba(139,32,32,0.5)"),
        ]);
        death_text.set_inner_html("YOU DIED");

        let death_hint = create_div(document, "wrela-death-hint")?;
        set_styles(&death_hint, &[
            ("font-size", "14px"),
            ("margin-top", "24px"),
            ("color", "#887868"),
            ("letter-spacing", "2px"),
        ]);
        death_hint.set_inner_html("Press R to restart");

        death_overlay
            .append_child(&death_text)
            .map_err(|_| "append death text".to_string())?;
        death_overlay
            .append_child(&death_hint)
            .map_err(|_| "append death hint".to_string())?;
        container
            .append_child(&death_overlay)
            .map_err(|_| "append death overlay".to_string())?;

        // ── Controls help (bottom-center, fades after 5s) ───────────
        let controls_help = create_div(document, "wrela-controls")?;
        set_styles(&controls_help, &[
            ("position", "absolute"),
            ("bottom", "12px"),
            ("left", "50%"),
            ("transform", "translateX(-50%)"),
            ("font-size", "11px"),
            ("letter-spacing", "1px"),
            ("color", "rgba(168,152,136,0.8)"),
            ("white-space", "nowrap"),
            ("transition", "opacity 1.5s ease-out"),
            ("opacity", "1"),
        ]);
        controls_help.set_inner_html(
            "WASD: Move &nbsp;|&nbsp; J: Light &nbsp;|&nbsp; K: Heavy &nbsp;|&nbsp; Space: Dodge &nbsp;|&nbsp; L: Parry",
        );
        container
            .append_child(&controls_help)
            .map_err(|_| "append controls".to_string())?;

        Ok(Self {
            _container: container,
            player_hp_fill,
            player_hp_text,
            stamina_fill,
            enemy_bar,
            enemy_hp_fill,
            enemy_hp_text,
            res_label,
            res_indicator,
            res_fill,
            kill_text,
            death_overlay,
            controls_help,
            controls_visible: true,
            controls_start_ms: None,
        })
    }

    /// Update every HUD element.  Called once per frame.
    pub fn update(&mut self, state: &HudState, now_ms: f64) {
        // ── Controls fade-out after 5 seconds ───────────────────────
        if self.controls_visible {
            let start = *self.controls_start_ms.get_or_insert(now_ms);
            if now_ms - start > 5000.0 {
                let _ = self.controls_help.style().set_property("opacity", "0");
                self.controls_visible = false;
            }
        }

        // ── Player health ───────────────────────────────────────────
        let hp_pct = (state.player_health_ratio * 100.0).clamp(0.0, 100.0);
        let _ = self
            .player_hp_fill
            .style()
            .set_property("width", &format!("{hp_pct:.1}%"));

        // Color shifts as health drops
        let hp_color = if hp_pct > 50.0 {
            "#8b2020"
        } else if hp_pct > 25.0 {
            "#b03030"
        } else {
            "#d04040"
        };
        let _ = self
            .player_hp_fill
            .style()
            .set_property("background", hp_color);

        let hp_display = (state.player_health_ratio * 100.0).round() as i32;
        self.player_hp_text
            .set_text_content(Some(&format!("HP {hp_display}%")));

        // ── Stamina ─────────────────────────────────────────────────
        let stam_pct = (state.player_stamina_ratio * 100.0).clamp(0.0, 100.0);
        let _ = self
            .stamina_fill
            .style()
            .set_property("width", &format!("{stam_pct:.1}%"));
        // Dim stamina when low
        let stam_color = if stam_pct > 30.0 {
            "#2a7858"
        } else {
            "#1a5838"
        };
        let _ = self
            .stamina_fill
            .style()
            .set_property("background", stam_color);

        // ── Enemy health ────────────────────────────────────────────
        if state.enemy_alive {
            let _ = self.enemy_bar.style().set_property("display", "block");
            let ehp_pct = (state.enemy_health_ratio * 100.0).clamp(0.0, 100.0);
            let _ = self
                .enemy_hp_fill
                .style()
                .set_property("width", &format!("{ehp_pct:.1}%"));
            let ehp_display = (state.enemy_health_ratio * 100.0).round() as i32;
            self.enemy_hp_text
                .set_text_content(Some(&format!("{ehp_display}%")));
        } else {
            let _ = self.enemy_bar.style().set_property("display", "none");
        }

        // ── Resonance ───────────────────────────────────────────────
        let (tier_name, tier_color) = match state.resonance_tier {
            0 => ("SURVIVOR", "#888888"),
            1 => ("FLOW", "#4488cc"),
            2 => ("EDGE", "#9944cc"),
            3 => ("OVERDRIVE", "#cc8822"),
            _ => ("TRANSCENDENT", "#dda520"),
        };
        self.res_label.set_text_content(Some(tier_name));
        let _ = self.res_label.style().set_property("color", tier_color);

        // Resonance fill bar
        let res_pct = (state.resonance as f32 / 10.0).clamp(0.0, 100.0);
        let _ = self
            .res_fill
            .style()
            .set_property("width", &format!("{res_pct:.1}%"));
        let _ = self
            .res_fill
            .style()
            .set_property("background", tier_color);

        // Glow/pulse for higher tiers
        let animation = match state.resonance_tier {
            0 => "none",
            1 => "none",
            2 => "hud-pulse 2s ease-in-out infinite",
            3 => "hud-pulse 1.2s ease-in-out infinite",
            _ => "hud-pulse-fast 0.8s ease-in-out infinite",
        };
        let _ = self
            .res_indicator
            .style()
            .set_property("background", tier_color);
        let _ = self
            .res_indicator
            .style()
            .set_property("animation", animation);

        // ── Kill counter ────────────────────────────────────────────
        self.kill_text
            .set_text_content(Some(&format!("KILLS: {}", state.kills)));

        // ── Death screen ────────────────────────────────────────────
        if state.player_dead {
            let _ = self.death_overlay.style().set_property("display", "flex");
        } else {
            let _ = self.death_overlay.style().set_property("display", "none");
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// Helper utilities
// ────────────────────────────────────────────────────────────────────

fn inject_keyframes(document: &Document) -> Result<(), String> {
    // Only inject once
    if document.get_element_by_id("wrela-hud-styles").is_some() {
        return Ok(());
    }
    let style = document
        .create_element("style")
        .map_err(|_| "create style element".to_string())?;
    style
        .set_attribute("id", "wrela-hud-styles")
        .map_err(|_| "set style id".to_string())?;
    style.set_inner_html(GLOW_KEYFRAMES);
    let head = document
        .head()
        .ok_or_else(|| "document.head unavailable".to_string())?;
    head.append_child(&style)
        .map_err(|_| "append style to head".to_string())?;
    Ok(())
}

fn create_div(document: &Document, id: &str) -> Result<HtmlElement, String> {
    let el = document
        .create_element("div")
        .map_err(|_| format!("create_element failed for {id}"))?;
    el.set_attribute("id", id)
        .map_err(|_| format!("set_attribute id failed for {id}"))?;
    el.dyn_into::<HtmlElement>()
        .map_err(|_| format!("dyn_into HtmlElement failed for {id}"))
}

fn set_styles(el: &HtmlElement, styles: &[(&str, &str)]) {
    let style = el.style();
    for (prop, value) in styles {
        let _ = style.set_property(prop, value);
    }
}

fn create_bar_track(document: &Document, id: &str) -> Result<HtmlElement, String> {
    let track = create_div(document, id)?;
    set_styles(&track, &[
        ("width", "100%"),
        ("height", "4px"),
        ("background", "rgba(255,255,255,0.08)"),
        ("border-radius", "2px"),
        ("overflow", "hidden"),
    ]);
    Ok(track)
}

fn create_bar_fill(document: &Document, id: &str, color: &str) -> Result<HtmlElement, String> {
    let fill = create_div(document, id)?;
    set_styles(&fill, &[
        ("height", "100%"),
        ("width", "100%"),
        ("background", color),
        ("border-radius", "2px"),
        ("transition", "width 0.1s ease-out"),
    ]);
    Ok(fill)
}

#[cfg(test)]
mod tests {
    use super::HudState;

    #[test]
    fn hud_state_tier_names_cover_all_tiers() {
        for tier in 0..=4 {
            let state = HudState {
                player_health_ratio: 1.0,
                player_stamina_ratio: 1.0,
                enemy_health_ratio: 1.0,
                enemy_alive: true,
                resonance_tier: tier,
                resonance: tier * 250,
                kills: 0,
                player_dead: false,
            };
            let (name, _color) = match state.resonance_tier {
                0 => ("SURVIVOR", "#888888"),
                1 => ("FLOW", "#4488cc"),
                2 => ("EDGE", "#9944cc"),
                3 => ("OVERDRIVE", "#cc8822"),
                _ => ("TRANSCENDENT", "#dda520"),
            };
            assert!(!name.is_empty());
        }
    }
}
