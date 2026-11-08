//! The pure design tokens, typed for the egui shell.
//!
//! The values mirror the design's token sheet: the palettes (the
//! OpenBitFun light and dark pair the app draws by default, the pure
//! shadow and dawn pair, and the Kimi pair), the type scale, the
//! spacing and radius scales, control and panel metrics, and the
//! shadow recipes. [`apply_choice`] installs a palette on the running
//! context and maps it onto egui's `Style`, so the shell renders the
//! same colors as the web client.

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};

use egui::Color32;
use serde::{Deserialize, Serialize};

/// The display family for brand moments and headings.
pub const FONT_DISPLAY: &str = "Schibsted Grotesk";

/// The body family for everything read as prose or UI text.
pub const FONT_BODY: &str = "Schibsted Grotesk";

/// The monospace family for code, identifiers and paths.
pub const FONT_MONO: &str = "JetBrains Mono";

/// Smallest text, badges and status pills.
pub const FS_2XS: f32 = 11.0;

/// Secondary small text, labels and hints.
pub const FS_XS: f32 = 12.0;

/// Small body text, sidebar rows and controls.
pub const FS_SM: f32 = 13.0;

/// The base body size of the whole app.
pub const FS_BASE: f32 = 14.0;

/// Emphasized body text and section titles.
pub const FS_LG: f32 = 16.0;

/// The relative line height of body text.
pub const LINE_HEIGHT: f32 = 1.5;

/// Tight tracking for brand marks, in em multiples of the font size.
pub const TRACKING_TIGHT: f32 = -0.03;

/// The wordmark's extra tight tracking, in em multiples.
pub const TRACKING_WORDMARK: f32 = -0.045;

/// Wide tracking for uppercase section labels, in em multiples.
pub const TRACKING_UPPER: f32 = 0.04;

/// 2px.
pub const SP_1: f32 = 2.0;

/// 4px.
pub const SP_2: f32 = 4.0;

/// 6px.
pub const SP_3: f32 = 6.0;

/// 8px.
pub const SP_4: f32 = 8.0;

/// 12px.
pub const SP_5: f32 = 12.0;

/// 16px.
pub const SP_6: f32 = 16.0;

/// 20px.
pub const SP_7: f32 = 20.0;

/// 24px.
pub const SP_8: f32 = 24.0;

/// 32px.
pub const SP_9: f32 = 32.0;

/// 48px.
pub const SP_10: f32 = 48.0;

/// 4px, inputs and small chips.
pub const R_XS: f32 = 4.0;

/// 6px, small controls.
pub const R_SM: f32 = 6.0;

/// 8px, the general control radius.
pub const R_MD: f32 = 8.0;

/// 12px, cards and inline panels.
pub const R_LG: f32 = 12.0;

/// 16px, shell cards, menus and the composer.
pub const R_XL: f32 = 16.0;

/// 20px, extra large surfaces.
pub const R_2XL: f32 = 20.0;

/// 28px, the dialog surface: the reference rounds its modal cards far
/// past a panel's radius.
pub const R_DIALOG: f32 = 28.0;

/// 16px, the composer card.
pub const R_COMPOSER: f32 = 16.0;

/// Fully round, pills and dots.
pub const R_FULL: f32 = 999.0;

/// Icon-only control, 26px square.
pub const CTL_ICO: f32 = 26.0;

/// Small control height, 32px.
pub const CTL_SM: f32 = 32.0;

/// The panel head height, 48px.
pub const PANEL_HEAD_H: f32 = 48.0;

/// The workbench tab height, 28px.
pub const TAB_H: f32 = 28.0;

/// The sidebar's open width, 270px.
pub const SIDE_W: f32 = 270.0;

/// The centered content column width, 768px.
pub const CONTENT_W: f32 = 768.0;

/// The welcome block's width, 728px: the wordmark, the composer and the
/// cards all measure to this, narrower than the content column because the
/// welcome block is a card-like stack rather than a transcript.
pub const WELCOME_W: f32 = 728.0;

/// The welcome block's side padding, 24px, outside the column: the
/// composer, the cards and the wordmark all measure to
/// [`WELCOME_W`] wide whatever the pane does.
pub const WELCOME_PAD_X: f32 = 24.0;

/// The welcome block's top padding, 40px.
pub const WELCOME_PAD_TOP: f32 = 40.0;

/// The share of the viewport the welcome block's bottom pad takes, 12%:
/// the design pads the bottom by 12vh against 40px at the top, which is
/// what lifts the stack above the middle of the pane.
pub const WELCOME_PAD_BOTTOM_SHARE: f32 = 0.12;

/// Fast transition, in seconds.
pub const T_FAST: f32 = 0.12;

/// Medium transition, in seconds.
pub const T_MED: f32 = 0.16;

/// Slow transition, in seconds.
pub const T_SLOW: f32 = 0.26;

/// The registered family that renders regular body text, the
/// proportional default.
pub const FAMILY_BODY_REGULAR: &str = "Schibsted Grotesk Regular";

/// The registered family that renders medium weight text, chip labels
/// and quiet emphasis.
pub const FAMILY_BODY_MEDIUM: &str = "Schibsted Grotesk Medium";

/// The registered family that renders semibold text, section labels
/// and row titles.
pub const FAMILY_BODY_SEMIBOLD: &str = "Schibsted Grotesk SemiBold";

/// The registered family that renders bold emphasis.
pub const FAMILY_BODY_BOLD: &str = "Schibsted Grotesk Bold";

/// The registered family that renders the welcome wordmark.
pub const FAMILY_BODY_EXTRABOLD: &str = "Schibsted Grotesk ExtraBold";

/// The registered mono families, one per weight, regular first.
pub const FAMILY_MONO_REGULAR: &str = "JetBrains Mono Regular";
/// Medium mono.
pub const FAMILY_MONO_MEDIUM: &str = "JetBrains Mono Medium";
/// Semibold mono.
pub const FAMILY_MONO_SEMIBOLD: &str = "JetBrains Mono SemiBold";
/// Bold mono.
pub const FAMILY_MONO_BOLD: &str = "JetBrains Mono Bold";
/// Extra bold mono.
pub const FAMILY_MONO_EXTRABOLD: &str = "JetBrains Mono ExtraBold";

/// The body face for prose: the regular body face with its line gap
/// set so a line is [`LINE`] times the font size, the reference's line
/// height. The markdown viewer renders under it; every other text keeps
/// the face's own metrics.
pub const FAMILY_PROSE: &str = "pure prose";
/// The semibold face for prose: bold runs and headings.
pub const FAMILY_PROSE_SEMIBOLD: &str = "pure prose semibold";
/// The mono face for prose, set to the same line height.
pub const FAMILY_PROSE_MONO: &str = "pure prose mono";

/// The reference's line height over the font size.
pub const LINE: f32 = 1.618;

/// The height of one line of text at `size`, snapped to a whole pixel
/// as the reference snaps every line.
#[must_use]
pub fn line_h(size: f32) -> f32 {
    snap(size * LINE)
}

/// Where to paint `galley` so its first line sits as the reference
/// centers a line in a box `box_h` tall from `top`: the line box and
/// the baseline in it each snapped to a whole pixel.
#[must_use]
pub fn line_y(top: f32, box_h: f32, galley: &egui::Galley) -> f32 {
    let size = galley
        .job
        .sections
        .first()
        .map_or(FS_SM, |section| section.format.font_id.size);
    let line = line_h(size);
    let line_top = snap(top + (box_h - line) / 2.0);
    galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first())
        .map_or(line_top, |glyph| {
            let lead = (line - glyph.font_height) / 2.0;
            line_top + snap(lead + glyph.font_face_ascent) - glyph.pos.y
        })
}

/// How far down to paint `galley`, laid out with a set line height,
/// so its glyphs sit where the reference centers them in each line:
/// egui sets them at a line's top and puts the extra below.
#[must_use]
pub fn half_lead(galley: &egui::Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first())
        .map_or(0.0, |glyph| {
            let lead = (glyph.line_height - glyph.font_height) / 2.0;
            snap(lead + glyph.font_face_ascent) - glyph.pos.y
        })
}

