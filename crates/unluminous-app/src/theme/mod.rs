//! The palette, the measurements and the drawn icons.
//!
//! Every colour in [`Palette::UNLUMINOUS_DARK`] was read out of `design/intial-design-screenshot.png` rather than
//! chosen by eye. The example at `examples/sample_design.rs` reports, for each region of that image, the
//! colour covering most of it and the most saturated colour in it, which is how the accents were found. Run
//! it with `cargo run --example sample_design` to check any of these against the design again.
//!
//! ## A colour is a question now, and the list of names is still closed
//!
//! `task-1776` asks for themes, and a theme is the answer to "what does `EDITOR` mean". Until it, every
//! name here was a `const` read at 689 places in 56 files, and a constant cannot be themed. So each name is
//! a function over the **active theme** — `color::editor()` rather than `color::EDITOR` — and everything
//! the style guide says about the palette is still true: this module is the whole list of colours Unluminous
//! draws with, a new one is added here with a comment saying where it was read from, and
//! `Color32::from_rgb` at the point of use is still how a window comes to have four slightly different
//! greys.
//!
//! The list lives once, in the [`palette!`] invocation below, and the struct, the default theme, the names
//! a manifest may set, the reader, the writer and the forty accessor functions are all generated from it.
//! Writing them out would be five places to forget a name.
//!
//! ## The active theme is thread-local
//!
//! A window is one thread, and a second window is a second **process** — `services::launcher::open_window`
//! runs `current_exe` — so nothing in the shipped binary wants two themes at once, and a process-global
//! would have been correct for the product. It would have been wrong for the tests:
//! `crates/unluminous-app/tests/screenshots.rs` holds 169 accepted pictures, cargo runs them in parallel in one
//! process, and a test that switched a global theme would recolour whatever else was mid-frame. Held per
//! thread, a theme chosen in one test cannot reach another's picture, and no test needs a lock or an
//! ordering.
//!
//! It is also the cheapest of the three shapes. A colour is read thousands of times a frame; a `RefCell`
//! borrow returning one `Color32` is a counter check and four bytes, where a `RwLock` is an atomic pair and
//! a `Cell<Palette>` copies all forty colours to read one.
//!
//! What it asks is that nothing paints off the thread that drew: the background workers — `unluminous_git`, the
//! text search, the symbol index and the debug adapter — hold no painter and name no colour, and the other
//! place a colour is read, `run_cli`, runs inside `pump_control` at the top of a frame.

use std::cell::RefCell;

use egui::{Color32, CornerRadius, Stroke, Vec2};

pub mod crisp;
pub mod icon;

