Settings screen 4 of 9: Tools › Terminal
Drawn by crates/unluminous-app/src/components/settings_dialog.rs (terminal_page). Keys are lines in the settings file.

Each setting: Label | control | options or range | default | key

**Font**
- Size: | dropdown | 10, 11, 12, 13, 14, 16, 18, 20 (file accepts 6 to 48) | 13 | terminal.font.size
Help: "The size of one cell in the terminal grid. Changing it tells the running program the new size."

**Shell**
- Program: | text field, 240 wide | a program name or path | empty (PowerShell on Windows, $SHELL elsewhere) | terminal.shell
Help: "The program each tab runs, started in the folder the explorer is showing." / "Leave it empty for PowerShell." (On macOS and Linux: "Leave it empty for the shell named in $SHELL.")

**Where a tab reopens**
- Ask PowerShell where it is | tick box | | off | terminal.shell_integration
Help: "A tab reopens in the folder its shell was in, read off the shell's own process. That answers for cmd.exe, bash and zsh and cannot answer for PowerShell: Set-Location moves PowerShell's location and never the process's current directory." / "On, Unluminous adds one line to the prompt, after your own profile has set it up, so the shell says where it is. Off, nothing about your shell is changed. A shell that already reports its folder is followed either way."

**Not built yet**
- Settings search does not find the "Where a tab reopens" section by name (settings.rs, Page::sections).