/// `color` as SVG attributes: `color_attr` names the hex color and
/// `opacity_attr` its alpha, as SVG keeps them apart.
#[must_use]
pub fn svg_color(color_attr: &str, opacity_attr: &str, color: Color32) -> String {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    format!(
        "{color_attr}=\"#{r:02x}{g:02x}{b:02x}\" {opacity_attr}=\"{:.3}\"",
        f32::from(a) / 255.0
    )
}

/// `value` snapped to a whole pixel the way the reference snaps text
/// lines: to the nearest, with a half rounding down.
#[must_use]
pub fn snap(value: f32) -> f32 {
    (value - 0.5).ceil()
}

/// The registered Inter families, the system-font stand-in behind the
/// design families.
pub const FAMILY_INTER_REGULAR: &str = "Inter Regular";
/// Medium Inter.
pub const FAMILY_INTER_MEDIUM: &str = "Inter Medium";
/// Semibold Inter.
pub const FAMILY_INTER_SEMIBOLD: &str = "Inter SemiBold";
/// Bold Inter.
pub const FAMILY_INTER_BOLD: &str = "Inter Bold";
/// Extra bold Inter.
pub const FAMILY_INTER_EXTRABOLD: &str = "Inter ExtraBold";

/// The regular body face, which the prose face is derived from too.
const BODY_REGULAR_TTF: &[u8] = include_bytes!("../assets/fonts/SchibstedGrotesk-Regular.ttf");
/// The semibold body face, which the prose semibold face is derived
/// from too.
const BODY_SEMIBOLD_TTF: &[u8] = include_bytes!("../assets/fonts/SchibstedGrotesk-SemiBold.ttf");
/// The regular mono face, which the prose mono face is derived from too.
const MONO_REGULAR_TTF: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

/// The bundled family files, static per-weight faces. The family each
/// face registers under is named for the face, and the chains live in
/// [`install_fonts`]. Static faces rather than one variable TTF because
/// the shell picks a family per weight (egui cannot select a variable
/// axis per text style either).
const FONT_FACES: &[(&str, &[u8])] = &[
    (FAMILY_BODY_REGULAR, BODY_REGULAR_TTF),
    (
        FAMILY_BODY_MEDIUM,
        include_bytes!("../assets/fonts/SchibstedGrotesk-Medium.ttf"),
    ),
    (FAMILY_BODY_SEMIBOLD, BODY_SEMIBOLD_TTF),
    (
        FAMILY_BODY_BOLD,
        include_bytes!("../assets/fonts/SchibstedGrotesk-Bold.ttf"),
    ),
    (
        FAMILY_BODY_EXTRABOLD,
        include_bytes!("../assets/fonts/SchibstedGrotesk-ExtraBold.ttf"),
    ),
    (
        FAMILY_INTER_REGULAR,
        include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    ),
    (
        FAMILY_INTER_MEDIUM,
        include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    ),
    (
        FAMILY_INTER_SEMIBOLD,
        include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    ),
    (
        FAMILY_INTER_BOLD,
        include_bytes!("../assets/fonts/Inter-Bold.ttf"),
    ),
    (
        FAMILY_INTER_EXTRABOLD,
        include_bytes!("../assets/fonts/Inter-ExtraBold.ttf"),
    ),
    (FAMILY_MONO_REGULAR, MONO_REGULAR_TTF),
    (
        FAMILY_MONO_MEDIUM,
        include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
    ),
    (
        FAMILY_MONO_SEMIBOLD,
        include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf"),
    ),
    (
        FAMILY_MONO_BOLD,
        include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    ),
    (
        FAMILY_MONO_EXTRABOLD,
        include_bytes!("../assets/fonts/JetBrainsMono-ExtraBold.ttf"),
    ),
];

/// Registers the three bundled families with egui, each chain falling
/// back through the other weights, then Inter, then egui's own fonts
/// for symbol coverage. Call once at startup, before the first frame.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in FONT_FACES {
        fonts.font_data.insert(
            (*name).to_owned(),
            Arc::new(egui::FontData::from_static(bytes)),
        );
    }
    for (name, bytes) in [
        (FAMILY_PROSE, BODY_REGULAR_TTF),
        (FAMILY_PROSE_SEMIBOLD, BODY_SEMIBOLD_TTF),
        (FAMILY_PROSE_MONO, MONO_REGULAR_TTF),
    ] {
        if let Some(patched) = with_line_height(bytes, LINE) {
            fonts.font_data.insert(
                name.to_owned(),
                Arc::new(egui::FontData::from_owned(patched)),
            );
        }
    }
    chain(
        &mut fonts,
        family(FAMILY_PROSE),
        &[FAMILY_PROSE, FAMILY_BODY_REGULAR, FAMILY_INTER_REGULAR],
    );
    chain(
        &mut fonts,
        family(FAMILY_PROSE_SEMIBOLD),
        &[
            FAMILY_PROSE_SEMIBOLD,
            FAMILY_BODY_SEMIBOLD,
            FAMILY_INTER_SEMIBOLD,
        ],
    );
    chain(
        &mut fonts,
        family(FAMILY_PROSE_MONO),
        &[FAMILY_PROSE_MONO, FAMILY_MONO_REGULAR, FAMILY_INTER_REGULAR],
    );
    // The body face is the proportional default; the other weights and
    // Inter follow as glyph fallbacks, egui's fonts last of all.
    chain(
        &mut fonts,
        egui::FontFamily::Proportional,
        &[FAMILY_BODY_REGULAR, FAMILY_INTER_REGULAR],
    );
    chain(
        &mut fonts,
        egui::FontFamily::Monospace,
        &[FAMILY_MONO_REGULAR, FAMILY_INTER_REGULAR],
    );
    chain(
        &mut fonts,
        family(FAMILY_BODY_REGULAR),
        &[FAMILY_BODY_REGULAR, FAMILY_INTER_REGULAR],
    );
    chain(
        &mut fonts,
        family(FAMILY_MONO_REGULAR),
        &[FAMILY_MONO_REGULAR, FAMILY_INTER_REGULAR],
    );
    chain(
        &mut fonts,
        family(FAMILY_BODY_MEDIUM),
        &[
            FAMILY_BODY_MEDIUM,
            FAMILY_BODY_REGULAR,
            FAMILY_INTER_MEDIUM,
            FAMILY_INTER_REGULAR,
        ],
    );
    chain(
        &mut fonts,
        family(FAMILY_BODY_SEMIBOLD),
        &[
            FAMILY_BODY_SEMIBOLD,
            FAMILY_BODY_BOLD,
            FAMILY_BODY_REGULAR,
            FAMILY_INTER_SEMIBOLD,
            FAMILY_INTER_REGULAR,
        ],
    );
    chain(
        &mut fonts,
        family(FAMILY_BODY_BOLD),
        &[
            FAMILY_BODY_BOLD,
            FAMILY_BODY_SEMIBOLD,
            FAMILY_BODY_REGULAR,
            FAMILY_INTER_BOLD,
            FAMILY_INTER_REGULAR,
        ],
    );
    chain(
        &mut fonts,
        family(FAMILY_BODY_EXTRABOLD),
        &[
            FAMILY_BODY_EXTRABOLD,
            FAMILY_BODY_BOLD,
            FAMILY_BODY_REGULAR,
            FAMILY_INTER_EXTRABOLD,
            FAMILY_INTER_REGULAR,
        ],
    );
    for name in [
        FAMILY_MONO_MEDIUM,
        FAMILY_MONO_SEMIBOLD,
        FAMILY_MONO_BOLD,
        FAMILY_MONO_EXTRABOLD,
    ] {
        chain(
            &mut fonts,
            family(name),
            &[name, FAMILY_MONO_REGULAR, FAMILY_INTER_REGULAR],
        );
    }
    // The CJK fallback lands behind every family, so a glyph a bundled
    // face owns still renders from that face. The system has no CJK
    // font only when the platform itself lacks one.
    crate::fonts::install_cjk_fallback(&mut fonts);
    // Every named family carries only the faces chained above, so
    // egui's bundled emoji stay out of them and a streamed ✅ lands on
    // the replacement glyph. The emoji go last everywhere: a text face
    // that owns the codepoint still renders it.
    for faces in fonts.families.values_mut() {
        for name in ["NotoEmoji-Regular", "emoji-icon-font"] {
            if !faces.iter().any(|face| face == name) {
                faces.push(name.to_owned());
            }
        }
    }
    ctx.set_fonts(fonts);
}

