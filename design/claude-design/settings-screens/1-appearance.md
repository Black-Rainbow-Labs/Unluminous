Settings screen 1 of 9: Appearance & Behavior › Appearance
Drawn by crates/unluminous-app/src/components/settings_dialog.rs (appearance_page). Keys are lines in the settings file.

The modal around every screen: 900 by 680. Header "Settings — <project>" with a round close cross. Left nav 258 wide: a "Search settings" field, then pages under group headings in this order: Appearance & Behavior (Appearance, Theme), Editor (Editor), Plugins (no heading), Tools (Terminal, MCP), Plugins (Agent-Chat, Agent-Tasks, Database). Breadcrumb "Group › Page" at the top of the page; a page taller than the body scrolls. Footer: "Changes take effect at once." and a "Done" button. There is no Apply.

Each setting: Label | control | options or range | default | key

**Font**
- Family: | dropdown | every font on this machine, "System default" when empty | empty | appearance.font.family
- Size: | dropdown | 9, 11, 13, 16, 20, 24, 32, 48, 64 (file accepts 6 to 144) | 16 | appearance.font.size
Help: "The font the editor sets every open file in. Bold, italic and colour stay as they were," / "and the size is also on the keyboard at command or control with plus and minus."

**Interface**
- Family: | dropdown | "The editor's", then every font on this machine | The editor's (empty) | appearance.ui.font.family
- Size: | dropdown | 10.0, 11.0, 12.5, 14.0, 16.0, 18.0, 20.0 (file accepts 8 to 24) | 12.5 | appearance.ui.font.size
Help: "The menus, the rail, the explorer and the status bar. The editing area keeps its own font above."

**Background**
- Opacity: | slider with a percentage beside it | 5% to 100% | 86% | appearance.background.opacity (0.05 to 1.0)
Help: "Fades the window so what is behind shows through. Text stays fully solid at every setting."
- Behind: | button showing the current choice; opens the background picture grid (a separate modal) | "Show Contents Underneath" (empty) or a picture file name | forest-1.jpg on a fresh install | appearance.background.image
Help: "A picture behind the window, or the desktop. Pictures chosen here are kept in Unluminous's own folder, so moving the original does not lose it."

**Plugin panes**
- Draw depth in plugin panes | tick box | | on | plugins.chrome
Help: "Soft shadows, gradients and pressed edges behind a plugin's own pane, drawn on the processor. Off, a plugin draws flat, which costs nothing at all."
