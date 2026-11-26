# UI revision plan

Status: plan, not started. Written from the owner's WizTree screenshots (menus, Options, Tweaks, Colors, filter bar, Tree View) and the earlier WizTree capture. Nothing here is verified against a running TimberStat build yet.

Mode: Operate (desktop analysis tool). Scanability, consistency and native conventions come first. Brand shows in precise details, not decoration.

## 1. What the owner said is wrong today

- Text is too large, and there is too much empty space around it.
- Sections cannot be moved or resized.
- The treemap detail is weak: no folder size headers, no file labels, one hue per top-level folder.
- The file tree had no tooltips (path, size, allocated). Added in the working tree; build and run not yet verified.
- Graph pane overflowed its border with the pie legend. A scroll wrapper was added; not yet confirmed on screen.

## 2. What WizTree does that the plan adopts

| Area | WizTree | TimberStat target |
|---|---|---|
| Top bar | Drive select, Scan button, filter icon, selection summary, version and donate block | One compact toolbar: drive select, Scan, filter toggle, Fast NTFS switch, selection summary, menu button |
| Views | Tree View / File View / About tabs | Tree View (default), Files (flat table), About |
| Folder tree | Folder, % of Parent, Size, Alloc, Items, Files, Folders, Modified, Attributes; column dividers | Same columns, in the order above. Column visibility set in Options |
| Extension panel | Colour swatch, Extension, File Type, Percent bar, Size, Alloc, Files | Right-hand panel, same columns. Clicking a row filters the tree and treemap |
| Treemap | Folder header strips with name and size; file labels `name (size)`; colour by Size or by Extension group | Same. Labels drop out when a tile is too small |
| Colours | 13 slots, each a set of editable extensions (Colors dialog) | 13 default groups from the owner's dialog; editable later |
| Options | Units (Dynamic, Byte, KB, MB, GB, TB), decimal places, treemap toggles | Same units and decimal places. Treemap toggles below |
| Filter | Include / Exclude fields, Match file name only or entire path, Clear, Apply | Same fields. Apply runs the filter. No live filtering that the owner did not ask for |

Not adopted, with reasons:

- **Always Run as Administrator**: declined. The owner's rule is that elevation only comes from the explicit Fast scan button. Normal scanning never asks for it.
- **Delete (Recycle Bin / permanent)**: not in scope. This is an analyser. Destructive file operations are deferred until the owner asks, and would need their own confirmation design.
- **Dump MFT File**: diagnostic only. Behind a Diagnostics entry, not in the main menu.
- **Enter Supporter Code / Donate**: not applicable.

## 3. Target layout

```
+--------------------------------------------------------------------------------+
| TimberStat  [W:\ v] [Scan]  [Fast NTFS o]  [filter]   W: 213.4 of 230.7 GB  ⋯ |  toolbar, 36px
+--------------------------------------------------------------------------------+
| [Tree View] [Files] [About]                                                    |  tabs
+------------------------------------------+-------------------------------------+
| Folder            % Parent  Size  Alloc  | Ext    Type      %       Size  ...  |
| > GTA              97.8%   208.6  ...    | [■] .rpf  Archive  ▇▇▇▇  70.9 GB    |
|   > data                                 |                                     |
|   > levels.rpf                           |   extension panel (resizable)       |
+------------------------------------------+-------------------------------------+
|  ◂ drag handle ▸                                                               |
| [Treemap] [Sunburst] [Pie]                                                     |
| [W:\ > WTP (213.4 GB) > GTA\ (208.6 GB) > ...]                                 |
|   graph pane (resizable height)                                                |
+--------------------------------------------------------------------------------+
| Walk complete: 12,103 files, 1,296 folders           Logical | On-disk   Fast  |  status bar, 24px
+--------------------------------------------------------------------------------+
```

Regions:

1. **Toolbar** (top): drive select, Scan, filter toggle, Fast NTFS switch, summary, menu. Every control is outlined so it reads as a control.
2. **View tabs**: Tree View, Files, About. These switch whole views, not graph modes.
3. **Left: folder tree**, the main area. Columns with vertical dividers.
4. **Right: extension panel**. Shows file types for the focused folder. Togglable (Show File Types, F8).
5. **Bottom: graph pane** with Treemap, Sunburst and Pie tabs. Togglable (Show Treemap, F9).
6. **Status bar**: last scan result, PARTIAL warning where totals are incomplete (Plan §17), size basis, mode.

The graph pane tabs stay. The owner asked for tabs there.

## 4. Pane behaviour (the "can't move or resize" complaint)

- Split panes: tree | extension panel (vertical), and views | graph pane (horizontal).
- Each divider drags. Minimum sizes so no pane can close to nothing; a pane can be hidden with its toggle instead.
- Sizes persist in config (config-rs, `%APPDATA%\TimberStat\config\config.json`).
- Before writing a splitter, check whether gpui-kit or gpui-component already ships a resizable split. Use it if so. Do not hand-roll one until that check is done.