/// `font` with its ascender and descender grown so a line is `ratio`
/// times the font size, the extra room split evenly above and below
/// the glyphs the way the reference centers them. Growing the extents
/// rather than the line gap puts the half-leading in the baseline, so
/// it rounds to a pixel with the glyph instead of shifting it by a
/// fraction. `None` when the bytes are not a font with the tables the
/// metrics come from.
///
/// Both the hhea and the OS/2 typographic metrics are set, since the
/// shaper reads whichever the font's flags select.
fn with_line_height(font: &[u8], ratio: f32) -> Option<Vec<u8>> {
    let u16_at = |at: usize| Some(u16::from_be_bytes([*font.get(at)?, *font.get(at + 1)?]));
    let i16_at = |at: usize| u16_at(at).map(|v| v as i16);
    let table = |tag: &[u8; 4]| {
        (0..usize::from(u16_at(4)?)).find_map(|n| {
            let record = 12 + n * 16;
            (font.get(record..record + 4)? == tag).then(|| {
                let at = font.get(record + 8..record + 12)?;
                Some(u32::from_be_bytes([at[0], at[1], at[2], at[3]]) as usize)
            })?
        })
    };
    let head = table(b"head")?;
    let hhea = table(b"hhea")?;
    let os2 = table(b"OS/2");
    let em = f32::from(u16_at(head + 18)?);
    let mut out = font.to_vec();
    let mut grow = |at: usize| -> Option<()> {
        let (ascent, descent) = (i16_at(at)?, i16_at(at + 2)?);
        let extra = (ratio * em - f32::from(ascent) + f32::from(descent)).max(0.0);
        let above = (extra / 2.0).round();
        let ascent = (f32::from(ascent) + above) as i16;
        let descent = (f32::from(descent) - (extra - above)) as i16;
        out[at..at + 2].copy_from_slice(&ascent.to_be_bytes());
        out[at + 2..at + 4].copy_from_slice(&descent.to_be_bytes());
        out[at + 4..at + 6].copy_from_slice(&0i16.to_be_bytes());
        Some(())
    };
    grow(hhea + 4)?;
    if let Some(os2) = os2 {
        grow(os2 + 68)?;
    }
    Some(out)
}

/// The prose face's family when the fonts are installed, the
/// proportional default otherwise, for contexts that never installed
/// them.
#[must_use]
pub fn prose_family(ctx: &egui::Context) -> egui::FontFamily {
    installed_or(ctx, FAMILY_PROSE, egui::FontFamily::Proportional)
}

/// The semibold prose face's family when the fonts are installed, the
/// body semibold otherwise.
#[must_use]
pub fn prose_semibold_family(ctx: &egui::Context) -> egui::FontFamily {
    installed_or(ctx, FAMILY_PROSE_SEMIBOLD, family(FAMILY_BODY_SEMIBOLD))
}

/// The prose mono face's family when the fonts are installed, the
/// regular mono otherwise.
#[must_use]
pub fn prose_mono_family(ctx: &egui::Context) -> egui::FontFamily {
    installed_or(ctx, FAMILY_PROSE_MONO, family(FAMILY_MONO_REGULAR))
}

fn installed_or(ctx: &egui::Context, name: &str, fallback: egui::FontFamily) -> egui::FontFamily {
    let wanted = family(name);
    if ctx.fonts(|fonts| fonts.definitions().families.contains_key(&wanted)) {
        wanted
    } else {
        fallback
    }
}

/// The named family a registered face renders under.
#[must_use]
pub fn family(name: &str) -> egui::FontFamily {
    egui::FontFamily::Name(name.into())
}

/// Puts `faces` at the front of `family`'s chain, egui's own fonts
/// staying behind them.
fn chain(fonts: &mut egui::FontDefinitions, family: egui::FontFamily, faces: &[&str]) {
    let entry = fonts.families.entry(family).or_default();
    for name in faces.iter().rev() {
        entry.insert(0, (*name).to_owned());
    }
}

/// Which of the palette pair a theme draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The dark palette family.
    Dark,
    /// The light palette family.
    Light,
}

impl Mode {
    /// True for [`Mode::Dark`].
    #[must_use]
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

/// One agent avatar role: a soft fill behind, a palette hue in front.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AvatarColor {
    /// The chip fill, the theme's soft fill.
    pub bg: Color32,
    /// The glyph color, one of the palette hues.
    pub fg: Color32,
}

/// The eight agent avatar hues of the design.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Avatars {
    /// Warm yellow avatar hue.
    pub amber: AvatarColor,
    /// Red avatar hue.
    pub rose: AvatarColor,
    /// Orange avatar hue, the accent family.
    pub orange: AvatarColor,
    /// Teal green avatar hue.
    pub emerald: AvatarColor,
    /// Teal cyan avatar hue.
    pub cyan: AvatarColor,
    /// Blue avatar hue, the info family.
    pub sky: AvatarColor,
    /// Violet avatar hue, the swarm family.
    pub violet: AvatarColor,
    /// Pink avatar hue.
    pub pink: AvatarColor,
}

/// Every color token of one theme, named after the design's roles.
#[derive(Clone, Debug)]
pub struct Palette {
    /// App background.
    pub bg: Color32,
    /// Sidebar and workbench background.
    pub sidebar: Color32,
    /// Cards, panels and inputs.
    pub surface: Color32,
    /// Raised elements above `surface`.
    pub raised: Color32,
    /// Pressed-in areas.
    pub sunken: Color32,
    /// The deepest level, used behind on-color glyphs.
    pub deep: Color32,
    /// Wells that read as inset surfaces.
    pub well: Color32,
    /// Barely-there fill for chips and pills.
    pub fill: Color32,
    /// `fill` one step up, for hover.
    pub fill_hover: Color32,
    /// Primary text.
    pub ink: Color32,
    /// Emphasized text.
    pub ink_strong: Color32,
    /// Secondary text.
    pub muted: Color32,
    /// Tertiary text.
    pub faint: Color32,
    /// Quaternary text and the faintest marks.
    pub ghost: Color32,
    /// Hairline borders.
    pub line: Color32,
    /// The faintest fills, below `line`.
    pub subtle: Color32,
    /// Stronger borders, hovered states.
    pub line_strong: Color32,
    /// Row and control hover fill.
    pub hover: Color32,
    /// Selected row fill.
    pub selected: Color32,
    /// Selected row fill on hover.
    pub selected_hover: Color32,
    /// The warm lantern accent.
    pub accent: Color32,
    /// Accent hover step.
    pub accent_hover: Color32,
    /// Accent tint background.
    pub accent_soft: Color32,
    /// Accent border.
    pub accent_bd: Color32,
    /// Solid primary action fill.
    pub primary: Color32,
    /// Primary action hover fill.
    pub primary_hover: Color32,
    /// Text and glyphs on the primary fill.
    pub primary_ink: Color32,
    /// Success green.
    pub ok: Color32,
    /// Success tint background.
    pub ok_soft: Color32,
    /// Success border.
    pub ok_bd: Color32,
    /// Text on a solid success fill.
    pub ok_ink: Color32,
    /// Warning yellow.
    pub warn: Color32,
    /// Warning tint background.
    pub warn_soft: Color32,
    /// Warning border.
    pub warn_bd: Color32,
    /// Danger red.
    pub danger: Color32,
    /// Danger tint background.
    pub danger_soft: Color32,
    /// Danger border.
    pub danger_bd: Color32,
    /// Swarm violet.
    pub done: Color32,
    /// Violet tint background.
    pub done_soft: Color32,
    /// Violet border.
    pub done_bd: Color32,
    /// Informational blue.
    pub info: Color32,
    /// The composer's background.
    pub composer_bg: Color32,
    /// The composer's idle border.
    pub composer_line: Color32,
    /// The composer's focused border.
    pub composer_focus_line: Color32,
    /// The send button background.
    pub send_bg: Color32,
    /// The send button hover background.
    pub send_bg_hover: Color32,
    /// The glyph on the send button.
    pub send_icon: Color32,
    /// The send button when disabled.
    pub send_bg_off: Color32,
    /// The glyph on the disabled send button.
    pub send_icon_off: Color32,
    /// The stop glyph, a softened danger red.
    pub stop_glyph: Color32,
    /// Diff added lines.
    pub diff_add: Color32,
    /// Diff added line background.
    pub diff_add_bg: Color32,
    /// Diff deleted lines.
    pub diff_del: Color32,
    /// Diff deleted line background.
    pub diff_del_bg: Color32,
    /// The user bubble fill.
    pub bubble: Color32,
    /// Text selection fill.
    pub selection: Color32,
    /// Menus and popovers, slightly translucent.
    pub menu: Color32,
    /// Inline code fill.
    pub code_inline: Color32,
    /// The agent avatar hues.
    pub avatars: Avatars,
    /// Whether the welcome pane paints the brand's lantern glow. The
    /// flat OpenBitFun pair leaves it out; the pure pair keeps it.
    pub glow: bool,
    /// Small ambient shadow.
    pub shadow_1: Vec<ShadowLayer>,
    /// Large flyout shadow.
    pub shadow_2: Vec<ShadowLayer>,
    /// Subtle input shadow.
    pub shadow_input: Vec<ShadowLayer>,
    /// The send button's layered shadow.
    pub shadow_send: Vec<ShadowLayer>,
    /// Menu shadow.
    pub shadow_menu: Vec<ShadowLayer>,
}

