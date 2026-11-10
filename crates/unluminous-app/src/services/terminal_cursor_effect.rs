//! Termoil's 20 cursor effects over Unluminous's terminals (`task-2601`).
//!
//! A cursor effect is a WGSL function, `termoil_cursor_effect(uv, color)`, run after the shader in the same pass,
//! so any cursor effect goes with any shader or with no shader. The files in `terminal_cursor_effects/` are copies
//! of Termoil's own, and [`compose`] is Termoil's `cursor_effects::compose`: the shader's `termoil_shade` is
//! renamed, and a new `termoil_shade` runs it and then the effect. A chosen effect replaces a shader's own cursor
//! light, because the shader reads `TERMOIL_SHADER_CURSOR`, which is 0 while an effect is chosen.
//!
//! What the terminals are drawn with is the `terminal.cursor_effect` setting, per thread like the shader, and the
//! appearance picker previews an effect without choosing it ([`preview`]). `services::terminal_shader` draws it.

/// One cursor effect that ships with Termoil.
#[derive(Debug)]
pub struct Effect {
    /// The name `terminal.cursor_effect` takes.
    pub name: &'static str,
    /// One line saying what it looks like.
    pub summary: &'static str,
    /// The WGSL source, defining `termoil_cursor_effect`.
    pub source: &'static str,
}

/// Declares a built in cursor effect from its file in `terminal_cursor_effects/`.
macro_rules! effect {
    ($name:literal, $summary:literal) => {
        Effect {
            name: $name,
            summary: $summary,
            source: include_str!(concat!("terminal_cursor_effects/", $name, ".wgsl")),
        }
    };
}

/// Every cursor effect, in the order Termoil lists them.
pub const EFFECTS: &[Effect] = &[
    effect!("glow", "A steady soft light around the cursor"),
    effect!("pulse", "A light around the cursor that swells and fades slowly"),
    effect!("neon-halo", "The pulsing magenta halo of neon-glow, with any shader"),
    effect!("sonar", "Thin rings travel outward from the cursor, like a radar ping"),
    effect!("spotlight", "The screen dims away from the cursor"),
    effect!("crosshair", "Faint lines through the cursor's row and column"),
    effect!("row", "A soft band of light across the cursor's row"),
    effect!("column", "A soft band of light down the cursor's column"),
    effect!("rainbow", "A halo that cycles through every color"),
    effect!("heartbeat", "A red glow that beats twice, rests, and beats again"),
    effect!("sparkle", "Small points of light twinkle around the cursor"),
    effect!("orbit", "Three small lights circle the cursor"),
    effect!("flame", "A warm flickering glow rising above the cursor"),
    effect!("frost", "An icy halo with a fine crystal texture"),
    effect!("magnify", "A small lens over the cursor shows the text under it larger"),
    effect!("ripple", "The picture around the cursor ripples like water"),
    effect!("beacon", "A beam turns around the cursor like a lighthouse"),
    effect!("starburst", "Thin rays of light shine out from the cursor and slowly turn"),
    effect!("invert", "A small disc around the cursor shows the colors inverted"),
    effect!("aurora", "A ribbon of green and violet light waves along the cursor's row"),
];

/// The helpers every cursor effect may use (`tce_shown`, `tce_offset`, `tce_read` and the rest).
const COMMON: &str = include_str!("terminal_cursor_effects/common.wgsl");

/// The shader used when a cursor effect is chosen with no shader: the frame unchanged.
const PLAIN: &str =
    "fn termoil_shade(uv: vec2<f32>, color: vec4<f32>) -> vec4<f32> {\n    return color;\n}\n";

/// The cursor effect with this name, ignoring case.
pub fn named(name: &str) -> Option<&'static Effect> {
    let name = name.trim();
    EFFECTS.iter().find(|effect| effect.name.eq_ignore_ascii_case(name))
}

/// True when a cursor effect moves on its own, so the window redraws while it is on.
pub fn moves(effect: &Effect) -> bool {
    effect
        .source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .any(|line| line.contains("termoil.time"))
}