## 5. Type and spacing (starting points)

These apply where the project has no spacing scale. Replace them with the theme tokens from gpui-kit once they exist.

- Base text 13px. Labels 12px. Section headings 14px. Nothing larger in the main area except the app name.
- Tree row height 22px. Toolbar height 36px. Status bar 24px.
- Padding 8px inside a group, 16px between groups (2× rule).
- Controls: 12px between adjacent outlined buttons; no separator lines where space works.
- No empty stretch longer than the content it sits beside. Pane sizes take the space; the tree does not float in a fixed box.

## 6. Treemap detail

- Folder header strip with name and size, e.g. `data\ (56.8 GB)`. Shown when the strip fits.
- File labels `name (size)` when the tile is large enough. Hidden otherwise. The full text is in the tooltip.
- Colour by **Size** (gradient) or by **Extension group** (the 13 slots). A Colors dialog edits the groups later.
- Options: Show Allocated Size on Treemap; Show Folder and File Names on Treemap; Enable gradient colours; Show Free Space on Treemap (only if free space is known for the volume).
- File grouping: files below a minimum size are gathered into one "N small files" tile per folder. Minimum size setting, 0 disables. **Unit of the owner's `4` is unconfirmed**: WizTree's dialog does not state it. Ask before choosing the unit.
- Zoom in and zoom out (F10 / F11) as a history of focused folders, not a second view.

## 7. Menus

- **Toolbar menu (⋯)**: Options, Tweaks, Colors, Language, Check for updates, About.
- **Options**: units (Dynamic, Byte, KB, MB, GB, TB), decimal places (default 1), show/hide File Types and Treemap, treemap toggles, colour mode.
- **Tweaks**: drag and drop (later), double-click opens file (later), monitor file system (later), folder last-modified from newest child, MFT scan on folder selection (default off, with WizTree's note that it is normally only useful for whole drives), WizTree classic theme (not planned), horizontal grid lines, alternate row colours, gradient colours, decimal places, file group minimum size.
- **Tree context menu** (right-click): Select Folder, Expand / Reset expansion, Select Parent, Zoom in / out treemap, Explore Folder, Command Prompt Here, Copy Path, Copy size info, Export to CSV.
- **Tab / Diagnostics**: Open log folder, Dump MFT (only in Diagnostics).

Keyboard shortcuts follow WizTree where the action is the same (F5 scan, F3 select folder, F4 reset expansion, F6 select parent, F8/F9 panes, F10/F11 zoom, Ctrl+E explore, Ctrl+Alt+C copy path). Shortcuts are listed next to their menu items.

## 8. Filter

- Include and Exclude fields, with Match file name only or Match entire path.
- Apply Filter runs it. Clear resets it.
- Our search field stays as the quick filter. The filter panel is for include and exclude rules. Two controls, two purposes, no overlap.

## 9. Tooltips

- File tree row: full path, logical size, allocated size (on-disk shows "not recorded by this scan" on a walk). Implemented in the working tree.
- Treemap tile: name, size, share of folder (exists).
- Pie slice: size and share (exists).
- Toolbar buttons that are disabled: a one-line reason (e.g. "On-disk sizes need a Fast scan").

## 10. Phases and checks

1. **Pane system and type scale.** Resizable panes with persisted sizes; compact type and row heights. Check: panes drag and stay within minimums; restart keeps sizes; screenshot shows no empty stretch.
2. **Tree columns, extension panel, tooltips.** Check: columns align under headers at 1400px and at the narrowest width; tooltip shows path, size and allocated; search still filters.
3. **Treemap detail.** Headers, labels, colour modes, small-file grouping. Check: labels appear only when they fit; grouping total equals the folder total.
4. **Toolbar, menus, Options and Tweaks.** Check: each menu item does something real or is absent; no disabled placeholder without its reason.
5. **Filter panel.** Check: include and exclude both applied on Apply; Clear restores the tree.
6. **Export and Save PNG**: deferred until the owner asks.

Every phase: `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, a live screenshot, and source files under 500 lines (the owner's rule).

## 11. Questions for the owner

1. The unit of the file-grouping minimum (`4`): MB, or something else?
2. Should the extension panel be a right-hand pane (as above), or a tab in the graph pane?
3. Keep Dump MFT in Diagnostics, or drop it?
4. Confirm declining Always Run as Administrator, which keeps the elevation rule.

## 12. Not verified

- WizTree behaviour is from screenshots only. Cua could not click WizTree (UIPI: WizTree runs elevated). Tooltip wording for toolbar controls is unknown.
- Pane resizing support in gpui-kit is not yet checked.
- The current build has the file-tree tooltip change, which has not been built or run.
