Settings screen 2 of 9: Appearance & Behavior › Theme
Drawn by crates/unluminous-app/src/components/settings_dialog.rs (theme_page). Keys are lines in the settings file.

Each setting: Label | control | options or range | default | key

**Theme**
- (no label) | a list of rows, one per installed theme: the theme's name, the plugin it came from in faint text, and six colour swatches on the right (editor, explorer, accent, git added, unsaved, close). The chosen row is a filled pill | Unluminous Dark, Unluminous Light, and every theme a theme plugin carries (themes-bundle-1: Islands Dracula Colorful, Material Palenight, Material Deep Ocean, Monokai Pro, One Dark) | Unluminous Dark (stored as empty) | appearance.theme ("<plugin>/<theme>")
Help: "A theme says what every colour in Unluminous's own palette means. One that names the nine token colours also recolours code, in every language at once, in the same frame."

**Accent**
- Accent: | eight round swatch buttons in a row, 24 points each, each with its name as a tooltip; the chosen one has a ring | "The theme's own" (drawn hollow), Accent, Unsaved, Added, Modified, Agent, Blame, Close. The colours are the active theme's own | The theme's own (empty) | appearance.accent (#RRGGBB)
Help: "One colour for everything the accent means: the caret, the open tab, an open folder and the line a program is stopped on. The first is whatever the theme chose."

**Icons**
- Icon set: | dropdown | Follow the theme, Classic, Material | Follow the theme (empty) | appearance.icons (material, classic or empty)
Help: "Which drawn marks the rail buttons, the folder arrow and the small controls use. Material draws a chevron where the classic set draws a triangle, and puts a folder in front of a folder's name."
