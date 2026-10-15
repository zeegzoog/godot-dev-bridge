extends SceneTree

const Check := preload("res://tests/gd/check.gd")
const Observer := preload("res://addons/godot_dev_bridge/runtime/observer.gd")

class Bridge:
    extends RefCounted
    var marker: int = 7

func _initialize() -> void:
    # The scene tree is not live inside _initialize(): the root is not yet "inside the tree".
    await process_frame
    var check := Check.new()
    var observer := Observer.new(self, Bridge.new())

    check.equal(observer.evaluate("1 + 2"), {"value": 3}, "an expression is evaluated")
    check.equal(observer.evaluate("bridge.marker"), {"value": 7}, "`bridge` is in scope")
    check.equal(observer.evaluate("root.name"), {"value": &"root"}, "`root` is in scope")
    check.is_true(observer.evaluate("1 +").has("error"), "a parse error is reported")
    check.is_true(observer.evaluate("nope.method()").has("error"), "a runtime failure is reported")

    check.equal(Observer.to_json(5), 5, "an int stays")
    check.equal(Observer.to_json("a"), "a", "a string stays")
    check.equal(Observer.to_json(Vector2(1, 2)), [1.0, 2.0], "a Vector2 is its components")
    check.equal(Observer.to_json(Vector3i(1, 2, 3)), [1, 2, 3], "a Vector3i is its components")
    check.equal(Observer.to_json(Color(1, 0.5, 0, 1)), [1.0, 0.5, 0.0, 1.0], "a Color is [r, g, b, a]")
    check.equal(Observer.to_json(Rect2(1, 2, 3, 4)), [1.0, 2.0, 3.0, 4.0], "a Rect2 is position then size")
    check.equal(Observer.to_json(&"name"), "name", "a StringName is a string")
    check.equal(Observer.to_json(NodePath("A/B")), "A/B", "a NodePath is a string")
    check.equal(Observer.to_json(INF), "inf", "a non-finite float is text")
    check.equal(Observer.to_json(PackedInt32Array([1, 2])), [1, 2], "a packed array is a list")
    check.equal(Observer.to_json({"a": Vector2.ONE, 3: "x"}), {"a": [1.0, 1.0], "3": "x"}, "dictionaries convert element by element and keys become strings")
    check.equal(Observer.to_json(Transform3D.IDENTITY), {"origin": [0.0, 0.0, 0.0], "basis": [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]}, "a Transform3D is origin and basis")
    check.equal(Observer.to_json(root), "/root", "a node in the tree is its absolute path")
    var loose := Node.new()
    loose.name = "Loose"
    check.equal(Observer.to_json(loose), "Loose", "a node outside the tree is its name")
    loose.free()
    var object := RefCounted.new()
    var converted: Variant = Observer.to_json(object)
    check.is_true(converted is Dictionary and converted["class"] == "RefCounted", "another object is {class, id}")
    check.is_true(Observer.to_json(Callable()) is String, "anything else is its var_to_str text")

    check.is_true(observer.node_at("/root") == root, "an absolute path finds the root")
    check.is_true(observer.node_at("/root/Nope") == null, "a missing path is null")
    var description: Dictionary = observer.describe(root, ["name", "position:x", "no_such_property"])
    check.equal(description["properties"]["name"], "root", "a property is described")
    check.equal(description["missing"], ["no_such_property"], "an unknown property is listed as missing")
    check.equal(description["class"], "Window", "the class is reported")

    var holder := Node.new()
    holder.name = "Holder"
    root.add_child(holder)
    var leaf := Node.new()
    leaf.name = "Leaf"
    holder.add_child(leaf)
    var shallow: Dictionary = observer.tree_of(holder, 0, false)
    check.equal(shallow["child_count"], 1, "a cut-off node says how many children it has")
    var deep: Dictionary = observer.tree_of(holder, 1, true)
    check.equal(deep["children"][0]["name"], "Leaf", "children are listed to the depth")
    check.equal(deep["children"][0]["process_mode"], 0, "properties adds process_mode")
    holder.free()

    var image := Image.create(8, 4, false, Image.FORMAT_RGBA8)
    var path := ProjectSettings.globalize_path("user://observer_test/shot.png")
    var saved: Dictionary = Observer.save_png(image, path, 4)
    check.equal(saved["size"], [4, 2], "a PNG is scaled to the requested width keeping proportions")
    check.is_true(FileAccess.file_exists(path), "and written")
    DirAccess.remove_absolute(path)
    check.is_true(Observer.save_png(image, "").has("error"), "an unwritable path is an error, not a crash")

    check.finish(self, "observer_test")