/// The WGSL for a shader and a cursor effect, before the wrapper: Termoil's `compose`. With no effect it is the
/// shader with `TERMOIL_SHADER_CURSOR` set to 1. With one, the shader's `termoil_shade` is renamed and a new one
/// runs it and then the effect. The shader's text comes first, so its error lines keep their numbers.
///
/// @param shader - the shader's source, or none for the frame unchanged
/// @param cursor - the cursor effect, if one is chosen
pub fn compose(shader: Option<&str>, cursor: Option<&Effect>) -> String {
    let Some(cursor) = cursor else {
        return format!("{}\nconst TERMOIL_SHADER_CURSOR: f32 = 1.0;\n", shader.unwrap_or(PLAIN));
    };
    let base = shader.unwrap_or(PLAIN).replace("fn termoil_shade(", "fn termoil_base_shade(");
    format!(
        "{base}\nconst TERMOIL_SHADER_CURSOR: f32 = 0.0;\n{COMMON}\n{}\n\
         fn termoil_shade(uv: vec2<f32>, color: vec4<f32>) -> vec4<f32> {{\n    \
         return termoil_cursor_effect(uv, termoil_base_shade(uv, color));\n}}\n",
        cursor.source
    )
}

thread_local! {
    /// The cursor effect the terminals on this thread are drawn with, from `terminal.cursor_effect`. Empty is none.
    static CHOSEN: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// An effect drawn for now without being chosen, which the appearance picker does as its list is moved through.
    static PREVIEW: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Choose the cursor effect the terminals are drawn with, by name. An empty name, `none`, or a name that is not an
/// effect turns it off. Answers whether the name was an effect.
///
/// @param name - the effect's name
pub fn choose(name: &str) -> bool {
    let found = named(name);
    CHOSEN.with(|chosen| {
        *chosen.borrow_mut() = found.map(|effect| effect.name.to_owned()).unwrap_or_default()
    });
    found.is_some()
}

/// Draw the terminals with a cursor effect for now, or with none for `Some("")`, or put the chosen one back with
/// `None`.
///
/// @param name - the effect to preview
pub fn preview(name: Option<&str>) {
    PREVIEW.with(|preview| *preview.borrow_mut() = name.map(str::to_owned));
}

/// True while the appearance picker is previewing a cursor effect.
pub fn previewing() -> bool {
    PREVIEW.with(|preview| preview.borrow().is_some())
}

/// The cursor effect the terminals are drawn with just now: the one being previewed, or else the one chosen.
pub fn in_use() -> Option<&'static Effect> {
    let previewed = PREVIEW.with(|preview| preview.borrow().clone());
    match previewed {
        Some(name) => named(&name),
        None => CHOSEN.with(|chosen| named(&chosen.borrow())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_cursor_effects_with_unique_names_and_the_entry_function() {
        assert_eq!(EFFECTS.len(), 20);
        for (at, effect) in EFFECTS.iter().enumerate() {
            assert!(effect.source.contains("fn termoil_cursor_effect("), "{}", effect.name);
            assert!(!effect.source.contains("@group"), "{} declares a binding", effect.name);
            assert!(EFFECTS[at + 1..].iter().all(|other| other.name != effect.name));
        }
    }

    #[test]
    fn composing_renames_the_shader_and_turns_its_cursor_light_off() {
        let shader =
            "fn termoil_shade(uv: vec2<f32>, color: vec4<f32>) -> vec4<f32> { return color; }";
        let alone = compose(Some(shader), None);
        assert!(
            alone.contains("fn termoil_shade(")
                && alone.contains("TERMOIL_SHADER_CURSOR: f32 = 1.0")
        );
        let both = compose(Some(shader), named("glow"));
        assert!(both.starts_with("fn termoil_base_shade("), "{both}");
        assert!(both.contains("TERMOIL_SHADER_CURSOR: f32 = 0.0"));
        assert_eq!(both.matches("fn termoil_shade(").count(), 1);
        assert!(compose(None, named("orbit")).contains("fn termoil_base_shade("));
    }

    #[test]
    fn a_chosen_effect_is_in_use_and_a_preview_wins_until_it_is_put_back() {
        assert!(choose("Sonar"));
        assert_eq!(in_use().map(|effect| effect.name), Some("sonar"));
        preview(Some("orbit"));
        assert!(previewing());
        assert_eq!(in_use().map(|effect| effect.name), Some("orbit"));
        preview(Some(""));
        assert!(in_use().is_none(), "previewing none");
        preview(None);
        assert_eq!(in_use().map(|effect| effect.name), Some("sonar"));
        assert!(!choose("none"));
        assert!(in_use().is_none());
        assert!(moves(named("sonar").expect("shipped")));
        assert!(!moves(named("glow").expect("shipped")));
    }
}
