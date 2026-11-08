# Installed fonts with a built-in family and style

Installed faces remain available when FigCraft already embeds that family and style. Desktop
scans retain those entries; browser Local Font Access requests the matching installed face even
while a built-in face can draw the text. When readable bytes arrive, the installed match wins.
Until then, or if they cannot be read, the built-in face remains available.

For a requested style, a matching installed face or its named variable-font instance takes
precedence over a matching built-in face. An absent installed style still uses a matching
built-in style. If neither supplies the requested style, nearest weight/italic matching remains;
installed faces win equal-distance ties and retain their original order. A repeated browser
handle does not add a duplicate face. Family aliases in a native collection continue to share
the same underlying file bytes.

This is the installed-font part of #2092. Explicitly loaded-font priority is a separate change
being combined with this one: explicit loads must remain ahead of installed faces. This producer
alone does not establish that combined behavior. Font digest matching is not implemented, and
the reporter's exact Inter3.19 widths and Windows installation path have not been validated.

This follows the issue's requested behavior and can change canvas text measurements on a
computer with a conflicting installed font. It is not a claim of Figma's Inter policy: the
[public conflicting-fonts guide](https://help.figma.com/hc/en-us/articles/4403175325719-Manage-conflicting-fonts)
gives installed fonts priority generally, while describing a built-in-first Inter exception.

The local producer reproduced five failures with unchanged production and two fallback controls.
A further named-instance regression failed against the first repair. The repaired producer's
187 text tests pass, including installed selection, browser deduplication, unreadable fallback,
built-in style fallback and shared collection bytes. Existing approved font bytes are renamed
only inside isolated test databases; no font file is added or installed. Combined validation
and platform-specific runtime checks remain pending.
