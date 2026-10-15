extends RefCounted

## Gives the game input the way a device does: an action, a key or a mouse button pressed and
## released as the very events a device produces, handed to `Input.parse_input_event()`, so game
## code reads the same intents it always reads and cannot tell the difference. Nothing here writes to
## a game's own state or looks at its nodes: input is the only way in. A press can be held until
## released, or released after a number of ticks of stepping; what is held is always known, and can
## be released at once.

## Mouse buttons by the name a driver uses.
const BUTTONS := {
    "left": MOUSE_BUTTON_LEFT,
    "right": MOUSE_BUTTON_RIGHT,
    "middle": MOUSE_BUTTON_MIDDLE,
}

## What is held: `"<kind>:<name>"` -> `{kind, name, release_at}`, where `release_at` is the tick
## count at which a scheduled release falls due, or `-1` for a press held until released.
var _held: Dictionary = {}
var _pointer: Vector2 = Vector2.ZERO
var _button_mask: int = 0

## Presses an input named by `args` (one of `action`, `key`, `button`), holding it until released, or
## for `args.ticks` ticks of stepping when that is given. `now` is the stepping tick count. Returns
## `{held}` or `{error}`.
func press(args: Dictionary, now: int) -> Dictionary:
    var input := _resolve(args)
    if input.has("error"):
        return input
    var release_at := -1
    if args.has("ticks"):
        var ticks: Variant = args.ticks
        if not _is_whole(ticks) or int(ticks) < 1:
            return {"error": "`ticks` must be a positive integer."}
        release_at = now + int(ticks)
    _send(input.kind, input.name, true)
    _held["%s:%s" % [input.kind, input.name]] = {
        "kind": input.kind, "name": input.name, "release_at": release_at
    }
    return {"held": held()}

## Releases an input named by `args`.
func release(args: Dictionary) -> Dictionary:
    var input := _resolve(args)
    if input.has("error"):
        return input
    _release_one("%s:%s" % [input.kind, input.name])
    return {"held": held()}

## Presses an input and schedules its release after `args.ticks` ticks (`default_ticks` when none is
## given). The game sees a press on the second tick after it was given, the same as a device event's
## delivery, so a tap of two ticks is seen down for exactly two ticks.
func tap(args: Dictionary, now: int, default_ticks: int) -> Dictionary:
    var with_ticks := args.duplicate()
    if not with_ticks.has("ticks"):
        with_ticks["ticks"] = default_ticks
    return press(with_ticks, now)

## Releases everything held; returns how many inputs that was.
func release_all() -> int:
    var released := _held.size()
    for id: String in _held.keys():
        _release_one(id)
    return released

## Releases every scheduled press whose tick has come, at tick count `now`.
func release_due(now: int) -> void:
    for id: String in _held.keys():
        var release_at: int = _held[id].release_at
        if release_at >= 0 and now >= release_at:
            _release_one(id)

## What is held, as `"<kind>:<name>"` strings.
func held() -> Array:
    return _held.keys()

## Moves the pointer to viewport pixel `(x, y)`, or by `(x, y)` when `args.relative` is true, as a
## mouse motion event. Returns `{position}` or `{error}`.
func pointer(args: Dictionary) -> Dictionary:
    if (
        not (args.get("x") is float or args.get("x") is int)
        or not (args.get("y") is float or args.get("y") is int)
    ):
        return {"error": "`x` and `y` must be numbers."}
    var given := Vector2(float(args.x), float(args.y))
    var event := InputEventMouseMotion.new()
    if args.get("relative", false):
        event.relative = given
        _pointer += given
    else:
        event.relative = given - _pointer
        _pointer = given
    event.position = _pointer
    event.global_position = _pointer
    event.button_mask = _button_mask
    Input.parse_input_event(event)
    return {"position": [_pointer.x, _pointer.y]}

## The one input `args` names: `{kind, name}`, or `{error}` when none or several are named or the
## name is not one the game knows.
func _resolve(args: Dictionary) -> Dictionary:
    var named: Array[String] = []
    for candidate: String in ["action", "key", "button"]:
        if args.has(candidate):
            named.append(candidate)
    if named.size() != 1:
        return {"error": "Name exactly one of `action`, `key` or `button`."}
    var kind: String = named[0]
    var name: Variant = args[kind]
    if not name is String or (name as String).is_empty():
        return {"error": "`%s` must be a name." % kind}
    match kind:
        "action":
            if not InputMap.has_action(name):
                return {"error": "No action named '%s'." % name}
        "key":
            if OS.find_keycode_from_string(name) == KEY_NONE:
                return {"error": "No key named '%s'." % name}
        "button":
            if not BUTTONS.has(name):
                return {"error": "No mouse button named '%s' (left, right, middle)." % name}
    return {"kind": kind, "name": name}

func _release_one(id: String) -> void:
    if not _held.has(id):
        return
    var entry: Dictionary = _held[id]
    _held.erase(id)
    _send(entry.kind, entry.name, false)

## Builds the device event for an input and hands it to the engine's input path.
func _send(kind: String, name: String, pressed: bool) -> void:
    match kind:
        "action":
            var action := InputEventAction.new()
            action.action = name
            action.pressed = pressed
            action.strength = 1.0 if pressed else 0.0
            Input.parse_input_event(action)
        "key":
            var code := OS.find_keycode_from_string(name)
            var key := InputEventKey.new()
            key.keycode = code
            key.physical_keycode = code
            key.pressed = pressed
            Input.parse_input_event(key)
        "button":
            var index: int = BUTTONS[name]
            var mask := 1 << (index - 1)
            _button_mask = (_button_mask | mask) if pressed else (_button_mask & ~mask)
            var button := InputEventMouseButton.new()
            button.button_index = index
            button.pressed = pressed
            button.position = _pointer
            button.global_position = _pointer
            button.button_mask = _button_mask
            Input.parse_input_event(button)

func _is_whole(value: Variant) -> bool:
    return (value is int) or (value is float and floorf(value) == value)