/// One layer of a box shadow as the reference draws it: a Gaussian
/// under the box moved by `offset` and grown by `spread`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLayer {
    /// How far the shadow sits from the box.
    pub offset: egui::Vec2,
    /// The Gaussian's sigma.
    pub blur: f32,
    /// How much the shadow grows past the box, or shrinks when negative.
    pub spread: f32,
    /// The shadow's color at full strength.
    pub color: Color32,
}

impl ShadowLayer {
    /// The nearest egui shadow, for the popups egui frames itself.
    #[must_use]
    pub fn egui(self) -> egui::Shadow {
        egui::Shadow {
            offset: [self.offset.x as i8, self.offset.y as i8],
            blur: self.blur as u8,
            spread: self.spread.max(0.0) as u8,
            color: self.color,
        }
    }
}

/// Converts one hex color to a [`Color32`].
///
/// A 6-digit literal is opaque; an 8-digit one carries alpha in its last
/// byte, as the design's own tokens do (`0xF2A65A42` is the accent at 26
/// percent). Reading a 6-digit value as four bytes would take its blue
/// as alpha and draw every surface at that opacity: `0x0F0E13` became 7
/// percent, which is what shifted the whole shell off the palette.
#[must_use]
fn color(hex: u32) -> Color32 {
    match hex.checked_shr(24) {
        Some(0) | None => Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8),
        Some(_) => Color32::from_rgba_unmultiplied(
            (hex >> 24) as u8,
            (hex >> 16) as u8,
            (hex >> 8) as u8,
            hex as u8,
        ),
    }
}

/// Black at `alpha`. [`color`] cannot carry it: an 8-digit black
/// literal such as `0x000000E6` has no top byte and reads as the opaque
/// blue `#0000E6`.
#[must_use]
fn black(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(0, 0, 0, alpha)
}

/// Builds one shadow layer from the design's shadow recipe. Shadow
/// colors always carry alpha, so `hex` is read as all four bytes. A
/// pure black shadow such as the design's 55 percent `0x0000008C` has
/// no top byte for [`color`] to see, so those take the [`black`] path.
#[must_use]
fn layer(hex: u32, x: f32, y: f32, blur: f32, spread: f32) -> ShadowLayer {
    let color = if hex <= 0xFF {
        black(hex as u8)
    } else {
        color(hex)
    };
    ShadowLayer {
        offset: egui::vec2(x, y),
        blur,
        spread,
        color,
    }
}

