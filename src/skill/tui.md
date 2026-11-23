# tome terminal browser

## Modes

- **Browse** — read-only reports (update and removal results).
- **Manage** — the `list` inventory: mark and remove in place.
- **Select** — install, remove, and export pickers.
- **Review** — deletion confirmation for `tome remove`.

## Keys

| Key | Action |
| --- | --- |
| Arrows / j / k, PgUp / PgDn, Home / End | Move through rows or details |
| / | Focus the filter; every word must match name, description, source, version or status |
| Esc in filter | Back to the list, keeping the query |
| Ctrl-U / Alt-Space in filter | Clear the query / type a space |
| Space | Toggle the focused mark (Select, Manage) |
| a / n | Mark / clear every match, including rows outside the viewport |
| Tab | Show only marked rows; hidden marks are kept and counted |
| x / Delete (Manage) | Remove marked rows now; the focused row when nothing is marked |
| d, then Esc | Full scrollable details, then back |
| Enter | Picker: continue. Manage/Browse: close. Manage filter: leave the filter |
| q / Esc / Ctrl-C | Close or cancel; Ctrl-C also works while searching |
| y / b (Review) | Confirm removal / back to selection |

## Manage specifics

- Checkboxes and `N marked` appear once something is marked.
- Footer shows `x: remove N marked | n: clear`, or `Removed N skill(s)` after a
  removal; the notice clears on the next keypress.
- Untracked rows refuse marks with a notice — tome removes only what it owns.
- After a removal the list reloads in place, keeping the filter, surviving
  marks and the cursor.

## Review specifics

- Lists the entire deletion set; searching never narrows it. Only `y` removes.

## Limits

- Below 24 columns or 11 rows the browser shows a resize hint and accepts only
  cancellation.
