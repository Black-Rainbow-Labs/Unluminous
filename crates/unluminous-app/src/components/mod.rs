//! The pieces of the window, one file each.
//!
//! Every component is a function that takes a `Ui` and the rectangle it is to fill, draws itself, and
//! returns what the user did in it. **None of them changes the document**, so the state changes in one
//! place, `app`, and two components cannot disagree about what happened.
//!
//! There are two exceptions and both are about the *settings* rather than about the document, which is
//! the line `task-1984` drew when it found this paragraph claiming more than the code does.
//! `settings_dialog` and `mcp_page` are handed `&mut Settings` and write it, because a settings page
//! whose every control had to be routed back through `app` would be a hundred outcomes carrying one
//! value each, and the page is the one place in the window where what is drawn *is* the state. They
//! write the value and nothing else: saving it to disk, putting a font into effect and telling the
//! plugins are all still `app`'s, which is why `UnluminousApp::set_the_font_everywhere` exists.
//!
//! A pane a plugin contributes is not an exception. It returns an outcome and `app` acts on it, which
//! is the rule above with a provider in the middle.

pub mod about_dialog;
pub mod activity_bar;
/// The Agent-Chat pane: the panel, the conversation, the composer and its Settings page.
pub mod agent_chat;
/// The Agent-Tasks board, which is the first plugin that draws. Nothing in it decides anything: the
/// lanes, the drag and the search are `services::agent_tasks`.
pub mod agent_tasks;
pub mod ai_page;
pub mod background_dialog;
pub mod branch_widget;
pub mod browser_view;
pub mod chrome_kit;
pub mod color_wheel;
pub mod command_palette;
pub mod completion;
pub mod context_menu;
pub mod controls;
pub mod database;
pub mod debug_dialogs;
pub mod debug_panel;
pub mod designer;
pub mod diagram_view;
pub mod dock;
pub mod editor_view;
pub mod explorer;
pub mod file_tabs;
pub mod find_bar;
pub mod find_in_files;
pub mod git_dialogs;
pub mod git_panel;
pub mod go_to_file;
pub mod gutter;
pub mod logo;
pub mod markdown_text;
pub mod skills;
// Settings -> Keymap, which chord each menu entry is on. `task-2286`.
pub mod keymap_page;
pub mod mcp_page;
pub mod menu_bar;
pub mod microphone_dialog;
pub mod modal;
pub mod new_project_dialog;
pub mod notebook_view;
pub mod picture_view;
pub mod plugins_page;
/// The project tabs in the title bar and the dropdown the plus opens - `task-2322`.
pub mod project_tabs;
pub mod prompt_dialog;
/// The Realm — `task-1904`.
pub mod realm;
pub mod references;
pub mod report_dialog;
pub mod resize_edges;
pub mod run_dialog;
pub mod run_panel;
pub mod run_widget;
pub mod save_dialog;
pub mod scrollbar;
pub mod settings_dialog;
pub mod settings_kit;
pub mod setup_dialog;
pub mod shortcut_gallery;
pub mod signature;
pub mod splitter;
pub mod spreadsheet;
pub mod status_bar;
pub mod templates_dialog;
pub mod terminal_panel;
pub mod text_menu;
pub mod text_tools;
pub mod title_bar;
/// Dismissible notices over the bottom right of the window: what a plugin sends when somebody has to
/// see it, rather than the status bar's running commentary.
pub mod toast;
pub mod unrenderable_view;
pub mod value_tooltip;
pub mod voice_page;
pub mod word;
