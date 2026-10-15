extends SceneTree

const Check := preload("res://tests/gd/check.gd")
const POLICY := "res://addons/godot_dev_bridge/export_policy.gd"
const ADDON := "res://addons/godot_dev_bridge"
const ADAPTER := "res://dev/BridgeAdapter.gd"

func _initialize() -> void:
    var check := Check.new()
    var policy: Variant = load(POLICY).new(ADDON)

    for path in [
        ADDON + "/runtime/session.gd",
        ADDON + "/runtime/observer.gd",
        ADDON + "/plugin.gd",
        ADDON + "/export_plugin.gd",
        ADDON + "/export_policy.gd",
        ADAPTER,
    ]:
        check.is_true(policy.should_skip(path, false, ADAPTER), "a non-debug export skips " + path)
        check.is_true(not policy.should_skip(path, true, ADAPTER), "a debug export keeps " + path)

    for path in [
        ADDON + "/dev_bridge.gd",
        ADDON + "/plugin.cfg",
        "res://src/Game.gd",
        "res://addons/godot_dev_bridge_other/runtime/x.gd",
        "res://dev/Other.gd",
    ]:
        check.is_true(not policy.should_skip(path, false, ADAPTER), "a non-debug export keeps " + path)

    check.is_true(not policy.should_skip("res://anything.gd", false, ""), "an empty adapter setting matches nothing")

    # The file picker of Project Settings stores the adapter as a uid://, the export asks by path.
    var adapter_id := ResourceUID.create_id()
    ResourceUID.add_id(adapter_id, ADAPTER)
    var as_uid := ResourceUID.id_to_text(adapter_id)
    check.is_true(policy.should_skip(ADAPTER, false, as_uid), "an adapter stored as a uid:// is skipped too")
    check.is_true(not policy.should_skip(ADAPTER, true, as_uid), "but not in a debug export")
    check.is_true(not policy.should_skip("res://dev/Other.gd", false, as_uid), "and the uid names only its own file")
    check.is_true(not policy.should_skip(ADAPTER, false, "uid://no_such_uid_registered"), "an unknown uid matches nothing")

    var moved: Variant = load(POLICY).new("res://tools/bridge")
    check.is_true(moved.should_skip("res://tools/bridge/runtime/session.gd", false, ""), "the policy follows the addon to another folder")
    check.is_true(not moved.should_skip(ADDON + "/runtime/session.gd", false, ""), "and ignores the old location")

    check.finish(self, "export_policy_test")