/// The palette the running app draws with per egui slot, set by
/// [`apply_choice`]: index 0 is dark, index 1 light.
static ACTIVE: RwLock<[Option<&'static Palette>; 2]> = RwLock::new([None, None]);

impl Palette {
    /// The palette the running app draws with: the theme last applied
    /// to `mode`'s slot, or before any the pure theme of `mode`.
    #[must_use]
    pub fn active(mode: Mode) -> &'static Self {
        let index = match mode {
            Mode::Dark => 0,
            Mode::Light => 1,
        };
        if let Some(palette) = ACTIVE.read().unwrap_or_else(PoisonError::into_inner)[index] {
            return palette;
        }
        let pure = if mode.is_dark() {
            "pure-shadow"
        } else {
            "pure-dawn"
        };
        crate::themes::builtin(pure).map_or_else(|| unreachable!(), |(palette, _)| palette)
    }

    /// The palette of the context's resolved theme, which is what a
    /// frame draws with under a System choice.
    #[must_use]
    pub fn active_of(ctx: &egui::Context) -> &'static Self {
        Self::active(match ctx.theme() {
            egui::Theme::Dark => Mode::Dark,
            egui::Theme::Light => Mode::Light,
        })
    }

    /// The dark palette: ink surfaces with a violet undertone, one
    /// warm lantern accent, violet reserved for swarms.
    pub fn shadow() -> Self {
        Self {
            bg: color(0x0F0E13),
            sidebar: color(0x0B0A0E),
            surface: color(0x17151D),
            raised: color(0x221F2A),
            sunken: color(0x0B0A0E),
            deep: color(0x09080C),
            well: color(0x17151D),
            fill: color(0xE8E2F50D),
            fill_hover: color(0xE8E2F516),
            ink: color(0xECE8F6DB),
            ink_strong: color(0xF4F1FA),
            muted: color(0xECE8F694),
            faint: color(0xECE8F66B),
            ghost: color(0xECE8F640),
            line: color(0xE8E2F51C),
            subtle: color(0xE8E2F50D),
            line_strong: color(0xE8E2F530),
            hover: color(0xE8E2F50D),
            selected: color(0xE8E2F516),
            selected_hover: color(0xE8E2F51F),
            accent: color(0xF2A65A),
            accent_hover: color(0xF6B674),
            accent_soft: color(0xF2A65A1F),
            accent_bd: color(0xF2A65A57),
            primary: color(0xF2A65A),
            primary_hover: color(0xF6B674),
            primary_ink: color(0xFFFFFF),
            ok: color(0x8BD49C),
            ok_soft: color(0x8BD49C1C),
            ok_bd: color(0x8BD49C4D),
            ok_ink: color(0x0B1A10),
            warn: color(0xE8C96A),
            warn_soft: color(0xE8C96A1C),
            warn_bd: color(0xE8C96A4D),
            danger: color(0xF2727F),
            danger_soft: color(0xF2727F1C),
            danger_bd: color(0xF2727F4D),
            done: color(0xA98BFA),
            done_soft: color(0xA98BFA1F),
            done_bd: color(0xA98BFA57),
            info: color(0x9AB4FF),
            composer_bg: color(0x15131B),
            composer_line: color(0xE8E2F51C),
            composer_focus_line: color(0xF2A65A6B),
            send_bg: color(0xF2A65A),
            send_bg_hover: color(0xF6B674),
            send_icon: color(0x1A1208),
            send_bg_off: color(0xE8E2F514),
            send_icon_off: color(0xE8E2F542),
            stop_glyph: color(0xF2727FBF),
            diff_add: color(0x8BD49C),
            diff_add_bg: color(0x8BD49C1F),
            diff_del: color(0xF2727F),
            diff_del_bg: color(0xF2727F1F),
            bubble: color(0x221F2A),
            selection: color(0xF2A65A42),
            menu: color(0x16141CF2),
            code_inline: color(0xE8E2F514),
            avatars: Avatars {
                amber: AvatarColor {
                    bg: color(0xE8E2F50D),
                    fg: color(0xE8C96A),
                },
                rose: AvatarColor {
                    bg: color(0xE8E2F50D),
                    fg: color(0xF2727F),
                },
                orange: AvatarColor {
                    bg: color(0xE8E2F50D),
                    fg: color(0xF2A65A),
                },
                emerald: AvatarColor {
                    bg: color(0xE8E2F50D),
                    fg: color(0x5FC9B0),
                },
                cyan: AvatarColor {
                    bg: color(0xE8E2F50D),
                    fg: color(0x7DD3D0),
                },
                sky: AvatarColor {
                    bg: color(0xE8E2F50D),
                    fg: color(0x9AB4FF),
                },
                violet: AvatarColor {
                    bg: color(0xE8E2F50D),
                    fg: color(0xA98BFA),
                },
                pink: AvatarColor {
                    bg: color(0xE8E2F50D),
                    fg: color(0xF29AC1),
                },
            },
            glow: true,
            shadow_1: vec![layer(0x00000066, 0., 1., 2., 0.)],
            shadow_2: vec![layer(0x0000008C, 0., 16., 40., 0.)],
            shadow_input: vec![layer(0x00000012, 0., 5., 16., -4.)],
            shadow_send: vec![
                layer(0x00000061, 0., 7., 16., -13.),
                layer(0x00000012, 0., 1., 2., 0.),
            ],
            shadow_menu: vec![
                layer(0x00000099, 0., 16., 48., 0.),
                layer(0x00000066, 0., 2., 8., 0.),
            ],
        }
    }

    /// The light palette: ink on violet-tinted paper, mapped onto the
    /// same roles as the dark palette.
    pub fn dawn() -> Self {
        Self {
            bg: color(0xF7F5F9),
            sidebar: color(0xEEEBF2),
            surface: color(0xEFECF3),
            raised: color(0xE4DFEB),
            sunken: color(0xEEEBF2),
            deep: color(0xFFFFFF),
            well: color(0xEFECF3),
            fill: color(0x1A142E0B),
            fill_hover: color(0x1A142E12),
            ink: color(0x1A142EED),
            ink_strong: color(0x15121C),
            muted: color(0x1A142EB8),
            faint: color(0x1A142E7D),
            ghost: color(0x1A142E4D),
            line: color(0x1A142E1F),
            subtle: color(0x1A142E0D),
            line_strong: color(0x1A142E33),
            hover: color(0x1A142E0B),
            selected: color(0x1A142E12),
            selected_hover: color(0x1A142E1A),
            accent: color(0xA8580F),
            accent_hover: color(0x924E11),
            accent_soft: color(0xA8580F1A),
            accent_bd: color(0xA8580F4D),
            primary: color(0xA8580F),
            primary_hover: color(0x924E11),
            primary_ink: color(0xFFFFFF),
            ok: color(0x2B7443),
            ok_soft: color(0x2B74431A),
            ok_bd: color(0x2B74434D),
            ok_ink: color(0x0B1A10),
            warn: color(0x836409),
            warn_soft: color(0x8364091A),
            warn_bd: color(0x8364094D),
            danger: color(0xBF3649),
            danger_soft: color(0xBF36491A),
            danger_bd: color(0xBF36494D),
            done: color(0x6A47CC),
            done_soft: color(0x6A47CC1A),
            done_bd: color(0x6A47CC4D),
            info: color(0x3456C2),
            composer_bg: color(0xFFFFFF),
            composer_line: color(0x1A142E1F),
            composer_focus_line: color(0xA8580F73),
            send_bg: color(0xA8580F),
            send_bg_hover: color(0x924E11),
            send_icon: color(0xFFFFFF),
            send_bg_off: color(0x1A142E14),
            send_icon_off: color(0x1A142E4D),
            stop_glyph: color(0xBF3649BF),
            diff_add: color(0x2B7443),
            diff_add_bg: color(0x2B74431F),
            diff_del: color(0xBF3649),
            diff_del_bg: color(0xBF36491F),
            bubble: color(0xE4DFEB),
            selection: color(0xA8580F33),
            menu: color(0xFFFFFFF2),
            code_inline: color(0x1A142E0F),
            avatars: Avatars {
                amber: AvatarColor {
                    bg: color(0x1A142E0B),
                    fg: color(0x836409),
                },
                rose: AvatarColor {
                    bg: color(0x1A142E0B),
                    fg: color(0xBF3649),
                },
                orange: AvatarColor {
                    bg: color(0x1A142E0B),
                    fg: color(0xA8580F),
                },
                emerald: AvatarColor {
                    bg: color(0x1A142E0B),
                    fg: color(0x1C7563),
                },
                cyan: AvatarColor {
                    bg: color(0x1A142E0B),
                    fg: color(0x157A84),
                },
                sky: AvatarColor {
                    bg: color(0x1A142E0B),
                    fg: color(0x3456C2),
                },
                violet: AvatarColor {
                    bg: color(0x1A142E0B),
                    fg: color(0x6A47CC),
                },
                pink: AvatarColor {
                    bg: color(0x1A142E0B),
                    fg: color(0xAC3A7A),
                },
            },
            glow: true,
            shadow_1: vec![layer(0x1A142E1A, 0., 1., 2., 0.)],
            shadow_2: vec![layer(0x1A142E29, 0., 12., 32., 0.)],
            shadow_input: vec![layer(0x1A142E14, 0., 5., 16., -4.)],
            shadow_send: vec![
                layer(0x1A142E4D, 0., 7., 16., -13.),
                layer(0x1A142E12, 0., 1., 2., 0.),
            ],
            shadow_menu: vec![
                layer(0x1A142E2E, 0., 12., 40., 0.),
                layer(0x1A142E14, 0., 2., 8., 0.),
            ],
        }
    }

    /// OpenBitFun's light desktop palette: near-white paper, a pale
    /// chrome, one cyan accent and a navy primary action. Neutral
    /// hairlines carry the structure instead of tinted fills.
    pub fn bitfun_light() -> Self {
        Self {
            bg: color(0xFFFFFF),
            sidebar: color(0xF8F8F9),
            surface: color(0xFFFFFF),
            raised: color(0xFFFFFF),
            sunken: color(0xF7F7F7),
            deep: color(0xFFFFFF),
            well: color(0xF7F7F7),
            fill: black(0x0000000D),
            fill_hover: black(0x00000014),
            ink: black(0x000000CC),
            ink_strong: color(0x1C1C1F),
            muted: color(0x6A6A6A),
            faint: black(0x00000073),
            ghost: black(0x0000004C),
            line: color(0x101A2714),
            subtle: color(0x101A2708),
            line_strong: color(0x101A2726),
            hover: black(0x0000000D),
            selected: black(0x00000014),
            selected_hover: black(0x0000001A),
            accent: color(0x059CB0),
            accent_hover: color(0x07879A),
            accent_soft: color(0x059CB017),
            accent_bd: color(0x059CB066),
            primary: color(0x101A27),
            primary_hover: color(0x1C1C1F),
            primary_ink: color(0xFFFFFF),
            ok: color(0x12722A),
            ok_soft: color(0x1AA73E1A),
            ok_bd: color(0x1AA73E4C),
            ok_ink: color(0xFFFFFF),
            warn: color(0x8A4C00),
            warn_soft: color(0xFF8C001A),
            warn_bd: color(0xFF8C004C),
            danger: color(0xBD1B19),
            danger_soft: color(0xEC221F1A),
            danger_bd: color(0xEC221F4C),
            done: color(0x7C6B99),
            done_soft: color(0x7C6B9917),
            done_bd: color(0x7C6B9940),
            info: color(0x2360C2),
            composer_bg: color(0xFFFFFF),
            composer_line: black(0x00000014),
            composer_focus_line: color(0x858585),
            send_bg: color(0x101A27),
            send_bg_hover: color(0x1C1C1F),
            send_icon: color(0xFFFFFF),
            send_bg_off: black(0x0000000D),
            send_icon_off: black(0x0000004C),
            stop_glyph: color(0xEC221F),
            diff_add: color(0x1AA73E),
            diff_add_bg: color(0x1AA73E1A),
            diff_del: color(0xEC221F),
            diff_del_bg: color(0xEC221F1A),
            bubble: color(0xF3F3F5),
            selection: color(0x059CB033),
            menu: color(0xFFFFFFF7),
            code_inline: black(0x0000000D),
            avatars: Avatars {
                amber: AvatarColor {
                    bg: black(0x00000008),
                    fg: color(0xD5A910),
                },
                rose: AvatarColor {
                    bg: black(0x00000008),
                    fg: color(0xC95765),
                },
                orange: AvatarColor {
                    bg: black(0x00000008),
                    fg: color(0xD47628),
                },
                emerald: AvatarColor {
                    bg: black(0x00000008),
                    fg: color(0x269F70),
                },
                cyan: AvatarColor {
                    bg: black(0x00000008),
                    fg: color(0x07879A),
                },
                sky: AvatarColor {
                    bg: black(0x00000008),
                    fg: color(0x2563EB),
                },
                violet: AvatarColor {
                    bg: black(0x00000008),
                    fg: color(0x7B43F8),
                },
                pink: AvatarColor {
                    bg: black(0x00000008),
                    fg: color(0xDB2777),
                },
            },
            glow: false,
            shadow_1: vec![layer(0x0000000F, 0., 1., 2., 0.)],
            shadow_2: vec![layer(0x0000001F, 0., 4., 20., 0.)],
            shadow_input: vec![layer(0x0000000F, 0., 4., 16., -4.)],
            shadow_send: vec![
                layer(0x0000002E, 0., 4., 12., -8.),
                layer(0x0000000F, 0., 1., 2., 0.),
            ],
            shadow_menu: vec![
                layer(0x0000001F, 0., 4., 20., 0.),
                layer(0x0000000F, 0., 2., 8., 0.),
            ],
        }
    }

    /// OpenBitFun's dark desktop palette: near-black chrome around a
    /// raised graphite panel, a soft blue accent and translucent white
    /// actions.
    pub fn bitfun_dark() -> Self {
        Self {
            bg: color(0x1C1C1F),
            sidebar: color(0x0E0E10),
            surface: color(0x1C1C1F),
            raised: color(0x26262A),
            sunken: color(0x0E0E10),
            deep: color(0x0E0E10),
            // Cards and setting groups lift off the panel with the
            // reference's 6% wash; the panel itself stays graphite.
            well: color(0xFFFFFF0F),
            fill: color(0xFFFFFF0F),
            fill_hover: color(0xFFFFFF1A),
            ink: color(0xE8E8E8),
            ink_strong: color(0xFDFDFD),
            muted: color(0xB0B0B0),
            faint: color(0x858585),
            ghost: color(0x555555),
            line: color(0xFFFFFF1F),
            subtle: color(0xFFFFFF0F),
            line_strong: color(0xFFFFFF38),
            hover: color(0xFFFFFF0F),
            selected: color(0xFFFFFF1A),
            selected_hover: color(0xFFFFFF1F),
            accent: color(0x60A5FA),
            accent_hover: color(0x3B82F6),
            accent_soft: color(0x60A5FA17),
            accent_bd: color(0x60A5FA66),
            primary: color(0xFFFFFF29),
            primary_hover: color(0xFFFFFF3D),
            primary_ink: color(0xF3F3F5),
            ok: color(0x5FC178),
            ok_soft: color(0x1AA73E1A),
            ok_bd: color(0x1AA73E4C),
            ok_ink: color(0x0E0E10),
            warn: color(0xFF930F),
            warn_soft: color(0xFF8C001A),
            warn_bd: color(0xFF8C004C),
            danger: color(0xF47F7D),
            danger_soft: color(0xEC221F1A),
            danger_bd: color(0xEC221F4C),
            done: color(0x8B5CF6),
            done_soft: color(0x8B5CF617),
            done_bd: color(0x8B5CF640),
            info: color(0x7DAFFF),
            composer_bg: color(0x1C1C1F),
            composer_line: color(0xFFFFFF2E),
            composer_focus_line: color(0x858585),
            send_bg: color(0xFFFFFF29),
            send_bg_hover: color(0xFFFFFF3D),
            send_icon: color(0xF3F3F5),
            send_bg_off: color(0xFFFFFF0F),
            send_icon_off: color(0xFFFFFF4C),
            stop_glyph: color(0xF47F7D),
            diff_add: color(0x1AA73E),
            diff_add_bg: color(0x1AA73E1A),
            diff_del: color(0xEC221F),
            diff_del_bg: color(0xEC221F1A),
            bubble: color(0xFFFFFF1A),
            selection: color(0x60A5FA42),
            menu: color(0x1C1C1FF2),
            code_inline: color(0xFFFFFF1A),
            avatars: Avatars {
                amber: AvatarColor {
                    bg: color(0xFFFFFF0F),
                    fg: color(0xFFCC00),
                },
                rose: AvatarColor {
                    bg: color(0xFFFFFF0F),
                    fg: color(0xF36F80),
                },
                orange: AvatarColor {
                    bg: color(0xFFFFFF0F),
                    fg: color(0xFF8C00),
                },
                emerald: AvatarColor {
                    bg: color(0xFFFFFF0F),
                    fg: color(0x34D399),
                },
                cyan: AvatarColor {
                    bg: color(0xFFFFFF0F),
                    fg: color(0x36C6D7),
                },
                sky: AvatarColor {
                    bg: color(0xFFFFFF0F),
                    fg: color(0x60A5FA),
                },
                violet: AvatarColor {
                    bg: color(0xFFFFFF0F),
                    fg: color(0x8B5CF6),
                },
                pink: AvatarColor {
                    bg: color(0xFFFFFF0F),
                    fg: color(0xF472B6),
                },
            },
            glow: false,
            shadow_1: vec![layer(0x00000059, 0., 1., 2., 0.)],
            shadow_2: vec![layer(0x0000007A, 0., 4., 20., 0.)],
            shadow_input: vec![layer(0x00000040, 0., 4., 16., -4.)],
            shadow_send: vec![
                layer(0x0000008C, 0., 4., 12., -8.),
                layer(0x00000033, 0., 1., 2., 0.),
            ],
            shadow_menu: vec![
                layer(0x0000007A, 0., 4., 20., 0.),
                layer(0x0000004D, 0., 2., 8., 0.),
            ],
        }
    }

    /// Kimi Code's dark desktop palette: neutral grey surfaces and its
    /// blue accent. Avatars and shadows are pure shadow's.
    pub fn kimi_dark() -> Self {
        Self {
            bg: color(0x121212),
            sidebar: color(0x0D0D0D),
            surface: color(0x1F1F1F),
            raised: color(0x292929),
            sunken: color(0x121212),
            deep: color(0x0D0D0D),
            well: color(0x1F1F1F),
            fill: color(0xFFFFFF0D),
            fill_hover: color(0xFFFFFF1A),
            ink: color(0xFFFFFFD6),
            ink_strong: color(0xFFFFFF),
            muted: color(0xFFFFFF8F),
            faint: color(0xFFFFFF6B),
            ghost: color(0xFFFFFF42),
            line: color(0xFFFFFF1F),
            subtle: color(0xFFFFFF0D),
            line_strong: color(0xFFFFFF2E),
            hover: color(0xFFFFFF0D),
            selected: color(0xFFFFFF1A),
            selected_hover: color(0xFFFFFF24),
            accent: color(0x1A88FF),
            accent_hover: color(0x258EFF),
            accent_soft: color(0x1A88FF1A),
            accent_bd: color(0x1A88FF47),
            primary: color(0x1A88FF),
            primary_hover: color(0x258EFF),
            primary_ink: color(0xFFFFFF),
            ok: color(0x3FB950),
            ok_soft: color(0x3FB95024),
            ok_bd: color(0x3FB95047),
            ok_ink: color(0x0D0D0D),
            warn: color(0xD29922),
            warn_soft: color(0xD2992224),
            warn_bd: color(0xD2992247),
            danger: color(0xF85149),
            danger_soft: color(0xF8514924),
            danger_bd: color(0xF8514947),
            done: color(0xA371F7),
            done_soft: color(0xA371F724),
            done_bd: color(0xA371F747),
            info: color(0x1A88FF),
            composer_bg: color(0x1F1F1F),
            composer_line: color(0xFFFFFF1F),
            composer_focus_line: color(0xFFFFFF40),
            send_bg: color(0xFFFFFFD6),
            send_bg_hover: color(0xFFFFFFD8),
            send_icon: color(0x1F1F1F),
            send_bg_off: color(0xFFFFFF1A),
            send_icon_off: color(0xFFFFFF47),
            stop_glyph: color(0xF85149B8),
            diff_add: color(0x3FB950),
            diff_add_bg: color(0x3FB95024),
            diff_del: color(0xF85149),
            diff_del_bg: color(0xF8514924),
            bubble: color(0x292929),
            selection: color(0x1A88FF45),
            menu: color(0x292929F2),
            code_inline: color(0xFFFFFF1A),
            ..Self::shadow()
        }
    }

    /// Kimi Code's light desktop palette: white and pale grey surfaces
    /// and its blue accent. Avatars and shadows are pure dawn's.
    pub fn kimi_light() -> Self {
        Self {
            bg: color(0xFFFFFF),
            sidebar: color(0xF9FBFC),
            surface: color(0xF5F5F5),
            raised: color(0xFFFFFF),
            sunken: color(0xF5F5F5),
            deep: color(0xFFFFFF),
            well: color(0xF5F5F5),
            fill: black(0x08),
            fill_hover: black(0x0D),
            ink: black(0xE6),
            ink_strong: color(0x000000),
            muted: black(0x99),
            faint: black(0x73),
            ghost: black(0x4D),
            line: black(0x21),
            subtle: black(0x0D),
            line_strong: black(0x26),
            hover: black(0x08),
            selected: black(0x0D),
            selected_hover: black(0x14),
            accent: color(0x1783FF),
            accent_hover: color(0x167FF7),
            accent_soft: color(0xE8F3FF),
            accent_bd: color(0x1783FF40),
            primary: color(0x1783FF),
            primary_hover: color(0x167FF7),
            primary_ink: color(0xFFFFFF),
            ok: color(0x0E7A38),
            ok_soft: color(0xE7F6EE),
            ok_bd: color(0xBFE3CC),
            ok_ink: color(0xFFFFFF),
            warn: color(0xA9610A),
            warn_soft: color(0xFBF1E0),
            warn_bd: color(0xF0D9B8),
            danger: color(0xC0392B),
            danger_soft: color(0xFBEAEA),
            danger_bd: color(0xF0CCCC),
            done: color(0x8250DF),
            done_soft: color(0xF3E8FF),
            done_bd: color(0xE0CCFF),
            info: color(0x1783FF),
            composer_bg: color(0xFFFFFF),
            composer_line: black(0x21),
            composer_focus_line: black(0x40),
            send_bg: black(0xE6),
            send_bg_hover: color(0x252525),
            send_icon: color(0xFFFFFF),
            send_bg_off: black(0x0D),
            send_icon_off: black(0x45),
            stop_glyph: color(0xC0392B),
            diff_add: color(0x16C456),
            diff_add_bg: color(0x16C4561A),
            diff_del: color(0xFF4756),
            diff_del_bg: color(0xFF4D4D1A),
            bubble: color(0xF5F5F5),
            selection: color(0x1783FF58),
            menu: color(0xFFFFFFF2),
            code_inline: black(0x08),
            ..Self::dawn()
        }
    }
}

