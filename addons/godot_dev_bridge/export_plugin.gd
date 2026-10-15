@tool
extends EditorExportPlugin

## Keeps debug-only files out of non-debug exports, so a forgotten export filter cannot ship the
## bridge. What to leave out is decided by `export_policy.gd`; this class only feeds it the export
## mode and the adapter setting.

const POLICY := preload("export_policy.gd")
const ADAPTER_SETTING := "godot_dev_bridge/adapter"

var _policy: Variant
var _debug_export: bool = false


func _init(addon_directory: String = "") -> void:
    _policy = POLICY.new(addon_directory)


func _get_name() -> String:
    return "GodotDevBridgeExport"


func _export_begin(_features: PackedStringArray, is_debug: bool, _path: String, _flags: int) -> void:
    _debug_export = is_debug


func _export_file(path: String, _type: String, _features: PackedStringArray) -> void:
    if _policy.should_skip(path, _debug_export, str(ProjectSettings.get_setting(ADAPTER_SETTING, ""))):
        skip()
