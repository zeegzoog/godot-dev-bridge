extends RefCounted

## The end-to-end fixture's adapter: the one file that tells the generic bridge what this tiny
## "game" can show and do.

func register(registry) -> void:
    registry.add_state("ticker", _ticker)
    registry.add_command("add", _add, "Adds {a} and {b}.")
    registry.add_command("ping", _add, "Clashes with a core name, so it is refused.")

## The ticker's counters, or an empty value while the scene is not there.
func _ticker() -> Dictionary:
    var tree := Engine.get_main_loop() as SceneTree
    var node := tree.root.get_node_or_null("Ticker")
    if node == null:
        return {}
    return {
        "ticks": node.ticks,
        "frames": node.frames,
        "pressed_ticks": node.pressed_ticks,
        "just_pressed_at": node.just_pressed_at,
    }

func _add(args: Dictionary) -> Dictionary:
    return {"sum": int(args.get("a", 0)) + int(args.get("b", 0))}