/// Generate the palette from one list.
///
/// Each entry becomes a field on [`Palette`], its value in [`Palette::UNLUMINOUS_DARK`], a name in
/// [`Palette::NAMES`], an arm of [`Palette::get`] and [`Palette::set`], and a function in [`color`].
/// The doc comment written once reaches the field and the function.
macro_rules! palette {
    ($( $(#[$note:meta])* $name:ident = $default:expr; )*) => {
        /// The colours one theme is made of.
        ///
        /// `Copy`, so a theme is a value a test can assert on with no window — the seam
        /// `unluminous_core::mermaid::Scene` and `services::vello_canvas::Decor` already are.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Palette {
            $( $(#[$note])* pub $name: Color32, )*
        }

        impl Palette {
            /// The palette Unluminous shipped with, and the one every theme inherits the names it does not set.
            pub const UNLUMINOUS_DARK: Palette = Palette { $( $name: $default, )* };

            /// Every role's name, in the order this file lists them.
            ///
            /// What a manifest may set, what `theme show` walks, and what the Settings page reads. One
            /// list, so a name cannot exist in one of the three and not the others.
            pub const NAMES: &'static [&'static str] = &[ $( stringify!($name), )* ];

            /// One role by name, or nothing when this version has no such role.
            pub fn get(&self, name: &str) -> Option<Color32> {
                match name { $( stringify!($name) => Some(self.$name), )* _ => None }
            }

            /// Set one role by name. False when the name is not one of [`Palette::NAMES`], which is what
            /// lets a manifest be refused with the list rather than loading with a colour it thought it
            /// had set.
            pub fn set(&mut self, name: &str, colour: Color32) -> bool {
                match name { $( stringify!($name) => { self.$name = colour; true } )* _ => false }
            }
        }

        /// The colours, by the names the style guide lists.
        pub mod color {
            use super::{with, Color32};
            pub use super::derived::*;
            $( $(#[$note])* pub fn $name() -> Color32 { with(|palette| palette.$name) } )*
        }
    };
}

palette! {
    /// Behind the text. The window's alpha is applied to this by the opacity setting.
    editor = Color32::from_rgb(0x1A, 0x1F, 0x26);
    /// The bar along the top holding the window buttons and the file name.
    title_bar = Color32::from_rgb(0x2A, 0x31, 0x3D);
    /// The bar holding the formatting controls.
    toolbar = Color32::from_rgb(0x1E, 0x22, 0x2A);
    /// Behind the file explorer.
    explorer = Color32::from_rgb(0x1F, 0x23, 0x2A);
    /// The strip at the bottom of the explorer counting the files.
    explorer_footer = Color32::from_rgb(0x1C, 0x20, 0x26);
    /// The bar along the very bottom of the window.
    status_bar = Color32::from_rgb(0x10, 0x15, 0x19);
    /// Inside a dropdown or a button that is not active.
    control = Color32::from_rgb(0x35, 0x3B, 0x46);
    /// Inside the box that filters the file list.
    field = Color32::from_rgb(0x1D, 0x21, 0x2A);
    /// Round the edge of a control.
    control_border = Color32::from_rgb(0x38, 0x3F, 0x4B);
    /// Between the panels.
    divider = Color32::from_rgb(0x2A, 0x30, 0x3B);
    /// Behind a menu. Darker than a control so that a control drawn on top of it stands out.
    menu = Color32::from_rgb(0x26, 0x2C, 0x36);

    /// Anything switched on: an active button, the caret, the row of the open file.
    accent = Color32::from_rgb(0x48, 0x9F, 0xF8);
    /// Behind the name of the file that is open.
    selected_row = Color32::from_rgb(0x30, 0x43, 0x61);
    /// There are changes that have not been saved.
    unsaved = Color32::from_rgb(0xFE, 0xBC, 0x2E);
    /// Behind selected text.
    text_selection = Color32::from_rgb(0x30, 0x43, 0x61);
    /// Behind every match of the Find bar's search that is **not** the current one.
    ///
    /// A name of its own rather than `text_selection` at an alpha, because the current match *is*
    /// the selection -- the bar selects it, so `Ctrl+F` then `Ctrl+C` copies it and Escape leaves
    /// the caret on it -- and two bands that were the same colour would say the seventeen matches
    /// and the one you are on are the same thing. Warmer than the selection and darker than the
    /// accent, so it reads as "also here" rather than as "chosen". `task-1804` §3.1.
    find_match = Color32::from_rgb(0x4A, 0x43, 0x2B);
    /// Behind a code block, a table and the front matter in the Markdown preview.
    ///
    /// A step up from `editor` rather than a colour of its own, so the block reads as a panel on the
    /// page rather than as a second surface. `task-1685` added it: a fence with no ground under it
    /// is the whole of what "code blocks aren't easy to read" meant.
    code_panel = Color32::from_rgb(0x23, 0x29, 0x33);
    /// Behind one piece of inline code, which is the same idea at the size of a word.
    code_chip = Color32::from_rgb(0x28, 0x2F, 0x3A);

    /// A heading in the editor, and the file name in the title bar.
    text_strong = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    /// Ordinary text in the editor.
    text = Color32::from_rgb(0xE8, 0xEB, 0xF1);
    /// A label on a control, and a name in the file list.
    text_control = Color32::from_rgb(0xC8, 0xCE, 0xDB);
    /// A heading in the explorer, the counts in its footer, and the status bar.
    text_dim = Color32::from_rgb(0x8B, 0x93, 0xA3);
    /// The words inside the filter box before anything is typed.
    text_faint = Color32::from_rgb(0x78, 0x80, 0x8F);

    /// The square in front of a Markdown file.
    file_markdown = Color32::from_rgb(0x41, 0x8C, 0xD9);
    /// The square in front of a plain text file.
    file_text = Color32::from_rgb(0x7E, 0x87, 0x95);

    /// The oldest commit in a file, in the blame column beside the line numbers.
    ///
    /// This pair is the one part of the palette not read out of `design/intial-design-screenshot.png`,
    /// because the design has no gutter in it. They were measured out of the capture the ask came with,
    /// `tasks/unluminous-ide-tdd.md` section 2, in the same way: the two colours covering the annotation
    /// column of that image.
    blame_old = Color32::from_rgb(0x3C, 0x7D, 0x64);
    /// The newest commit in a file. Everything between is interpolated by rank.
    blame_new = Color32::from_rgb(0xB4, 0x58, 0x8C);

    /// A file, or a line, that git does not have yet. Measured from the commit panel in the same
    /// capture, where it is the colour of the `added` count.
    git_added = Color32::from_rgb(0x7F, 0xCA, 0x98);
    /// A file, or a line, that differs from the version git has. The `modified` count in that capture.
    git_modified = Color32::from_rgb(0x4D, 0x9D, 0xC3);
    /// A file git is not tracking at all.
    git_untracked = Color32::from_rgb(0x9A, 0x8C, 0x5A);

    /// The blue a plugin's own page is built on, when that page is a copy of somebody else's.
    ///
    /// **This is the one place a second blue is right, and it took two reviews to be sure of it.** Unluminous's
    /// [`color::accent`] is an azure and the page the Agent-Tasks board is measured against is built on a
    /// periwinkle; the first pass kept the azure, on the grounds that a plugin should look like the rest of
    /// the window. `task-1765` asked for a board that looks *nearly identical to a picture*, and the
    /// reviewer named this as the most obvious mismatch in it, twice. Between a rule about how a plugin
    /// should look and an instruction about how this one must look, the instruction wins.
    ///
    /// It is contained: it reaches `plugin_ui::Palette` as `board_accent` and only the board reads it.
    /// Unluminous's own accent still means *this is where the keyboard is* everywhere, the board included, so
    /// the two never say the same thing in two colours.
    board_accent = Color32::from_rgb(0x4C, 0x6E, 0xF5);

    /// The colour an agent wears: the round badge on a card, and the dot on the `AGENT DONE` lane.
    ///
    /// The palette is closed and this does not open it, for the reason [`derived::breakpoint`] records
    /// beside itself. Unluminous's palette has a red, an amber, a green, two blues and a pink, and the board
    /// needs the four lanes to be four colours a person can tell apart at nine points across — grey, red,
    /// blue and this. It is the violet of the picture the board is measured against,
    /// `_agent_output/task-1765-vello-board/reference-board.png`, and it is used nowhere else.
    agent = Color32::from_rgb(0x9B, 0x7C, 0xF6);

    /// The three window buttons.
    close = Color32::from_rgb(0xFF, 0x5F, 0x57);
    minimise = Color32::from_rgb(0xFE, 0xBC, 0x2E);
    maximise = Color32::from_rgb(0x28, 0xC8, 0x40);

    /// A drawn icon sitting there: a rail button whose pane is put away, the explorer's arrow.
    ///
    /// The five roles from here down are `task-1776`, and each is **the colour that was already being
    /// passed** at the point of use, so nothing moved when they were added. They exist because Material
    /// Theme UI's own theme files carry exactly this — `Actions.Grey`, `Objects.Blue`,
    /// `Checkbox.Focus.Wide` — and because without them a theme could recolour the whole window and leave
    /// every icon in the greys of the theme before it.
    icon = Color32::from_rgb(0x8B, 0x93, 0xA3);
    /// The same icon when what it opens is open, or its button is on.
    icon_active = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    /// The same icon when it cannot be used — git outside a repository.
    icon_disabled = Color32::from_rgb(0x78, 0x80, 0x8F);
    /// A folder's arrow, and the folder mark the `material` icon set draws in front of its name.
    folder = Color32::from_rgb(0x8B, 0x93, 0xA3);
    /// The same folder when it is open. Atom Material Icons' one loud move, and the reason
    /// `folder` and `icon` are two roles rather than one.
    folder_open = Color32::from_rgb(0x48, 0x9F, 0xF8);

    /// A translucent wash painted over a control on hover, when nothing more specific applies.
    ///
    /// The Agent-Tasks board painted this seven times as a bare `Color32::from_white_alpha(n)`, a
    /// different `n` at each call site and no name any of them shared. `from_white_alpha(n)` is a
    /// colour whose every channel is `n`, so `hover_wash().gamma_multiply(n as f32 / 255.0)`
    /// reproduces it exactly — `no_component_writes_a_colour_of_its_own`'s round trip test proves
    /// this for every `n` from 0 to 255. The strength stays the caller's own number rather than
    /// becoming a second role: the seven washes are not all the same strength, and folding the
    /// strength into the palette as well as the colour would change what each one draws.
    hover_wash = Color32::WHITE;

    /// Words and marks drawn **on** a fill of the accent: the label on a primary button, the tick in a
    /// chosen box, the glyph on the accent disc.
    ///
    /// `task-2215` added it with the light theme. Those places drew in `text_strong`, which is white in a
    /// dark theme and is exactly right there, and is nearly black in a light one, where it put dark words
    /// on a blue button. White in both of Unluminous's own themes; a theme whose accent is pale can name a
    /// dark one.
    on_accent = Color32::WHITE;
}

impl Palette {
    /// The palette of [`Theme::unluminous_light`], written out in full so that a role added to the list
    /// above is a compile error here until somebody says what it means on a light ground.
    ///
    /// `task-2215`. The surfaces are a ladder of cool greys taken from the same place `rux`'s light theme
    /// takes its own (`reference/neumorphic-tokens.css`), so a `rux` control drawn in the window sits on a
    /// ground it was designed for: the editor is the lightest, the panels a step down, the status bar the
    /// darkest. The accent is `rux`'s light blue a shade deeper, `#2A63F0`, because Unluminous's own azure is 2.9 to 1
    /// against white and the caret, a chosen tab and a link all have to be read on white. Every text colour
    /// meets WCAG 2.2's 4.5 to 1 against the surface it is drawn on except `text_faint`, which is the
    /// words in an empty field and is held to the 3 to 1 the dark theme's own faint text meets.
    /// `light_text_is_readable_on_the_surfaces_it_is_drawn_on` measures every pair.
    pub const UNLUMINOUS_LIGHT: Palette = Palette {
        editor: Color32::from_rgb(0xFB, 0xFC, 0xFD),
        title_bar: Color32::from_rgb(0xE6, 0xEA, 0xF0),
        toolbar: Color32::from_rgb(0xF1, 0xF3, 0xF6),
        explorer: Color32::from_rgb(0xF1, 0xF3, 0xF7),
        explorer_footer: Color32::from_rgb(0xE9, 0xEC, 0xF1),
        status_bar: Color32::from_rgb(0xE1, 0xE5, 0xEB),
        control: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        field: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        control_border: Color32::from_rgb(0xC3, 0xCA, 0xD5),
        divider: Color32::from_rgb(0xD5, 0xDA, 0xE2),
        menu: Color32::from_rgb(0xF8, 0xF9, 0xFB),

        accent: Color32::from_rgb(0x2A, 0x63, 0xF0),
        selected_row: Color32::from_rgb(0xD6, 0xE3, 0xFF),
        unsaved: Color32::from_rgb(0xD9, 0x92, 0x00),
        text_selection: Color32::from_rgb(0xC6, 0xD8, 0xFF),
        find_match: Color32::from_rgb(0xF8, 0xE3, 0x9C),
        code_panel: Color32::from_rgb(0xF0, 0xF2, 0xF6),
        code_chip: Color32::from_rgb(0xE8, 0xEB, 0xF0),

        text_strong: Color32::from_rgb(0x10, 0x15, 0x1C),
        text: Color32::from_rgb(0x1E, 0x25, 0x30),
        text_control: Color32::from_rgb(0x34, 0x3D, 0x4B),
        text_dim: Color32::from_rgb(0x5A, 0x65, 0x75),
        text_faint: Color32::from_rgb(0x74, 0x7E, 0x8D),

        file_markdown: Color32::from_rgb(0x2B, 0x78, 0xD6),
        file_text: Color32::from_rgb(0x8A, 0x94, 0xA3),

        blame_old: Color32::from_rgb(0x3C, 0x8C, 0x6C),
        blame_new: Color32::from_rgb(0xB4, 0x4E, 0x8A),

        git_added: Color32::from_rgb(0x1C, 0x84, 0x48),
        git_modified: Color32::from_rgb(0x1F, 0x6C, 0xB5),
        git_untracked: Color32::from_rgb(0x8A, 0x6A, 0x1C),

        board_accent: Color32::from_rgb(0x4C, 0x6E, 0xF5),
        agent: Color32::from_rgb(0x7C, 0x5C, 0xE6),

        close: Color32::from_rgb(0xFF, 0x5F, 0x57),
        minimise: Color32::from_rgb(0xFE, 0xBC, 0x2E),
        maximise: Color32::from_rgb(0x28, 0xC8, 0x40),

        icon: Color32::from_rgb(0x5A, 0x65, 0x75),
        icon_active: Color32::from_rgb(0x10, 0x15, 0x1C),
        icon_disabled: Color32::from_rgb(0xA3, 0xAB, 0xB7),
        folder: Color32::from_rgb(0x6B, 0x76, 0x86),
        folder_open: Color32::from_rgb(0x2A, 0x63, 0xF0),

        // Black rather than white: a hover on a light surface darkens it, which is the same wash at the
        // same strengths read the other way up.
        hover_wash: Color32::BLACK,
        on_accent: Color32::WHITE,
    };
}

/// Colours that are another colour, and the marks a person makes on their own text.
///
/// Re-exported from [`color`], so `color::breakpoint()` and `color::HIGHLIGHT_YELLOW` read the way every
/// other name in the palette does. They are here rather than in the [`palette!`] list because each is
/// **defined as** something else, and a theme that could set them separately could make a breakpoint a
/// different red from the one the close button is.
pub mod derived {
    use super::{color, Color32};
    use unluminous_core::Rgba;

    /// The four colours a passage can be marked in, on the editor's right click menu.
    ///
    /// The palette is closed and these do not open it: they are the accents Unluminous shipped with — the
    /// unsaved amber, the accent blue, git's added green and blame's newest pink — each at the same alpha,
    /// which is low enough that the writing over them stays readable at every window opacity. A colour
    /// chosen in the wheel is somebody's own mark on their own text, which is the exception the style
    /// guide records beside a syntax theme's token colours.
    ///
    /// They are `unluminous_core::Rgba` rather than `Color32` because a highlight is a value that is written to
    /// a file and sent over the command line's wire as well as painted, and because egui's own alpha is
    /// premultiplied while a colour a person chose is not. [`super::color32`] is the one place the two
    /// meet.
    ///
    /// **A theme does not change them**, and that is `task-1776`'s decision rather than an omission. A
    /// mark carries the colour it was made in, in a file beside the project; if the four defaults moved
    /// with the theme, a document marked under one theme and read under another would show four colours
    /// the menu no longer offers.
    pub const HIGHLIGHT_ALPHA: u8 = 0x59;
    pub const HIGHLIGHT_YELLOW: Rgba = Rgba::new(0xFE, 0xBC, 0x2E, HIGHLIGHT_ALPHA);
    pub const HIGHLIGHT_GREEN: Rgba = Rgba::new(0x7F, 0xCA, 0x98, HIGHLIGHT_ALPHA);
    pub const HIGHLIGHT_BLUE: Rgba = Rgba::new(0x48, 0x9F, 0xF8, HIGHLIGHT_ALPHA);
    pub const HIGHLIGHT_PINK: Rgba = Rgba::new(0xB4, 0x58, 0x8C, HIGHLIGHT_ALPHA);

    /// How opaque the band behind the line a program is stopped on is, and how much of the accent's
    /// brightness it keeps.
    ///
    /// Fitted to the colour the design shipped with: `#1C3C5E` at `0x9E`, premultiplied, is the accent at
    /// a shade over 61 per cent of its brightness. The three channels want 0.628, 0.609 and 0.612, because
    /// the original was sampled off the design rather than computed from the accent, so one number
    /// reproduces it to **within one unit a channel** — which is a difference no eye has ever seen at an
    /// alpha of 158, and worth far less than a band that follows the accent under a pink theme.
    const EXECUTION_ALPHA: u8 = 0x9E;
    const EXECUTION_BRIGHTNESS: f32 = 0.615;

    /// The band behind the line the program is stopped on.
    ///
    /// The accent, at an alpha of its own so it cannot be mistaken for a passage somebody marked: the four
    /// highlight colours are all at [`HIGHLIGHT_ALPHA`] and this is deliberately not one of them. It is
    /// painted under the glyphs, where `paint_highlights` paints.
    ///
    /// **Derived rather than a role a manifest can set**, and that is a trap avoided rather than a
    /// simplification. Every other colour in the palette is opaque and is written `#RRGGBB`; this one is
    /// the only one whose alpha carries meaning, so a theme that set it in the same three bytes as the
    /// rest would paint an opaque band over the line the debugger stopped on and hide the code under it.
    /// Following the accent is also what a person means by choosing a pink theme.
    pub fn execution_point() -> Color32 {
        let accent = color::accent();
        // On a light ground a band dimmed towards black is a dark blue bar with dark words on it, so a
        // light theme takes the accent itself, faint, which is the same "the program is here" read the
        // other way up. `task-2215`.
        if !super::is_dark() {
            return Color32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                LIGHT_EXECUTION_ALPHA,
            );
        }
        let dim = |channel: u8| (channel as f32 * EXECUTION_BRIGHTNESS).round() as u8;
        Color32::from_rgba_unmultiplied(
            dim(accent.r()),
            dim(accent.g()),
            dim(accent.b()),
            EXECUTION_ALPHA,
        )
    }

    /// How opaque the accent is behind the stopped line in a light theme. Still not
    /// [`HIGHLIGHT_ALPHA`], so it cannot be mistaken for a passage somebody marked.
    const LIGHT_EXECUTION_ALPHA: u8 = 0x3A;

    /// A control's ground while the pointer is over it.
    ///
    /// The control lifted towards white in a dark theme, which is what egui's hovered style and the
    /// modal buttons always did, and pressed towards black in a light one, where a control is already
    /// white and lifting it does nothing. One function, so the three places that draw a hover agree.
    pub fn control_hover() -> Color32 {
        let control = color::control();
        match super::is_dark() {
            true => control.gamma_multiply(1.25),
            false => {
                let down = |channel: u8| (f32::from(channel) * 0.93).round() as u8;
                Color32::from_rgb(down(control.r()), down(control.g()), down(control.b()))
            }
        }
    }

    /// The words of a piece of code in the Markdown preview, inline or in a block nothing colours.
    ///
    /// The mint the preview has always set code in on a dark ground, which was a literal in
    /// `app/preview.rs` until `task-2215` found it as a pale green on a pale chip in the light theme. On a
    /// light ground it is git's added green, which is the same green taken dark enough to read.
    pub fn inline_code() -> Color32 {
        match super::is_dark() {
            true => Color32::from_rgb(0x7E, 0xD3, 0x9B),
            false => color::git_added(),
        }
    }

    /// What a modal dims the window behind it with.
    ///
    /// Black at 120 on a dark ground, which is what `design/style-guide.md` has always said. On a light
    /// ground the same black turns the window a muddy grey, so a light theme dims with `rux`'s light scrim,
    /// a deep navy at a third. `task-2215`.
    pub fn scrim() -> Color32 {
        match super::is_dark() {
            true => Color32::from_black_alpha(120),
            false => Color32::from_rgba_unmultiplied(0x14, 0x19, 0x24, 0x58),
        }
    }

    /// The four surfaces a plugin's board is built from: the page behind it, a lane, a card and a well.
    ///
    /// In a dark theme they are the editor, the explorer, the code panel and the field, which is what
    /// `plugin_ui::Palette` has always named, because a step *up* in brightness is a step towards the
    /// viewer there. In a light theme the step towards the viewer is towards white, and the code panel is
    /// a step *down* from the page, so a card painted in it sank into its lane. A light theme therefore
    /// lifts the card to the control's white and sinks the well to the code chip. `task-2215`.
    pub fn board_surfaces() -> [Color32; 4] {
        match super::is_dark() {
            true => [color::editor(), color::explorer(), color::code_panel(), color::field()],
            false => [color::explorer(), color::explorer_footer(), color::control(), color::code_chip()],
        }
    }

    /// The breakpoint dot in the gutter.
    ///
    /// The palette is closed and this does not open it: it is the close button's red, which is the one red
    /// the design already has, and a breakpoint is red in every editor there has ever been. A breakpoint
    /// that is switched off, or one the adapter could not bind, is drawn as a ring in the same colour
    /// rather than in a second one — what is different about it is that it is hollow, not that it is
    /// another colour.
    pub fn breakpoint() -> Color32 {
        color::close()
    }

    /// A value painted at the end of a line while the program is paused.
    ///
    /// The faintest text, because an inline value is decoration over somebody's code and must never be
    /// mistaken for text in the document.
    pub fn inline_value() -> Color32 {
        color::text_faint()
    }

    /// The tint a variable that has just changed wears: the unsaved amber, which is what stepping is for
    /// and is the one thing on the tree worth looking at twice.
    pub fn value_changed() -> Color32 {
        color::unsaved()
    }

    /// The ring round the badge of a ticket whose agent is running in this window.
    ///
    /// Git's added green rather than a colour of its own: it is the green Unluminous already means "there is
    /// something here that was not here before" by, which is what an attached terminal is.
    pub fn attached() -> Color32 {
        color::git_added()
    }

    /// Behind the two brackets that answer each other around the caret. `task-1922` WP4.
    ///
    /// The Find bar's own band, because the two say the same thing: *the thing you are looking at is
    /// also here*. Derived rather than a role of its own, which is [`breakpoint`]'s decision made
    /// again — the palette is closed, a theme that recoloured its find band and left its bracket band
    /// behind would be a theme with two answers to one question, and no theme has to name this.
    pub fn bracket_match() -> Color32 {
        color::find_match()
    }
}

/// Which drawn set an icon comes from.
///
/// The sixth registry of the shape `plugins::RENDERERS` started: a name checked against a list, so a
/// manifest asking for a set this version has not got is refused with the list rather than loading as a
/// theme whose buttons are drawn as nothing. See `theme::icon` for what each set covers and what falls
/// through to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconSet {
    /// The marks Unluminous shipped with: a solid disclosure triangle, stroked rail buttons, no folder mark.
    ///
    /// Kept and selectable rather than deleted, which is what makes this a **set** rather than a
    /// redrawing: One Dark names it, because One Dark's own the reference editor icons are the IDE's rather than
    /// Material's, and anybody who preferred the triangles has them back in one line.
    Classic,
    /// Heavier, rounder and filled where the classic one is a stroke, in the manner of Atom Material
    /// Icons: a chevron for a disclosure, and a folder mark in front of a folder's name.
    ///
    /// **The default**, because `task-1776` asks for the marks on the rail and the explorer's arrow to be
    /// *improved* — not merely to become choosable. A seam that left the default where it was would have
    /// answered half the ticket.
    #[default]
    Material,
}

impl IconSet {
    /// The word a manifest, the settings file and the command line call it.
    pub fn name(self) -> &'static str {
        match self {
            IconSet::Classic => "classic",
            IconSet::Material => "material",
        }
    }

    pub fn parse(name: &str) -> Option<IconSet> {
        match name.trim().to_lowercase().as_str() {
            "classic" => Some(IconSet::Classic),
            "material" => Some(IconSet::Material),
            _ => None,
        }
    }

    /// Both of them, for the Settings page's dropdown and for `plugins::ICON_SETS`.
    pub const ALL: [IconSet; 2] = [IconSet::Material, IconSet::Classic];
}

/// One theme: what every name in the palette means, what the tokens are coloured, and which icons are
/// drawn.
///
/// Read from a `plugin.kind = theme` manifest, or [`Theme::unluminous_dark`], which is the one built in.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// `unluminous/dark`, `themes-bundle-1/dracula` — the plugin and the theme, as a contributed pane is named.
    pub key: String,
    /// What a person reads in the Settings list.
    pub name: String,
    /// Which plugin it came from, for `theme list`. `unluminous` for the built-in one.
    pub plugin: String,
    /// False is refused when a manifest is read — see `services::plugins::theme_from`. Kept on the value
    /// so the seam is visible and so `theme list` can answer the question.
    pub dark: bool,
    pub palette: Palette,
    /// The nine token colours, or none — in which case each language plugin's own are used, which is what
    /// every plugin that shipped before `task-1776` carries.
    pub syntax: Option<crate::services::plugins::SyntaxTheme>,
    pub icons: IconSet,
}

