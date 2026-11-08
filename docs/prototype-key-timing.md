# Keyboard and controller trigger phases

The Prototype interaction editor offers **Key/Gamepad** (the existing immediate press),
**Key down** and **Key up**. Record one key with modifiers or choose one controller button.
The two new phases have a **Minimum hold** field in milliseconds, from 0 to 600000.

`proto.interaction.add` and `proto.interaction.set` accept:

```json
{"trigger":{"type":"keyDown","key":"Shift+A","ms":500}}
{"trigger":{"type":"keyUp","key":"Gamepad A","ms":500}}
```

`ms` defaults to zero. A down action fires once when the continuously held main key/button
reaches the minimum. An up action fires on its release only if that hold reached the minimum;
it does not wait another interval after release. Releasing a modifier first does not lose the
main key's paired release. Repeats do not restart or repeat a hold.

Pending inputs belong to the presenter and the interaction that acquired them. Source changes,
restart/navigation, focus loss, inactive input scopes and replacement overlay instances retire
them. A base interaction may pair its release with an overlay that its down action opened.
Controllers keep their pointer-independent preview scope. Keyboard input follows the preview's
existing pointer scope. A positive minimum hold cannot bypass the prototype link notice as a
fresh request to open a URL.

These phases are FigCraft extensions. Native `.figcraft` files preserve them. `.fig` export maps
zero-duration key down to the format's standard key-down event; timed down and all key up
interactions are omitted with a warning. Penpot export reports unsupported key triggers through
its existing warning. Static image/vector exports have no interactive playback. The existing
control-channel `gamepad` pulse carries no release or duration and remains a legacy press;
continuous controller phases use the platform's held-button polling.