/// The egui style of one palette: the `Visuals` mapping (panels,
/// widgets, selection, shadows), the design's spacing and the type
/// scale over the per-weight families.
#[must_use]
pub fn style(palette: &Palette, mode: Mode) -> egui::Style {
    let mut style = egui::Style::default();
    let mut visuals = match mode {
        Mode::Dark => egui::Visuals::dark(),
        Mode::Light => egui::Visuals::light(),
    };
    map_visuals(palette, &mut visuals);
    // GPUI's contrast and gamma correction lands near coverage^0.6 for
    // light text on dark; dark text on light already matches egui's.
    if mode == Mode::Dark {
        visuals.text_options.color_transfer_function =
            egui::epaint::FontColorTransferFunction::Gamma(0.6);
    }
    style.visuals = visuals;
    style.spacing.button_padding = egui::vec2(SP_4, SP_2);
    style.spacing.item_spacing = egui::vec2(SP_3, SP_3);
    style.spacing.menu_margin = egui::Margin::same(SP_3 as i8);
    style.spacing.window_margin = egui::Margin::same(SP_5 as i8);
    style.text_styles = text_styles();
    style.animation_time = T_MED;
    style
}

/// Maps one palette onto egui's `Visuals`. Roles the design has no
/// token for keep the nearest palette value, per the spike's role
/// table: `deep` sits behind on-color glyphs and code, links use the
/// accent, and the composer's focus line is not a `Visuals` role.
fn map_visuals(p: &Palette, v: &mut egui::Visuals) {
    v.override_text_color = Some(p.ink);
    v.hyperlink_color = p.accent;
    v.faint_bg_color = p.subtle;
    v.extreme_bg_color = p.deep;
    v.code_bg_color = p.code_inline;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.danger;
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = egui::Stroke::new(1.0, p.line);
    v.window_corner_radius = corner(R_XL);
    v.menu_corner_radius = corner(R_XL);
    v.window_shadow = p.shadow_2[0].egui();
    v.popup_shadow = p.shadow_menu[0].egui();
    // The caret takes the accent, as the reference's does.
    v.text_cursor.stroke.color = p.accent;
    // Selected text keeps the ink on the selection fill alone, as the
    // reference selects; the outline egui adds by default reads as a
    // second, stray border around every selection.
    v.selection = egui::style::Selection {
        bg_fill: p.selection,
        stroke: egui::Stroke::NONE,
    };
    v.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: p.surface,
        weak_bg_fill: p.surface,
        bg_stroke: egui::Stroke::new(1.0, p.line),
        fg_stroke: egui::Stroke::new(1.0, p.muted),
        corner_radius: corner(R_SM),
        expansion: 0.0,
    };
    v.widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: p.fill,
        weak_bg_fill: p.fill,
        bg_stroke: egui::Stroke::new(1.0, p.line),
        fg_stroke: egui::Stroke::new(1.0, p.ink),
        corner_radius: corner(R_MD),
        expansion: 0.0,
    };
    v.widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: p.hover,
        weak_bg_fill: p.hover,
        bg_stroke: egui::Stroke::new(1.0, p.line_strong),
        fg_stroke: egui::Stroke::new(1.0, p.ink_strong),
        corner_radius: corner(R_MD),
        expansion: 0.0,
    };
    v.widgets.active = egui::style::WidgetVisuals {
        bg_fill: p.selected,
        weak_bg_fill: p.selected,
        bg_stroke: egui::Stroke::new(1.0, p.accent_bd),
        fg_stroke: egui::Stroke::new(1.0, p.ink_strong),
        corner_radius: corner(R_MD),
        expansion: 0.0,
    };
    v.widgets.open = egui::style::WidgetVisuals {
        bg_fill: p.surface,
        weak_bg_fill: p.surface,
        bg_stroke: egui::Stroke::new(1.0, p.line),
        fg_stroke: egui::Stroke::new(1.0, p.ink),
        corner_radius: corner(R_MD),
        expansion: 0.0,
    };
}

