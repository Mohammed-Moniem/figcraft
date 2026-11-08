# Explicit loads and installed fonts

Equally exact style labels and actual named variable-font instances use one source order: the latest explicit load, then installed faces, then built-in faces. Nearest weight/italic quality remains the first choice when no exact style is available; source priority only breaks equal quality. Stable ties preserve the existing scan order.

A pending installed style remains indexed beside an explicit load but is not requested merely to replace an equally matching loaded face. A closer pending style can still fill a missing style. Readable installed responses retain installed priority, and unreadable responses retain the usable loaded or built-in fallback.

This sparse proposal composes the already validated local loaded-variable repair with the independently supplied installed-font producer. It preserves that producer's nine tests and adds eleven composition tests using isolated databases and existing approved bytes. The composition has not been compiled or executed in this preparation. Optional craft-fonts absence makes stand-in fixtures skip; actual validation must establish that the font bytes were available.

No operating-system font is installed. Font digest matching, the reporter's Windows installation path, exact Inter widths, and browser permission behavior are unverified. Installed Inter precedence follows the requested local policy and is not a Figma Inter-policy parity claim.