impl Theme {
    /// The theme Unluminous shipped with: the design's own numbers, and **no syntax colours at all**.
    ///
    /// Naming none is what makes the default build pixel-identical to the one before themes existed. Every
    /// language plugin goes on colouring its own files with the scheme in its own manifest until a theme
    /// that names the nine is chosen, and then that one theme recolours all of them at once — which is
    /// what a colour scheme is for and why five copies of Dracula were the wrong shape.
    pub fn unluminous_dark() -> Theme {
        Theme {
            key: "unluminous/dark".to_owned(),
            name: "Unluminous Dark".to_owned(),
            plugin: "unluminous".to_owned(),
            dark: true,
            palette: Palette::UNLUMINOUS_DARK,
            syntax: None,
            icons: IconSet::default(),
        }
    }

    /// Unluminous's light theme. `task-2215`.
    ///
    /// The other theme built in, beside [`Theme::unluminous_dark`], and **unlike it, it names the nine token
    /// colours**: every language plugin carries a scheme written for a dark ground, and a keyword in
    /// Dracula's pink on white is barely there. So the light theme colours code itself, in One Light's
    /// scheme — the light half of the One family Unluminous already ships the dark half of in the themes
    /// bundle — which is what a theme naming the nine is for.
    pub fn unluminous_light() -> Theme {
        use unluminous_core::syntax::Token;
        use unluminous_core::Color;
        let scheme = vec![
            (Token::Keyword, Color::rgb(0xA6, 0x26, 0xA4)),
            (Token::Builtin, Color::rgb(0x01, 0x74, 0xA8)),
            (Token::Function, Color::rgb(0x3A, 0x6C, 0xE0)),
            (Token::Type, Color::rgb(0x98, 0x64, 0x00)),
            (Token::String, Color::rgb(0x2F, 0x7A, 0x2E)),
            (Token::Number, Color::rgb(0x98, 0x68, 0x01)),
            (Token::Comment, Color::rgb(0x84, 0x89, 0x93)),
            (Token::Operator, Color::rgb(0x0E, 0x7C, 0x86)),
            (Token::Text, Color::rgb(0x2C, 0x31, 0x3A)),
        ];
        Theme {
            key: LIGHT_KEY.to_owned(),
            name: "Unluminous Light".to_owned(),
            plugin: "unluminous".to_owned(),
            dark: false,
            palette: Palette::UNLUMINOUS_LIGHT,
            syntax: Some(crate::services::plugins::SyntaxTheme::of("Unluminous Light", scheme)),
            icons: IconSet::default(),
        }
    }

