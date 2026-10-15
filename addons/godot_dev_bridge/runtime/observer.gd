extends RefCounted

## Reads the running program on demand: expressions evaluated against the live scene tree, the
## properties of a node, the shape of the tree and the frame the window drew, all as JSON-able values.
## The names an expression can use are `root`, `tree`, `bridge` and every autoload by its registered
## name; the scene tree's root is the base instance, so its own methods are callable directly. A
## parse error or a failure at run time is reported with the engine's own message and changes nothing.

## The value types that are sequences of values: an array and every packed array.
const SEQUENCE_TYPES := [
    TYPE_ARRAY,
    TYPE_PACKED_BYTE_ARRAY,
    TYPE_PACKED_INT32_ARRAY,
    TYPE_PACKED_INT64_ARRAY,
    TYPE_PACKED_FLOAT32_ARRAY,
    TYPE_PACKED_FLOAT64_ARRAY,
    TYPE_PACKED_STRING_ARRAY,
    TYPE_PACKED_VECTOR2_ARRAY,
    TYPE_PACKED_VECTOR3_ARRAY,
    TYPE_PACKED_COLOR_ARRAY,
    TYPE_PACKED_VECTOR4_ARRAY,
]

var _tree: SceneTree
var _bridge: Variant

func _init(scene_tree: SceneTree, bridge_object: Variant) -> void:
    _tree = scene_tree
    _bridge = bridge_object

## Evaluates `text`; the answer is `{value}` or `{error}`.
func evaluate(text: String) -> Dictionary:
    if _tree == null:
        return {"error": "There is no scene tree to evaluate against."}
    var names := PackedStringArray(["root", "tree", "bridge"])
    var values: Array = [_tree.root, _tree, _bridge]
    for autoload_name in _autoload_names():
        var node := _tree.root.get_node_or_null(autoload_name)
        if node != null:
            names.append(autoload_name)
            values.append(node)

    var expression := Expression.new()
    if expression.parse(text, names) != OK:
        return {"error": expression.get_error_text()}
    var value: Variant = expression.execute(values, _tree.root, false)
    if expression.has_execute_failed():
        return {"error": expression.get_error_text()}
    return {"value": value}

## The names of the project's autoloads.
func _autoload_names() -> Array[String]:
    var found: Array[String] = []
    for property in ProjectSettings.get_property_list():
        var name: String = property.name
        if name.begins_with("autoload/"):
            found.append(name.trim_prefix("autoload/"))
    return found

## `value` as something JSON can hold: scalars as they are; arrays, packed arrays and dictionaries
## element by element; vectors, quaternions, colours (`[r, g, b, a]`) and rectangles as arrays of
## components; a transform as `{origin, basis: [x, y, z]}`; a node as its absolute path; any other
## object as `{class, id}`; and anything else as its `var_to_str` text.
static func to_json(value: Variant) -> Variant:
    if typeof(value) in SEQUENCE_TYPES:
        var elements := []
        for element in value:
            elements.append(to_json(element))
        return elements
    match typeof(value):
        TYPE_NIL, TYPE_BOOL, TYPE_INT, TYPE_STRING:
            return value
        TYPE_FLOAT:
            return value if is_finite(value) else str(value)
        TYPE_STRING_NAME, TYPE_NODE_PATH:
            return str(value)
        TYPE_VECTOR2, TYPE_VECTOR2I:
            return [value.x, value.y]
        TYPE_VECTOR3, TYPE_VECTOR3I:
            return [value.x, value.y, value.z]
        TYPE_VECTOR4, TYPE_VECTOR4I, TYPE_QUATERNION:
            return [value.x, value.y, value.z, value.w]
        TYPE_COLOR:
            return [value.r, value.g, value.b, value.a]
        TYPE_RECT2, TYPE_RECT2I:
            return [value.position.x, value.position.y, value.size.x, value.size.y]
        TYPE_TRANSFORM3D:
            return {
                "origin": to_json(value.origin),
                "basis": [to_json(value.basis.x), to_json(value.basis.y), to_json(value.basis.z)],
            }
        TYPE_DICTIONARY:
            var members := {}
            for key in value:
                members[str(key)] = to_json(value[key])
            return members
        TYPE_OBJECT:
            if not is_instance_valid(value):
                return null
            if value is Node:
                return str(value.get_path()) if value.is_inside_tree() else str(value.name)
            return {"class": value.get_class(), "id": value.get_instance_id()}
    return var_to_str(value)

## The node at `path` (absolute, or relative to the tree's root), or `null`.
func node_at(path: String) -> Node:
    if _tree == null:
        return null
    return _tree.root.get_node_or_null(NodePath(path))

## The requested properties of `node`: `{path, class, properties, missing?}`. A property name may be
## indexed (`position:x`); one the node does not have is listed in `missing` instead.
func describe(node: Node, properties: Array) -> Dictionary:
    var values := {}
    var missing: Array[String] = []
    for property in properties:
        var name := str(property)
        var head := name.get_slice(":", 0)
        if head in node:
            values[name] = to_json(node.get_indexed(NodePath(name)))
        else:
            missing.append(name)
    var description := {"path": str(node.get_path()), "class": node.get_class(), "properties": values}
    if not missing.is_empty():
        description["missing"] = missing
    return description

## The tree below `node` as nested `{name, class, children}`, `depth` levels deep; a node whose
## children are cut off says how many it has (`child_count`). With `properties`, each node also
## gives `visible` (where it has one) and `process_mode`.
func tree_of(node: Node, depth: int, properties: bool) -> Dictionary:
    var entry := {"name": str(node.name), "class": node.get_class()}
    if properties:
        if "visible" in node:
            entry["visible"] = bool(node.visible)
        entry["process_mode"] = int(node.process_mode)
    if depth > 0:
        var children := []
        for child in node.get_children():
            children.append(tree_of(child, depth - 1, properties))
        entry["children"] = children
    elif node.get_child_count() > 0:
        entry["child_count"] = node.get_child_count()
    return entry

## The frame the main window last drew, or `null` where nothing is drawn (a headless session).
func window_image() -> Image:
    if _tree == null or DisplayServer.get_name() == "headless":
        return null
    var image := _tree.root.get_viewport().get_texture().get_image()
    if image == null or image.is_empty():
        return null
    return image

## Writes `image` to `path` as a PNG, scaled to `width` pixels across first when `width` is positive
## (keeping its proportions); returns `{path, size}`, or `{error}` when the file cannot be written.
static func save_png(image: Image, path: String, width: int = 0) -> Dictionary:
    var picture := image.duplicate() as Image
    if width > 0 and width != picture.get_width():
        var height := maxi(1, roundi(float(picture.get_height()) * width / picture.get_width()))
        picture.resize(width, height, Image.INTERPOLATE_LANCZOS)
    DirAccess.make_dir_recursive_absolute(path.get_base_dir())
    if picture.save_png(path) != OK:
        return {"error": "Cannot write %s." % path}
    return {"path": path, "size": [picture.get_width(), picture.get_height()]}
