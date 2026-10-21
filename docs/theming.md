# Theming

The interface takes its colours, corner radii and page transition from a built-in theme, with your
own file over it:

```
~/.config/slatty/theme.toml
```

## Built-in themes

**Settings → Appearance → Theme** offers six complete colour sets: Slatty (the default),
Carbonfox, Everforest, Gruvbox Dark, and two light ones, Pastel Glow and Gruvbox Light. The theme
file is read over the one picked: a key it sets wins, the others come from the built-in theme.

## The theme file

In **Settings → Appearance → Theme file**:

- **Create theme file** writes the look in use (the built-in theme picked) to the file, every key
  with its value, as a starting point. Pick a built-in theme first to start from it.
- **Edit** opens the file in your desktop's text editor.
- **Reload** applies it without restarting.

A mistake (a misspelled key, a colour that is not one, a number out of range) is named in the
interface. On **Reload**, the look in use stays, so a typo while editing changes nothing on
screen. At start, or when you pick another built-in theme, the file is not used and the built-in
theme shows alone until the mistake is fixed.

## Font

The font is chosen in the interface: **Settings → Appearance → Font**. The default is Geist, built
into Slatty so the look is the same on every system; the menu also lists the font families
installed on the system, applied at once. A family without a semibold weight shows bold where the
interface uses semibold.

## Format

Every key is optional: one left out keeps the built-in theme's value.

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
radius = 8                  # Corners of controls: buttons, fields, menus, tabs
cover_radius = 6            # Corners of game covers

[motion]
transition_ms = 180         # Page transition; 0 turns it off
transition_rise = 8         # How far a new page rises, in pixels
```

Colours are `#rrggbb`, or `#rrggbbaa` with transparency. Radii and the rise are in pixels, and must
be numbers (not `nan` or `inf`); a transition lasts at most 10,000 ms. Cards, drawers and dialogs
keep corners of their own, which the file does not change yet.

## Example

A theme that only changes the accent and squares the controls and covers:

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
widget style reads the tokens in use, so a key added to `Tokens` and to the file format reaches the
whole interface. A new token also goes into this page.