    /// Both themes Unluminous carries, dark first, which is the order the Settings list shows them in.
    pub fn built_in() -> [Theme; 2] {
        [Theme::unluminous_dark(), Theme::unluminous_light()]
    }

    /// The built-in theme a theme plugin's manifest inherits the colours it does not name from.
    ///
    /// A light theme that named only its accent would otherwise come out as a dark window with a light
    /// flag on it, which is the trap the flag exists to close.
    pub fn parent(dark: bool) -> Theme {
        match dark {
            true => Theme::unluminous_dark(),
            false => Theme::unluminous_light(),
        }
    }

    /// The same theme with one colour used for everything the accent means.
    ///
    /// Material Theme UI's best known setting, and the one thing on its configuration page somebody
    /// changes twice a year. It reaches the two roles that **are** the accent rather than merely being
    /// blue: the accent itself and an open folder's mark. The wash behind the line a program is stopped on
    /// follows it without being named here, because `derived::execution_point` is worked out from the
    /// accent rather than stored.
    pub fn with_accent(mut self, accent: Color32) -> Theme {
        self.palette.folder_open = accent;
        self.palette.accent = accent;
        self
    }
}

thread_local! {
    /// The theme this thread paints in. See the note at the top of this file for why it is not global.
    static ACTIVE: RefCell<Theme> = RefCell::new(Theme::unluminous_dark());
}

/// Read one colour out of the active theme.
///
/// The whole of what an accessor in [`color`] does. One `RefCell` borrow and a four byte copy, which is
/// what makes reading a colour thousands of times a frame cost nothing worth measuring.
fn with<T>(read: impl FnOnce(&Palette) -> T) -> T {
    ACTIVE.with_borrow(|theme| read(&theme.palette))
}

/// Paint in this theme from now on, on this thread.
///
/// `apply` still has to be called with a context afterwards, because egui keeps its own copy of the
/// colours in its style and has to be told. The window does both in one place —
/// `UnluminousApp::apply_the_theme` — so the two can never be half done.
pub fn activate(theme: Theme) {
    ACTIVE.with_borrow_mut(|active| *active = theme);
}

/// The theme this thread is painting in.
pub fn active() -> Theme {
    ACTIVE.with_borrow(Theme::clone)
}

/// The theme a window is painted in while its settings name none.
///
/// Unluminous Dark, except while `UNLUMINOUS_SURVEY_THEME=light` is set, which is how the whole screenshot
/// suite is run in the light theme to look at every surface at once (`task-2215`; the Tests section of
/// `CLAUDE.md` says how). Nothing else sets it, so in the window a settings file that says nothing is
/// Unluminous Dark, as it always was.
pub fn when_nothing_is_chosen() -> Theme {
    match std::env::var("UNLUMINOUS_SURVEY_THEME").as_deref() {
        Ok("light") => Theme::unluminous_light(),
        _ => Theme::unluminous_dark(),
    }
}

/// The key `appearance.theme` holds for [`Theme::unluminous_light`].
pub const LIGHT_KEY: &str = "unluminous/light";

/// Whether the active theme is a dark one.
///
/// Asked by the few things whose *recipe* differs between a dark ground and a light one rather than only
/// their colours: the depth `vello_canvas` draws, the hover on a control, the band behind the stopped
/// line, and which of `rux`'s two themes a control is drawn in. Everything else reads a colour and needs
/// to know nothing.
pub fn is_dark() -> bool {
    ACTIVE.with_borrow(|theme| theme.dark)
}