/// The type scale over the per-weight families: body regular at the
/// base size, buttons medium at small, headings semibold at large,
/// monospace at small.
fn text_styles() -> BTreeMap<egui::TextStyle, egui::FontId> {
    BTreeMap::from([
        (
            egui::TextStyle::Small,
            egui::FontId::new(FS_XS, family(FAMILY_BODY_REGULAR)),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(FS_BASE, family(FAMILY_BODY_REGULAR)),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(FS_SM, family(FAMILY_BODY_MEDIUM)),
        ),
        (
            egui::TextStyle::Heading,
            egui::FontId::new(FS_LG, family(FAMILY_BODY_SEMIBOLD)),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(FS_SM, family(FAMILY_MONO_REGULAR)),
        ),
    ])
}

/// A design radius token as egui's corner radius, saturating at the
/// type's maximum (`R_FULL` pills).
fn corner(px: f32) -> egui::CornerRadius {
    egui::CornerRadius::same(px.round().clamp(0.0, 255.0) as u8)
}

/// Installs the theme the user chose: System, which fills both of
/// egui's slots with the pure pair so the desktop's live light/dark
/// flips re-map on their own, or one theme by name, which pins its
/// slot and pins the preference with it.
pub fn apply_choice(choice: &ThemeChoice, ctx: &egui::Context) {
    apply_choice_with(choice, ctx, &crate::themes::Catalog::default());
}

