# Omaspec's colors for every Omarchy theme.
#
# Copy this file to ~/.config/omarchy/themed/omaspec.toml.tpl and switch
# themes once (or run omarchy-theme-refresh). On every switch Omarchy fills
# in the placeholders and writes the result next to the theme's colors.toml,
# where Omaspec reads it; `omaspec theme` shows what it took.
#
# Every line is optional: what is missing comes from colors.toml as before.
# The placeholders are Omarchy's: {{ accent }}, {{ background }},
# {{ foreground }}, {{ red }} … {{ bright_blue }}, {{ color0 }} … {{ color15 }},
# and {{ mix background foreground 40% }} for a shade in between.

[palette]
# Any name of colors.toml, for Omaspec only. For example a calmer accent:
# accent = "{{ blue }}"

[syntax]
# The colors of the source text.
keyword = "{{ accent }}"
arrow = "{{ cyan }}"
string = "{{ green }}"
message = "{{ green }}"
activity = "{{ bright_foreground }}"
comment = "{{ mix background foreground 45% }}"
directive = "{{ magenta }}"
preprocessor = "{{ magenta }}"
color = "{{ orange }}"
number = "{{ orange }}"
variable = "{{ bright_blue }}"
section = "{{ yellow }}"
stereotype = "{{ blue }}"
note = "{{ light_foreground }}"

[marks]
# Beside the line numbers: changed since the last save, different from the
# version compared with, lines removed.
changed = "{{ accent }}"
compared = "{{ blue }}"
removed = "{{ red }}"