/// The `rux` theme that matches the active one, so a `rux` control sits on a ground it was designed for.
pub fn rux_theme() -> &'static rux::Theme {
    match is_dark() {
        true => rux::Theme::named("dark-neumorphic").unwrap_or_else(rux::theme::dark),
        false => rux::Theme::named("light-neumorphic").unwrap_or_else(rux::theme::light),
    }
}

/// Put a `rux` state on the theme that matches the active one, before it is drawn with.
///
/// A `RuxState` is made once and kept, so the theme it was made in would otherwise outlive a change of
/// Unluminous's theme. Every place that opens a `rux` layer or builds a `Rux` calls this first, and
/// `every_rux_drawing_follows_the_theme` refuses a file that draws with `rux` and does not. It costs one
/// `Cell` write, and `rux` rasterises again only when a colour really moved.
pub fn in_step(state: &rux::RuxState) {
    let wanted = rux_theme();
    if !std::ptr::eq(state.theme(), wanted) {
        state.set_theme(wanted);
    }
}

/// The active theme's palette, which is what a plugin's provider is handed.
pub fn palette() -> Palette {
    ACTIVE.with_borrow(|theme| theme.palette)
}

/// Which icons the active theme draws with.
pub fn icons() -> IconSet {
    ACTIVE.with_borrow(|theme| theme.icons)
}

/// The nine token colours the active theme names, if it names them.
///
/// Asked at the moment a file is coloured, exactly as `Plugins::renders` is asked before a diagram is
/// drawn, so choosing a theme recolours every open file in the same frame rather than at the next restart.
pub fn syntax() -> Option<crate::services::plugins::SyntaxTheme> {
    ACTIVE.with_borrow(|theme| theme.syntax.clone())
}

/// Measurements taken from the design.
pub mod size {
    /// Height of the bar holding the window buttons and the file name.
    ///
    /// Fifty left more room above and below the buttons than anything in the bar needed — nothing in
    /// it is taller than the twenty four point run widget — so the window opened with a band of empty
    /// colour across the top. Thirty eight keeps seven points clear of the tallest thing in it.
    pub const TITLE_BAR: f32 = 38.0;
    /// Width of the rail of pane buttons down the far left of the window.
    ///
    /// Narrower than the reference editor's, which is about forty points, because `task-1658` asks for that and
    /// because Unluminous's holds three buttons rather than a dozen. Twenty four for the button, six either
    /// side — and the six on the left is exactly what `components::resize_edges` takes, so a button and
    /// the window's own left grip never fight over the same point.
    pub const ACTIVITY_BAR: f32 = 36.0;
    /// Height of the bar along the bottom of the window.
    pub const STATUS_BAR: f32 = 32.0;
    /// Width of the file explorer.
    pub const EXPLORER: f32 = 248.0;
    /// Height of the strip counting the files.
    pub const EXPLORER_FOOTER: f32 = 28.0;
    /// One row in the file list.
    pub const ROW: f32 = 28.0;
    /// How far one level of nesting indents.
    pub const INDENT: f32 = 18.0;
    /// Space between the text and the left edge of the editing area.
    pub const EDITOR_PADDING_X: f32 = 43.0;
    /// Space between the text and the top of the editing area.
    pub const EDITOR_PADDING_Y: f32 = 36.0;
    /// The narrowest an editing pane may be dragged.
    ///
    /// Wide enough to still be an editor rather than a stripe: the gutter, the padding either side
    /// and enough room for a line of text. A divider that could be dragged past it would be a way of
    /// losing a pane off the side of the window.
    pub const EDITOR_PANE_MIN: f32 = 160.0;
    /// The window's rounded corner.
    pub const WINDOW_CORNER: u8 = 12;
    /// A control's rounded corner.
    pub const CONTROL_CORNER: u8 = 6;
}

/// A highlight's colour as egui wants it.
///
/// The one place the two spellings of a colour meet. `unluminous_core::Rgba` is what a mark is stored,
/// written and sent as — four plain bytes with the alpha kept separate — and egui's `Color32` keeps
/// its alpha premultiplied. Converting in one function is what stops a highlight coming out too
/// bright in one place and right in another.
pub fn color32(color: unluminous_core::Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

/// Set up egui so that the ordinary controls come out looking like the design, rather than restyling each
/// one where it is used.
///
/// Called again whenever the theme changes, because egui keeps its own copy of these colours in its style.
/// `interface_scale` is the interface font size as a multiple of egui's own default — `appearance.ui.font.size`
/// divided by the size egui sets a body of text in — which is the reference editor's `Use custom font` and is 1.0 until
/// somebody asks for something else.
pub fn apply(ctx: &egui::Context) {
    apply_scaled(ctx, 1.0);
}

/// The same, with the interface set larger or smaller.
pub fn apply_scaled(ctx: &egui::Context, interface_scale: f32) {
    // egui scales its whole interface — every menu, the explorer, the status bar — when command and
    // plus is pressed. That is a browser's zoom, and it is not what an editor's zoom means: Unluminous's
    // command and plus changes the size the *document* is set in and leaves the window alone. With
    // egui's own left on, one press would do both.
    ctx.options_mut(|options| options.zoom_with_keyboard = false);
    let scale = interface_scale.clamp(0.6, 2.0);
    let dark = is_dark();
    // egui keeps a dark style and a light style and picks one by `Context::theme`, which follows the
    // operating system unless it is told. Both are written below, and the window says which to use, so a
    // light theme is light on a machine set to dark mode and the other way round. `task-2215`.
    ctx.set_theme(match dark {
        true => egui::Theme::Dark,
        false => egui::Theme::Light,
    });
    ctx.all_styles_mut(|style| {
        // Start from egui's own visuals for this kind of ground, so the handful of colours set nowhere
        // below — the shadow under a popup, the warning and error text, a hyperlink — are the ones egui
        // chose for a light or a dark window rather than whatever the other style had.
        // The text cursor is kept: it is not a colour, and a test harness turns its blinking off
        // through the style so that a picture is the same on every run.
        let cursor = style.visuals.text_cursor.clone();
        style.visuals = match dark {
            true => egui::Visuals::dark(),
            false => egui::Visuals::light(),
        };
        style.visuals.text_cursor = cursor;
        let visuals = &mut style.visuals;
        visuals.dark_mode = dark;
        visuals.panel_fill = color::toolbar();
        visuals.window_fill = color::menu();
        visuals.extreme_bg_color = color::field();
        visuals.faint_bg_color = color::explorer_footer();
        visuals.window_corner_radius = CornerRadius::same(size::CONTROL_CORNER);
        visuals.window_stroke = Stroke::new(1.0, color::control_border());
        // A selection in a text box is painted under the words, which keep their own colour. On a dark
        // ground the accent is dark enough for light words to be read on it; on a light ground it is not,
        // so a light theme selects in the same pale blue the editor selects in.
        match dark {
            true => {
                visuals.selection.bg_fill = color::accent();
                visuals.selection.stroke = Stroke::new(1.0, color::on_accent());
            }
            false => {
                visuals.selection.bg_fill = color::text_selection();
                visuals.selection.stroke = Stroke::new(1.0, color::text_strong());
            }
        }

        let corner = CornerRadius::same(size::CONTROL_CORNER);
        // Not interactive: labels and separators.
        visuals.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
        visuals.widgets.noninteractive.weak_bg_fill = Color32::TRANSPARENT;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, color::divider());
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, color::text_control());
        visuals.widgets.noninteractive.corner_radius = corner;
        // Sitting there, not being pointed at.
        visuals.widgets.inactive.bg_fill = color::control();
        visuals.widgets.inactive.weak_bg_fill = color::control();
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, color::control_border());
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, color::text_control());
        visuals.widgets.inactive.corner_radius = corner;
        // Being pointed at.
        visuals.widgets.hovered.bg_fill = color::control_hover();
        visuals.widgets.hovered.weak_bg_fill = color::control_hover();
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, color::accent().gamma_multiply(0.6));
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, color::text_strong());
        visuals.widgets.hovered.corner_radius = corner;
        // Being pressed.
        visuals.widgets.active.bg_fill = color::accent();
        visuals.widgets.active.weak_bg_fill = color::accent();
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, color::accent());
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, color::on_accent());
        visuals.widgets.active.corner_radius = corner;
        // A dropdown that is open.
        visuals.widgets.open.bg_fill = color::control();
        visuals.widgets.open.weak_bg_fill = color::control();
        visuals.widgets.open.bg_stroke = Stroke::new(1.0, color::accent());
        visuals.widgets.open.fg_stroke = Stroke::new(1.0, color::text_strong());
        visuals.widgets.open.corner_radius = corner;

        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.spacing.menu_margin = egui::Margin::same(6);

        // The interface's own font size, which is the reference editor's Appearance -> Use custom font.
        //
        // **Set from egui's own defaults rather than multiplied in place**, because this function is
        // called again every time the theme changes: scaling what is already there would compound, so
        // choosing 16 points twice would land on 20. Reading the defaults each time makes it absolute,
        // and at a scale of one it writes back exactly what egui had, so an Unluminous that names no size in
        // its settings file is drawn exactly as it always was.
        let defaults = egui::Style::default();
        for (kind, font) in style.text_styles.iter_mut() {
            if let Some(default) = defaults.text_styles.get(kind) {
                font.size = (default.size * scale).round();
            }
        }
    });
}