/// Applies `choice`, resolving names and System picks through `catalog`
/// so the engine's user themes draw.
pub fn apply_choice_with(
    choice: &ThemeChoice,
    ctx: &egui::Context,
    catalog: &crate::themes::Catalog,
) {
    match choice {
        ThemeChoice::System => {
            ctx.set_theme(egui::ThemePreference::System);
            for light in [false, true] {
                let name = catalog.system(light);
                if let Some((palette, mode)) = crate::themes::resolve(&name, catalog, light) {
                    apply_slot(ctx, slot_of(mode), (palette, mode));
                }
            }
        }
        ThemeChoice::Named(name) => {
            if let Some((palette, mode)) = crate::themes::resolve(name, catalog, false) {
                ctx.set_theme(match mode {
                    Mode::Dark => egui::ThemePreference::Dark,
                    Mode::Light => egui::ThemePreference::Light,
                });
                apply_slot(ctx, slot_of(mode), (palette, mode));
            }
        }
    }
}

/// The egui style slot a theme's mode fills.
fn slot_of(mode: Mode) -> egui::Theme {
    match mode {
        Mode::Dark => egui::Theme::Dark,
        Mode::Light => egui::Theme::Light,
    }
}

/// Fills one style slot with `palette` and remembers it as the active
/// palette of the slot's mode, so views can read it back without
/// touching egui.
fn apply_slot(ctx: &egui::Context, slot: egui::Theme, palette_mode: (&'static Palette, Mode)) {
    let (palette, mode) = palette_mode;
    let index = match mode {
        Mode::Dark => 0,
        Mode::Light => 1,
    };
    ctx.set_style_of(slot, style(palette, mode));
    ACTIVE.write().unwrap_or_else(PoisonError::into_inner)[index] = Some(palette);
}

/// The theme the user chose: System, which follows the platform's
/// appearance, or one theme by name.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ThemeChoice {
    /// The config's dark or light pick, following the desktop.
    #[default]
    System,
    /// One bundled or user theme.
    Named(String),
}

impl From<String> for ThemeChoice {
    /// Reads a stored choice, the names older clients stored included.
    fn from(name: String) -> Self {
        match name.as_str() {
            "system" => Self::System,
            "shadow" => Self::Named("pure-shadow".to_owned()),
            "dawn" => Self::Named("pure-dawn".to_owned()),
            _ => Self::Named(name),
        }
    }
}

impl From<ThemeChoice> for String {
    fn from(choice: ThemeChoice) -> Self {
        match choice {
            ThemeChoice::System => "system".to_owned(),
            ThemeChoice::Named(name) => name,
        }
    }
}

impl ThemeChoice {
    /// The label the theme cards show.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::System => "System".to_owned(),
            Self::Named(name) => crate::themes::label(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BODY_REGULAR_TTF, FAMILY_PROSE, FAMILY_PROSE_MONO, FAMILY_PROSE_SEMIBOLD, LINE, Mode,
        Palette, ThemeChoice, color, corner, family, install_fonts, line_h, snap, with_line_height,
    };
    use egui::Color32;

    #[test]
    fn lines_snap_to_whole_pixels_with_halves_rounding_down() {
        assert_eq!(snap(19.5), 19.0);
        assert_eq!(snap(19.51), 20.0);
        assert_eq!(snap(19.0), 19.0);
        assert_eq!(line_h(13.0), 21.0);
        assert_eq!(line_h(14.0), 23.0);
        assert_eq!(line_h(11.0), 18.0);
    }

    #[test]
    fn the_prose_faces_set_lines_at_the_reference_height() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        for (face, size) in [
            (FAMILY_PROSE, 14.0),
            (FAMILY_PROSE_SEMIBOLD, 17.0),
            (FAMILY_PROSE_MONO, 12.5),
        ] {
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout_no_wrap(
                    "one\ntwo".to_owned(),
                    egui::FontId::new(size, family(face)),
                    Color32::WHITE,
                )
            });
            let line = galley.size().y / 2.0;
            assert!(
                (line - size * LINE).abs() < 1.0,
                "{face} sets {line} per line at {size}"
            );
        }
    }

    #[test]
    fn the_streamed_emoji_resolve_behind_every_face() {
        // `has_glyph` reports a false negative for a character that
        // resolves to the same face as the replacement glyph, which is
        // exactly what egui's own emoji faces do. The layout width
        // tells them apart: a resolved emoji comes from the emoji
        // face's advance, while a missing one falls back to the
        // replacement `?` and takes its narrow advance.
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |_| {})
                .textures_delta
                .clear();
        }
        let width = |face: &str, text: &str| {
            let fid = egui::FontId::new(14.0, family(face));
            ctx.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(text.to_owned(), fid, Color32::WHITE)
                    .rect
                    .width()
            })
        };
        for face in [FAMILY_PROSE, FAMILY_PROSE_SEMIBOLD, FAMILY_PROSE_MONO] {
            let fallback = width(face, "?");
            for emoji in ["\u{2705}", "\u{1F389}"] {
                let drawn = width(face, emoji);
                assert!(
                    (drawn - fallback).abs() > 0.5,
                    "{face} draws {emoji:?} at the replacement glyph's width ({drawn} vs {fallback})"
                );
            }
        }
    }

    #[test]
    fn the_line_height_patch_refuses_what_is_not_a_font() {
        assert!(with_line_height(b"not a font", LINE).is_none());
        assert!(with_line_height(BODY_REGULAR_TTF, LINE).is_some());
    }

    #[test]
    fn colors_take_six_or_eight_digits() {
        assert_eq!(color(0xFFFFFF), Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(color(0xF2A65A).a(), 255);
        assert_eq!(
            color(0x1A88FF47),
            Color32::from_rgba_unmultiplied(0x1A, 0x88, 0xFF, 0x47)
        );
    }

    #[test]
    fn corner_radii_saturate_at_the_type_maximum() {
        assert_eq!(corner(8.0), egui::CornerRadius::same(8));
        assert_eq!(corner(999.0), egui::CornerRadius::same(255));
    }

    #[test]
    fn the_bundled_palettes_build_and_differ() {
        let shadow = Palette::shadow();
        let dawn = Palette::dawn();
        assert_ne!(shadow.bg, dawn.bg);
        for palette in [
            shadow,
            dawn,
            Palette::bitfun_light(),
            Palette::bitfun_dark(),
            Palette::kimi_dark(),
            Palette::kimi_light(),
        ] {
            assert_ne!(palette.bg, Color32::default(), "every slot is set");
        }
        for kimi in [Palette::kimi_dark(), Palette::kimi_light()] {
            assert_eq!(
                kimi.primary, kimi.accent,
                "a palette that inherits its primary from another one overrides it"
            );
        }
    }

    #[test]
    fn active_falls_back_to_the_pure_theme_of_the_mode() {
        assert_eq!(Palette::active(Mode::Dark).bg, Palette::shadow().bg);
        assert_eq!(Palette::active(Mode::Light).bg, Palette::dawn().bg);
    }

    #[test]
    fn the_stored_choice_reads_its_older_names() {
        assert_eq!(ThemeChoice::from("system".to_owned()), ThemeChoice::System);
        assert_eq!(
            ThemeChoice::from("shadow".to_owned()),
            ThemeChoice::Named("pure-shadow".to_owned())
        );
        assert_eq!(
            String::from(ThemeChoice::Named("pure-dawn".to_owned())),
            "pure-dawn"
        );
    }
}
