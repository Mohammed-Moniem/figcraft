# Repeated menu option changes

Preferences offers **Keep menus open when toggling options**, off by default. When enabled,
clicking a checked option in the in-app main menu leaves that menu and its open submenu available
for another change. View options and the checked Preferences rows use their current state.
Ordinary actions close the menu; Escape and a click outside dismiss it.

The command `pref.keepMenuOpen` accepts `{on: true}` or `{on: false}`. Omitting `on` toggles the
setting; it is not a read operation. A changed value requests preference persistence, while an
idempotent setter preserves existing pending work without adding a write request. Native settings
and the browser's selective appearance record retain the choice. Existing records without the
field default to off. Document content, dirty state and Undo are independent of this setting.

This applies to the in-app main menu. Native operating-system menus retain their platform
behavior, and context menus and other popovers retain their existing behavior. The preference is
also available as a command in the Actions palette. Original command, actual egui pointer and
browser-codec regressions cover the behavior; launched native/browser acceptance remains separate.
