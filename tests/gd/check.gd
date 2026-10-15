extends RefCounted

## A tiny assertion helper for the GDScript unit scripts: it collects failures, prints them, and
## sets the exit status so that the Rust runner can tell a failed script from a passed one.

var failures: Array[String] = []
var checks: int = 0

func is_true(condition: bool, label: String) -> void:
    checks += 1
    if not condition:
        failures.append(label)

func equal(actual: Variant, expected: Variant, label: String) -> void:
    checks += 1
    if not _same(actual, expected):
        failures.append("%s: expected %s, got %s" % [label, var_to_str(expected), var_to_str(actual)])

## Arrays and dictionaries are compared through their JSON text, so a typed `Array[String]` equals an
## untyped literal with the same elements, key order does not matter, and an int equals the float
## that a JSON round trip turns it into.
func _same(actual: Variant, expected: Variant) -> bool:
    if (actual is Array or actual is Dictionary) and typeof(actual) == typeof(expected):
        return JSON.stringify(_normalize(actual), "", true) == JSON.stringify(_normalize(expected), "", true)
    return actual == expected

func _normalize(value: Variant) -> Variant:
    if value is int:
        return float(value)
    if value is Array:
        var items := []
        for item: Variant in value:
            items.append(_normalize(item))
        return items
    if value is Dictionary:
        var members := {}
        for key: Variant in value:
            members[str(key)] = _normalize(value[key])
        return members
    return value

func finish(tree: SceneTree, script_name: String) -> void:
    for failure in failures:
        printerr("FAIL [%s] %s" % [script_name, failure])
    print("%s: %d checks, %d failed" % [script_name, checks, failures.size()])
    tree.quit(1 if not failures.is_empty() else 0)
