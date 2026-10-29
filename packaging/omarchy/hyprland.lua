-- Omaspec on Omarchy. Copy what you want into ~/.config/hypr/bindings.lua
-- (the binding) and ~/.config/hypr/hyprland.lua (the window rules).

-- SUPER+SHIFT+U opens the editor (U for UML; free in Omarchy's defaults).
o.bind("SUPER + SHIFT + U", "Diagram editor", "uwsm-app -- omaspec")

-- SUPER+F stays Omarchy's fullscreen of the whole window. The diagram
-- gets a window of its own with Ctrl+Shift+N inside the editor.

-- The diagram in its own window (Ctrl+Shift+N in the editor) and the
-- preview window (`omaspec preview FILE`) share the class
-- omaspec-preview. Put them on another workspace, for example the one on
-- your second monitor:
-- o.window("^(omaspec-preview)$", { workspace = "2" })

-- Or let them float beside the terminal running Neovim:
-- o.window("^(omaspec-preview)$", { float = true, size = "45% 80%" })
