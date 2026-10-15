extends SceneTree

const Check := preload("res://tests/gd/check.gd")
const Injector := preload("res://addons/godot_dev_bridge/runtime/input_injector.gd")
const TEST_ACTION := "godot_dev_bridge_test_action"

func _initialize() -> void:
    var check := Check.new()
    if not InputMap.has_action(TEST_ACTION):
        InputMap.add_action(TEST_ACTION)
    var injector := Injector.new()

    var refused := [
        {},
        {"action": "no_such_action_anywhere"},
        {"key": "NotAKey"},
        {"button": "side"},
        {"action": TEST_ACTION, "key": "A"},
        {"action": ""},
        {"action": 7},
        {"action": TEST_ACTION, "ticks": 0},
        {"action": TEST_ACTION, "ticks": "soon"},
    ]
    for args: Dictionary in refused:
        check.is_true(injector.press(args, 0).has("error"), "refused: %s" % [args])
    check.equal(injector.held(), [], "nothing was left held by a refused press")
    check.is_true(injector.pointer({"x": "left", "y": 1}).has("error"), "a pointer needs numbers")

    check.equal(injector.press({"action": TEST_ACTION, "ticks": 3}, 10)["held"], ["action:" + TEST_ACTION], "a press is held")
    injector.release_due(12)
    check.equal(injector.held().size(), 1, "not yet due at tick 12")
    injector.release_due(13)
    check.equal(injector.held(), [], "released at tick 13")

    injector.press({"action": TEST_ACTION}, 0)
    injector.release_due(100000)
    check.equal(injector.held().size(), 1, "a press without ticks is held until released")
    check.equal(injector.release_all(), 1, "release_all reports what it released")
    check.equal(injector.held(), [], "and nothing is left")

    injector.tap({"action": TEST_ACTION}, 4, 2)
    injector.release_due(5)
    check.equal(injector.held().size(), 1, "a tap is held through tick 5")
    injector.release_due(6)
    check.equal(injector.held(), [], "and released at tick 6")

    check.equal(injector.press({"key": "A"}, 0)["held"], ["key:A"], "a key can be pressed")
    check.equal(injector.press({"button": "left"}, 0)["held"].size(), 2, "and a mouse button")
    injector.release({"key": "A"})
    check.equal(injector.held(), ["button:left"], "release names what to let go")
    injector.release_all()

    check.equal(injector.pointer({"x": 10, "y": 20})["position"], [10.0, 20.0], "the pointer moves to a position")
    check.equal(injector.pointer({"x": 5, "y": 5, "relative": true})["position"], [15.0, 25.0], "or by an amount")

    check.is_true(injector.pointer({"x": 1.0e308, "y": 0}).has("error"), "a position that does not fit a float is refused")
    injector.pointer({"x": 3.0e38, "y": 0})
    check.is_true(injector.pointer({"x": 3.0e38, "y": 0, "relative": true}).has("error"), "a move that overflows is refused")
    check.equal(injector.pointer({"x": 1, "y": 1})["position"], [1.0, 1.0], "and the pointer was not left at infinity")

    injector.press({"key": "space"}, 0)
    injector.press({"key": "Space"}, 0)
    check.equal(injector.held(), ["key:Space"], "one key is one entry, whatever its spelling")
    injector.release({"key": "space"})
    check.equal(injector.held(), [], "and one release lets it go")

    InputMap.erase_action(TEST_ACTION)
    check.finish(self, "input_injector_test")
