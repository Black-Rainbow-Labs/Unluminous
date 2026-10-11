Settings screen 6 of 9: Plugins
Drawn by crates/unluminous-app/src/components/plugins_page.rs. In the nav it has no group heading and sits between Editor and Tools. It is a plugin list, not a page of settings: nothing on it is written to the settings file.

- Two tabs: Marketplace and Installed (Installed shows a count).
- A "Search plugins" field.
- A list of plugins on the left, 300 wide.
- The chosen plugin's page on the right: vendor, an action button, ENABLE / DISABLE, an "Overview" heading and a "What it does not do" heading.
- The action button says INSTALL, CUSTOMISE (a bundled plugin not yet written out to its folder) or UNINSTALL. The line under it is one of:
  - "Installed with Unluminous and working. Customising writes its folder out where it can be edited by hand."
  - "Its folder is under the settings folder and can be edited by hand. Uninstalling removes it."
  - "Its folder is under the settings folder and can be edited by hand. Uninstalling takes that folder away and puts the plugin back to the one Unluminous ships."
