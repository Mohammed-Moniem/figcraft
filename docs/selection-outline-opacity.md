# Selection outline opacity

Preferences → **Selection outline opacity** controls the opacity of selected outlines, transform
handles and size labels. The default is 100%; choose a lower percentage to see the artwork more
clearly while editing its stroke or corners. Zero makes this selection chrome invisible. **Reset
selection outline opacity** restores 100%.

The selection, property panel and transform hit areas keep working. The existing View →
**Selection outlines** toggle still hides or shows the chrome independently, using the chosen
percentage when shown. Text carets, vector editing points, ordinary unselected hover, guides,
frame labels and live drop indicators keep their normal appearance.

This app preference applies to all documents and windows and is saved with other preferences.
It does not change layer opacity, document pixels, exports, file dirty state or Undo history.
An actual property edit still has its usual Undo step. Old preference files keep 100%; malformed
saved values fall back to that visible default.

The Actions palette entry opens Preferences. Scripts use the same engine command without opening
a dialog:

```json
{"command":"pref.selectionOutlineOpacity","params":{"opacity":30}}
```

`opacity` is a finite percentage from 0 to 100, including fractions. Empty parameters query the
current percentage. `{"reset":true}` restores 100%; do not combine it with an opacity value.
