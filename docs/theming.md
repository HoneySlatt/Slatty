# Theming

The interface takes its colours, corner radii and page transition from one file:

```
~/.config/slatty/theme.toml
```

Without that file, SlattyLauncher uses its own look. **Settings → Appearance → Create theme file**
writes the default theme there, every key with its value, as a starting point. **Edit** opens the
file in your desktop's text editor, and **Reload** applies it without restarting.

The font is chosen in the interface instead: **Settings → Appearance → Font** lists the font
families installed on the system and applies the one picked at once. A family without a semibold
weight shows bold where the interface uses semibold.

## Format

Every key is optional: one left out keeps its default. A mistake (a misspelled key, a colour that
is not one) keeps the look in use, and the interface names the mistake.

```toml
[colors]
background = "#14131d"      # Window background
surface = "#1e1c2a"         # Cards, drawers
surface_high = "#2a2739"    # Fields, hovered controls
outline = "#353146"         # Borders, dividers
accent = "#c4b1fa"          # Primary buttons, selection
accent_hover = "#d3c4fc"
on_accent = "#1b1530"       # Text on the accent colour
text = "#f2f0fa"
muted = "#a19db5"           # Secondary text
success = "#4ade80"
warning = "#fbbf24"
danger = "#f87171"
danger_hover = "#fca5a5"
error_surface = "#3b1d24"   # Background of error notices
scrim = "#05040ab3"         # Darkens the page behind dialogs (#rrggbbaa)

[shape]
radius = 8                  # Corners of controls
cover_radius = 6            # Corners of game covers

[motion]
transition_ms = 180         # Page transition; 0 turns it off
transition_rise = 8         # How far a new page rises, in pixels
```

Colours are `#rrggbb`, or `#rrggbbaa` with transparency. Radii and the rise are in pixels.

## Example

A theme that only changes the accent and squares the corners:

```toml
[colors]
accent = "#7dd3fc"
accent_hover = "#a5e1fd"
on_accent = "#0c1a24"

[shape]
radius = 0
cover_radius = 0
```

## For developers

The file maps onto `theme::Tokens` in `crates/gui/src/theme.rs`. Views never name a colour; every
widget style reads the tokens in use, so a key added there and to the file format reaches the whole
interface.
