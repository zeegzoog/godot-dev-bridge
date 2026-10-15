extends SceneTree

const Check := preload("res://tests/gd/check.gd")
const Registry := preload("res://addons/godot_dev_bridge/runtime/registry.gd")

class Provider:
    extends RefCounted

    func answer() -> int:
        return 42

    func echo(args: Dictionary) -> Dictionary:
        return args

func _initialize() -> void:
    var check := Check.new()
    var registry := Registry.new()
    registry.reserved = ["ping", "step"]
    var provider := Provider.new()

    check.is_true(registry.add_state("answer", provider.answer), "a state provider is accepted")
    check.is_true(not registry.add_state("", provider.answer), "an empty state name is refused")
    check.is_true(not registry.add_state("nothing", Callable()), "an invalid callable is refused")
    check.is_true(not registry.add_state("greedy", func(_unused: int) -> int: return 1), "a state provider takes no arguments")
    check.equal(registry.state_names(), ["answer"], "only the accepted provider is listed")

    check.is_true(registry.add_command("echo", provider.echo, "Echoes."), "a command is accepted")
    check.is_true(not registry.add_command("ping", provider.echo), "a core name is refused")
    check.is_true(not registry.add_command("", provider.echo), "an empty command name is refused")
    check.is_true(not registry.add_command("bad", Callable()), "an invalid handler is refused")
    check.is_true(not registry.add_command("blind", func() -> int: return 1), "a command that cannot take the arguments dictionary is refused up front")
    check.is_true(not registry.add_command("crowded", func(_a: Dictionary, _b: int) -> int: return 1), "and so is one that wants more than the dictionary")
    check.is_true(registry.add_command("optional", func(_args: Dictionary = {}) -> int: return 1), "a command whose argument has a default is fine")
    check.equal(registry.command_names(), ["echo", "optional"], "only the accepted commands are listed")
    check.equal(registry.command_help(), {"echo": "Echoes.", "optional": ""}, "help is kept by name")
    check.is_true(registry.has_command("echo"), "has_command sees it")

    check.equal(registry.read_state([]), {"answer": 42}, "no names asks for every provider")
    check.equal(registry.read_state(["answer"]), {"answer": 42}, "named providers are read")
    var missing: Dictionary = registry.read_state(["nope"])
    check.is_true(missing["nope"] is Dictionary and missing["nope"].has("error"), "an unknown provider answers an error for itself")

    check.equal(registry.run_command("echo", {"a": 1}), {"value": {"a": 1}}, "a command's value is wrapped")
    check.equal(registry.run_command("echo", {"__bad_args__": "`a` is missing."}), {"bad_args": "`a` is missing."}, "a dictionary whose only key is __bad_args__ refuses the arguments")
    check.equal(registry.run_command("echo", {"__bad_args__": 7}), {"bad_args": "7"}, "with its value as the message")
    check.equal(registry.run_command("echo", {"__bad_args__": "x", "b": 2}), {"value": {"__bad_args__": "x", "b": 2}}, "a dictionary with other keys is a value")
    check.equal(registry.run_command("echo", {"error": "x"}), {"value": {"error": "x"}}, "and so is one with an error entry")

    var nothing := Registry.new()
    nothing.add_command("quiet", func(_args: Dictionary) -> void: pass)
    check.equal(nothing.run_command("quiet", {}), {"value": null}, "a handler that returns nothing is a null value, not a failure")

    var node := Node.new()
    var gone := Registry.new()
    gone.add_state("doomed", Callable(node, "get_name"))
    gone.add_command("doomed", Callable(node, "set_name"))
    node.free()
    var state: Dictionary = gone.read_state(["doomed"])
    check.is_true(state["doomed"] is Dictionary and state["doomed"].has("error"), "a provider whose object was freed answers an error")
    check.is_true(gone.run_command("doomed", {}).has("error"), "a command whose object was freed answers an error")

    check.finish(self, "registry_test")