/// Put egui back on the style that matches the active theme, if something has moved it.
///
/// `apply_scaled` writes both of egui's styles and tells it which to use, but the choice can be moved
/// afterwards by whoever owns the context: the test harness sets one when it builds a window, and egui
/// follows the operating system when it is left to. Asked at the top of every frame, which costs one
/// comparison, so a light theme can never be drawn with egui's dark menus. `task-2215`.
pub fn keep_egui_on_the_theme(ctx: &egui::Context) {
    let wanted = match is_dark() {
        true => egui::Theme::Dark,
        false => egui::Theme::Light,
    };
    if ctx.theme() != wanted {
        ctx.set_theme(wanted);
    }
}

/// The family name egui uses for the interface's bold text.
pub const BOLD_FAMILY: &str = "unluminous-bold";

/// Set the interface in a real font, so that the toolbar's bold B is actually bold.
///
/// egui's built in fonts have no bold face, so its `strong` styling only brightens the colour. The design
/// shows a genuinely bold B, so the family Unluminous is using is handed to egui as well, with its bold face
/// under a name the toolbar can ask for. egui's own fonts stay in the list behind ours, because they carry
/// symbols such as the triangles in front of a folder that a text face does not have.
pub fn install_fonts(
    ctx: &egui::Context,
    family: &str,
    regular: Option<Vec<u8>>,
    bold: Option<Vec<u8>>,
) {
    let mut fonts = egui::FontDefinitions::default();
    let mut bold_stack = Vec::new();
    if let Some(bytes) = bold {
        fonts.font_data.insert(
            "unluminous-ui-bold".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        bold_stack.push("unluminous-ui-bold".to_owned());
    }
    if let Some(bytes) = regular {
        fonts.font_data.insert(
            "unluminous-ui".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            list.insert(0, "unluminous-ui".to_owned());
        }
        bold_stack.push("unluminous-ui".to_owned());
    }
    // Whatever egui already had stays behind ours as a fallback for symbols.
    if let Some(defaults) = fonts.families.get(&egui::FontFamily::Proportional) {
        for name in defaults.clone() {
            if !bold_stack.contains(&name) {
                bold_stack.push(name);
            }
        }
    }
    if !bold_stack.is_empty() {
        fonts.families.insert(egui::FontFamily::Name(BOLD_FAMILY.into()), bold_stack.clone());
    }
    name_the_rux_families(&mut fonts, &bold_stack);
    let _ = family;
    ctx.set_fonts(fonts);
}

/// Bind the seven family names `rux` sets its text in to the faces Unluminous already has.
///
/// `rux`'s components ask for `FontFamily::Name("rux-sans-500")` and the like, and egui panics on a
/// family nobody bound. `rux::text::install` binds them to the sans and monospace faces it carries, but it does
/// that by replacing every font in the context, which would change the typeface of the whole window. So
/// the names are bound here to the interface's own regular and bold faces and to egui's monospace, and a
/// `rux` control in Unluminous is set in Unluminous's typeface. `task-2096`, for the model selector.
fn name_the_rux_families(fonts: &mut egui::FontDefinitions, bold_stack: &[String]) {
    let regular = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    let bold = match bold_stack.is_empty() {
        true => regular.clone(),
        false => bold_stack.to_vec(),
    };
    let mono = fonts.families.get(&egui::FontFamily::Monospace).cloned().unwrap_or_default();
    let names = [
        ("rux-sans-400", &regular),
        ("rux-sans-500", &regular),
        ("rux-sans-600", &bold),
        ("rux-sans-700", &bold),
        ("rux-mono-400", &mono),
        ("rux-mono-500", &mono),
        ("rux-mono-600", &mono),
    ];
    for (name, faces) in names {
        fonts.families.insert(egui::FontFamily::Name(name.into()), faces.clone());
    }
}

/// The colour a document's text is painted in, from the colour its formatting holds.
///
/// A document that nobody has coloured holds `Color::WHITE`, which is what `CharStyle` starts with and
/// what the `F` panel's first swatch puts back. It means "the ordinary ink", and on a dark ground that is
/// the white it says. On a light ground white words are invisible, so a light theme paints that one value
/// in its own `text` colour and every other colour as it is. It is a reading at paint time, not a change
/// to the document, so nothing is written to the file or put on the undo history, and switching back to a
/// dark theme shows the white that was there all along. `task-2215`.
pub fn ink(colour: unluminous_core::Color) -> Color32 {
    if colour == unluminous_core::Color::WHITE && !is_dark() {
        return color::text();
    }
    Color32::from_rgb(colour.r, colour.g, colour.b)
}

/// Apply the opacity setting to a background colour.
///
/// Only backgrounds go through this. Text, icons and the caret are always drawn at full alpha, which is
/// what lets the desktop show through the window without making the writing hard to read.
pub fn faded(base: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        base.r(),
        base.g(),
        base.b(),
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// The colour of the square in front of a file, by what kind of file it is.
///
/// Markdown gets the blue square because Unluminous treats it differently, having a preview for it. Every
/// other kind of text gets the grey one, whether Unluminous knows the extension or not. What the status bar
/// calls the file is decided by `services::file_kind::kind_name`, not here.
pub fn file_marker(path: &std::path::Path) -> Color32 {
    if crate::services::file_kind::is_markdown(Some(path)) {
        color::file_markdown()
    } else {
        color::file_text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test that makes the 689-site rewrite safe: every colour Unluminous shipped with, still there.
    #[test]
    fn unluminous_dark_is_exactly_what_shipped() {
        activate(Theme::unluminous_dark());
        assert_eq!(color::editor(), Color32::from_rgb(0x1A, 0x1F, 0x26));
        assert_eq!(color::title_bar(), Color32::from_rgb(0x2A, 0x31, 0x3D));
        assert_eq!(color::accent(), Color32::from_rgb(0x48, 0x9F, 0xF8));
        assert_eq!(color::text_strong(), Color32::WHITE);
        assert_eq!(color::text_faint(), Color32::from_rgb(0x78, 0x80, 0x8F));
        assert_eq!(color::git_added(), Color32::from_rgb(0x7F, 0xCA, 0x98));
        assert_eq!(color::board_accent(), Color32::from_rgb(0x4C, 0x6E, 0xF5));
        assert_eq!(color::close(), Color32::from_rgb(0xFF, 0x5F, 0x57));
        // The five icon roles are the colours that were being passed before they existed.
        assert_eq!(color::icon(), color::text_dim(), "an icon sitting there was TEXT_DIM");
        assert_eq!(color::icon_active(), color::text_strong());
        assert_eq!(color::icon_disabled(), color::text_faint());
        assert_eq!(color::folder(), color::text_dim(), "the explorer's arrow was TEXT_DIM");
    }

    /// WCAG 2.2's relative luminance of one colour.
    fn luminance(colour: Color32) -> f32 {
        let linear = |channel: u8| {
            let c = f32::from(channel) / 255.0;
            match c <= 0.04045 {
                true => c / 12.92,
                false => ((c + 0.055) / 1.055).powf(2.4),
            }
        };
        0.2126 * linear(colour.r()) + 0.7152 * linear(colour.g()) + 0.0722 * linear(colour.b())
    }

    /// WCAG 2.2's contrast ratio between two colours.
    fn contrast(one: Color32, two: Color32) -> f32 {
        let (a, b) = (luminance(one), luminance(two));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// `task-2215`. Every word in the light theme can be read on the surfaces it is drawn on: 4.5 to 1
    /// for text, which is WCAG 2.2's AA for ordinary text, and 3 to 1 for the faint words in an empty
    /// field and for the accent as a mark, which is its figure for a control's state.
    #[test]
    fn light_text_is_readable_on_the_surfaces_it_is_drawn_on() {
        let light = Palette::UNLUMINOUS_LIGHT;
        let surfaces = [
            ("editor", light.editor),
            ("explorer", light.explorer),
            ("toolbar", light.toolbar),
            ("title_bar", light.title_bar),
            ("status_bar", light.status_bar),
            ("menu", light.menu),
            ("control", light.control),
            ("selected_row", light.selected_row),
            ("code_panel", light.code_panel),
        ];
        let mut failures = Vec::new();
        for (surface, ground) in surfaces {
            for (word, ink, floor) in [
                ("text_strong", light.text_strong, 4.5),
                ("text", light.text, 4.5),
                ("text_control", light.text_control, 4.5),
                ("text_dim", light.text_dim, 4.5),
                ("icon", light.icon, 4.5),
                ("text_faint", light.text_faint, 3.0),
                ("accent", light.accent, 3.0),
            ] {
                let ratio = contrast(ink, ground);
                if ratio < floor {
                    failures.push(format!("{word} on {surface}: {ratio:.2} to 1, under {floor}"));
                }
            }
        }
        let on_accent = contrast(light.on_accent, light.accent);
        if on_accent < 4.5 {
            failures.push(format!("on_accent on accent: {on_accent:.2} to 1"));
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// The light theme's code colours can be read on its editor, at the same 4.5 to 1.
    #[test]
    fn light_code_is_readable_on_the_light_editor() {
        let light = Theme::unluminous_light();
        let scheme = light.syntax.expect("the light theme colours code itself");
        for token in unluminous_core::syntax::Token::ALL {
            let colour = scheme.colour(token).expect("all nine are named");
            let ink = Color32::from_rgb(colour.r, colour.g, colour.b);
            let ratio = contrast(ink, light.palette.editor);
            let floor = match token {
                // A comment is the one token every editor deliberately sets quieter than the code.
                unluminous_core::syntax::Token::Comment => 3.0,
                _ => 4.5,
            };
            assert!(ratio >= floor, "{} is {ratio:.2} to 1 on the light editor", token.name());
        }
    }

    /// `task-2215`. What follows the darkness of the theme rather than a colour in it.
    #[test]
    fn the_light_theme_changes_the_recipes_that_depend_on_the_ground() {
        activate(Theme::unluminous_dark());
        assert!(is_dark());
        assert_eq!(rux_theme().name, "dark-neumorphic");
        assert_eq!(color::control_hover(), color::control().gamma_multiply(1.25), "as it always was");
        assert_eq!(color::scrim(), Color32::from_black_alpha(120), "as the style guide says");
        assert_eq!(
            color::board_surfaces(),
            [color::editor(), color::explorer(), color::code_panel(), color::field()],
            "the board's ladder is the one it always had"
        );

        activate(Theme::unluminous_light());
        assert!(!is_dark());
        assert_eq!(rux_theme().name, "light-neumorphic");
        assert!(
            luminance(color::control_hover()) < luminance(color::control()),
            "a hover on a light control darkens it"
        );
        let [page, lane, card, _] = color::board_surfaces();
        assert!(luminance(card) > luminance(lane), "a card stands up off its lane, towards white");
        assert!(luminance(page) > luminance(lane));
        assert_eq!(color::hover_wash(), Color32::BLACK, "a hover darkens a light surface");
        let band = color::execution_point();
        assert!(band.a() < 0x60, "the stopped line is a faint wash on a light ground, not a bar");
        activate(Theme::unluminous_dark());
    }

    /// A document nobody coloured holds white, and a light theme paints that white as its own ink while
    /// leaving every colour somebody chose alone.
    #[test]
    fn the_ordinary_ink_follows_the_ground_and_a_chosen_colour_does_not() {
        use unluminous_core::Color;
        activate(Theme::unluminous_dark());
        assert_eq!(ink(Color::WHITE), Color32::from_rgb(0xF2, 0xF2, 0xF2), "white, as it always was");
        activate(Theme::unluminous_light());
        assert_eq!(ink(Color::WHITE), color::text(), "the light theme's own ink");
        assert_eq!(ink(Color::RED), Color32::from_rgb(Color::RED.r, Color::RED.g, Color::RED.b));
        activate(Theme::unluminous_dark());
    }

    /// Both built-in themes, dark first, and when nothing is chosen the window is the dark one.
    #[test]
    fn unluminous_carries_a_dark_theme_and_a_light_one() {
        let [dark, light] = Theme::built_in();
        assert_eq!(dark.key, "unluminous/dark");
        assert!(dark.dark);
        assert_eq!(light.key, LIGHT_KEY);
        assert!(!light.dark);
        assert_eq!(light.name, "Unluminous Light");
        if std::env::var("UNLUMINOUS_SURVEY_THEME").is_err() {
            assert_eq!(when_nothing_is_chosen(), dark, "a settings file that says nothing is dark");
        }
    }

    #[test]
    fn a_derived_colour_follows_the_one_it_is_defined_as() {
        activate(Theme::unluminous_dark());
        assert_eq!(color::breakpoint(), color::close());
        assert_eq!(color::inline_value(), color::text_faint());
        assert_eq!(color::value_changed(), color::unsaved());
        assert_eq!(color::attached(), color::git_added());

        let mut theme = Theme::unluminous_dark();
        theme.palette.close = Color32::from_rgb(0xFF, 0x61, 0x88);
        activate(theme);
        assert_eq!(color::breakpoint(), Color32::from_rgb(0xFF, 0x61, 0x88), "and it followed");
        activate(Theme::unluminous_dark());
    }

    #[test]
    fn a_theme_reaches_every_accessor() {
        let mut theme = Theme::unluminous_dark();
        theme.key = "test/monokai".to_owned();
        theme.palette.editor = Color32::from_rgb(0x2D, 0x2A, 0x2E);
        theme.palette.accent = Color32::from_rgb(0xFF, 0xD8, 0x66);
        activate(theme);
        assert_eq!(color::editor(), Color32::from_rgb(0x2D, 0x2A, 0x2E));
        assert_eq!(color::accent(), Color32::from_rgb(0xFF, 0xD8, 0x66));
        assert_eq!(active().key, "test/monokai");

        activate(Theme::unluminous_dark());
        assert_eq!(color::editor(), Color32::from_rgb(0x1A, 0x1F, 0x26), "and back");
    }

    #[test]
    fn every_role_is_readable_and_writable_by_name() {
        let mut palette = Palette::UNLUMINOUS_DARK;
        for name in Palette::NAMES {
            assert!(palette.get(name).is_some(), "{name} can be read");
            assert!(palette.set(name, Color32::RED), "{name} can be set");
        }
        assert_eq!(palette.editor, Color32::RED);
        assert!(palette.get("editor_background").is_none(), "a name Unluminous has not got");
        assert!(!palette.set("editor_background", Color32::RED));
    }

    #[test]
    fn an_accent_reaches_everything_that_means_the_accent() {
        activate(Theme::unluminous_dark());
        // What the design shipped, to within one unit a channel — see `EXECUTION_BRIGHTNESS`.
        let shipped = Color32::from_rgba_premultiplied(0x1C, 0x3C, 0x5E, 0x9E);
        let derived = color::execution_point();
        assert_eq!(derived.a(), shipped.a());
        for (was, now) in shipped.to_array().iter().zip(derived.to_array()) {
            assert!(was.abs_diff(now) <= 1, "{shipped:?} against {derived:?}");
        }

        activate(Theme::unluminous_dark().with_accent(Color32::from_rgb(0xFF, 0x79, 0xC6)));
        assert_eq!(color::accent(), Color32::from_rgb(0xFF, 0x79, 0xC6));
        assert_eq!(color::folder_open(), Color32::from_rgb(0xFF, 0x79, 0xC6));
        let wash = color::execution_point();
        assert_eq!(wash.a(), 0x9E, "the wash keeps its own alpha, not a highlight's");
        assert!(wash.r() > wash.b(), "and it followed the accent into the pink");
        activate(Theme::unluminous_dark());
    }

    /// Why the active theme is thread-local rather than global — see the note at the top of this file.
    #[test]
    fn the_active_theme_does_not_leak_between_threads() {
        activate(Theme::unluminous_dark());
        let elsewhere = std::thread::spawn(|| {
            let mut theme = Theme::unluminous_dark();
            theme.palette.editor = Color32::from_rgb(0x0F, 0x11, 0x1A);
            activate(theme);
            color::editor()
        })
        .join()
        .expect("the other thread finished");
        assert_eq!(elsewhere, Color32::from_rgb(0x0F, 0x11, 0x1A), "it got its own");
        assert_eq!(
            color::editor(),
            Color32::from_rgb(0x1A, 0x1F, 0x26),
            "and this one is untouched"
        );
    }

    /// The interface size is set from egui's defaults rather than multiplied into what is there.
    ///
    /// `apply_scaled` runs again on every theme change, so a scale applied in place would compound:
    /// choosing sixteen points and then choosing a theme would land on twenty.
    #[test]
    fn the_interface_size_is_absolute_rather_than_compounding() {
        let context = egui::Context::default();
        let size_of = |context: &egui::Context| {
            context
                .style_of(egui::Theme::Dark)
                .text_styles
                .get(&egui::TextStyle::Body)
                .map(|font| font.size)
        };
        apply(&context);
        let plain = size_of(&context).expect("egui has a body style");

        apply_scaled(&context, 1.6);
        let once = size_of(&context).expect("still there");
        assert!(once > plain, "asking for a larger interface makes it larger");
        apply_scaled(&context, 1.6);
        assert_eq!(size_of(&context), Some(once), "and asking twice is asking once");

        apply_scaled(&context, 1.0);
        assert_eq!(size_of(&context), Some(plain), "and one is exactly what egui had");
    }

    #[test]
    fn an_icon_set_is_named_the_way_the_settings_file_writes_it() {
        for set in IconSet::ALL {
            assert_eq!(IconSet::parse(set.name()), Some(set));
        }
        assert_eq!(IconSet::parse("MATERIAL"), Some(IconSet::Material));
        assert_eq!(
            IconSet::default(),
            IconSet::Material,
            "the improved marks are what a window comes up in"
        );
        assert_eq!(IconSet::parse("atom"), None);
    }

    /// `color::hover_wash()` is `Color32::WHITE`, and every one of the board's hover washes used
    /// to be `Color32::from_white_alpha(n)` for its own `n`. This is the proof that
    /// `hover_wash().gamma_multiply(n as f32 / 255.0)` draws the same colour `from_white_alpha(n)`
    /// always did, for every `n` a `u8` can hold, so moving the seven call sites onto the shared
    /// role could not have changed a pixel.
    #[test]
    fn a_gamma_multiplied_hover_wash_reproduces_every_white_alpha_the_board_used() {
        for n in 0..=255u8 {
            let expected = Color32::from_white_alpha(n);
            let actual = color::hover_wash().gamma_multiply(n as f32 / 255.0);
            assert_eq!(actual, expected, "at alpha {n}");
        }
    }
}

#[cfg(test)]
mod closed_palette {
    /// **No component names a colour.** `task-1804` §8.1 asked for this test and said why: the
    /// palette is closed and the discipline is real -- `theme::color` is the whole list, a theme says
    /// what a name *means* and cannot add a forty-first -- but *"none of it is enforced by a test,
    /// unlike almost every other invariant in this project"*.
    ///
    /// So this is the enforcement. It reads the source of `components/` and fails on a literal
    /// colour: `Color32::from_rgb(0x1A, 0x1F, 0x26)` or `from_rgb(26, 31, 38)`.
    ///
    /// ## What it deliberately allows
    ///
    /// **A conversion of a colour something else already decided.** All 22 raw `from_rgb` calls in
    /// `components/` today are `Color32::from_rgb(colour.r, colour.g, colour.b)` -- a syntax token's
    /// colour, a terminal palette entry, an epic's colour off the board -- and those are not the
    /// thing the rule is about. What the rule forbids is a component *choosing* a colour, and a
    /// component cannot choose one it was handed.
    ///
    /// The test is therefore about **literal numbers**, which is exactly what "a hardcoded hex
    /// triple" means and is what makes it a rule a reader can check by eye.
    #[test]
    fn no_component_writes_a_colour_of_its_own() {
        // `app/` as well as `components/` since `task-2215`: the Markdown preview's code colour was a
        // literal `Color::rgb` in `app/preview.rs`, outside the one folder this read, and it was the one
        // colour in the window that did not follow the light theme.
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders: Vec<String> = Vec::new();
        for folder in ["components", "app"] {
            for (path, text) in every_source(&source.join(folder)) {
                for (number, line) in text.lines().enumerate() {
                    let trimmed = line.trim_start();
                    // Test code at the bottom of a file asserts on numbers, which is what it is for.
                    if trimmed.starts_with("#[cfg(test)]") {
                        break;
                    }
                    // A comment may name a colour -- several explain why one was chosen -- and a
                    // comment is not code. `epaint`'s own named constants are not literals either.
                    if trimmed.starts_with("//") {
                        continue;
                    }
                    for call in ["from_rgb", "Color::rgb"] {
                        if let Some(at) = trimmed.find(call) {
                            if writes_a_literal(&trimmed[at..]) {
                                offenders.push(format!("{folder}/{}:{}: {}", path, number + 1, trimmed));
                            }
                        }
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "a component chose a colour of its own. The palette is `theme::color` and it is \
             closed -- a colour that is not in it goes in `theme/mod.rs`'s `palette!` list with a \
             note saying what it means, so a theme can say what it means too:\n  {}",
            offenders.join("\n  ")
        );
    }

    /// **No component paints in a white or a black of its own**, which is the other half of the rule
    /// above and the half `task-2215` needed.
    ///
    /// `Color32::WHITE` and `from_black_alpha(n)` are not hex triples, so the test above let them
    /// through, and they are exactly the colours that assume a dark ground: a white wash for a hover, a
    /// black shadow, white words on a control. On a light theme each one is either invisible or wrong.
    /// The answer is a role or a derived colour in this file — `hover_wash`, `on_accent`, `scrim`,
    /// `control_hover` — which a light theme can answer differently.
    ///
    /// A line may still name one when the colour really is the same on any ground, and it says so with
    /// `// any ground:` and the reason: the tint an image is drawn with, which leaves it as it is, or a
    /// plate over a photograph. Test code at the bottom of a file is not drawing anything.
    #[test]
    fn no_component_paints_a_white_or_black_of_its_own() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/components");
        let forbidden =
            ["Color32::WHITE", "Color32::BLACK", "from_white_alpha(", "from_black_alpha(", "from_gray("];
        let mut offenders: Vec<String> = Vec::new();
        for (path, text) in every_source(&folder) {
            for (number, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("#[cfg(test)]") {
                    break;
                }
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") || line.contains("// any ground:") {
                    continue;
                }
                if forbidden.iter().any(|name| line.contains(name)) {
                    offenders.push(format!("{}:{}: {}", path, number + 1, trimmed));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "a component painted a white or a black that only reads on one kind of ground. Use a role \
             in `theme::color` (`hover_wash`, `on_accent`, `scrim`, `control_hover`), or, if the colour \
             really is the same on any ground, end the line with `// any ground:` and the reason:\n  {}",
            offenders.join("\n  ")
        );
    }

    /// **Every `rux` drawing is put on the active theme first.** A `RuxState` is made once and kept,
    /// so a file that opens a `rux` layer without `theme::in_step` draws its controls in whichever
    /// theme was active when the state was made — dark controls in a light window. `task-2215`.
    #[test]
    fn every_rux_drawing_follows_the_theme() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders: Vec<String> = Vec::new();
        for (path, text) in every_source(&source) {
            if path.starts_with("theme/") {
                continue;
            }
            let draws = text.lines().any(|line| {
                let line = line.trim_start();
                !line.starts_with("//")
                    && (line.contains("rux::layer(")
                        || (line.contains("Rux {") && !line.contains("state: rux.state")))
            });
            if draws && !text.contains("theme::in_step(") {
                offenders.push(path);
            }
        }
        assert!(
            offenders.is_empty(),
            "these files draw with `rux` and never call `crate::theme::in_step` on the state first, so \
             their controls keep the theme they were made in: {offenders:?}"
        );
    }

    /// Whether a `from_rgb(` call's first value is a literal number rather than something read.
    fn writes_a_literal(after: &str) -> bool {
        let Some(open) = after.find('(') else {
            return false;
        };
        let inside = &after[open + 1..];
        let first = inside.trim_start();
        first.starts_with("0x") || first.chars().next().is_some_and(|c| c.is_ascii_digit())
    }

    /// Every `.rs` file under `folder`, with its path relative to it.
    fn every_source(folder: &std::path::Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut waiting = vec![folder.to_path_buf()];
        while let Some(here) = waiting.pop() {
            let Ok(entries) = std::fs::read_dir(&here) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    waiting.push(path);
                } else if path.extension().is_some_and(|kind| kind == "rs") {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        let name = path
                            .strip_prefix(folder)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        out.push((name, text));
                    }
                }
            }
        }
        assert!(!out.is_empty(), "components/ should hold some source");
        out
    }

    /// And the test's own reading is checked, so it cannot pass by finding nothing.
    #[test]
    fn a_literal_is_told_from_a_colour_that_was_handed_in() {
        assert!(writes_a_literal("from_rgb(0x1A, 0x1F, 0x26)"), "a hex triple is a literal");
        assert!(writes_a_literal("from_rgb(26, 31, 38)"), "and so is a decimal one");
        assert!(writes_a_literal("from_rgb( 0xFF, 0, 0)"), "however it is spaced");
        assert!(
            !writes_a_literal("from_rgb(colour.r, colour.g, colour.b)"),
            "this is a conversion"
        );
        assert!(!writes_a_literal("from_rgb(found.r, found.g, found.b)"));
        assert!(!writes_a_literal("from_rgb(mix(a), mix(b), mix(c))"));
    }
}
