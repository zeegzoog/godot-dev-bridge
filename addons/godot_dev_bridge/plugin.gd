@tool
extends EditorPlugin

## Sets the bridge up in a project: registers the `DevBridge` autoload, the adapter setting and the
## export plugin that keeps the runtime out of non-debug exports.

const MIN_MAJOR: int = 4
const MIN_MINOR: int = 5
const AUTOLOAD_NAME := "DevBridge"
const ADAPTER_SETTING := "godot_dev_bridge/adapter"

var _export_plugin: EditorExportPlugin = null

func _enter_tree() -> void:
    var version: Dictionary = Engine.get_version_info()
    var major: int = version["major"]
    var minor: int = version["minor"]
    if major < MIN_MAJOR or (major == MIN_MAJOR and minor < MIN_MINOR):
        printerr("[Godot Dev Bridge] Requires Godot %d.%d or newer, but this is Godot %s. The bridge was not set up." % [MIN_MAJOR, MIN_MINOR, version["string"]])
        return

    _register_adapter_setting()
    _register_autoload()
    var export_script: GDScript = load(_addon_directory().path_join("export_plugin.gd"))
    _export_plugin = export_script.new(_addon_directory())
    add_export_plugin(_export_plugin)

func _exit_tree() -> void:
    if _export_plugin != null:
        remove_export_plugin(_export_plugin)
        _export_plugin = null

## Runs when the user switches the plugin off: take the autoload away if it is ours.
func _disable_plugin() -> void:
    if ProjectSettings.has_setting(_autoload_key()) and _autoload_target() == _stub_path():
        remove_autoload_singleton(AUTOLOAD_NAME)

## The addon's own folder, wherever the project keeps it.
func _addon_directory() -> String:
    return (get_script() as Script).resource_path.get_base_dir()

func _stub_path() -> String:
    return _addon_directory().path_join("dev_bridge.gd")

func _autoload_key() -> String:
    return "autoload/" + AUTOLOAD_NAME

## The script the registered autoload points at, as a plain res:// path.
func _autoload_target() -> String:
    var target := str(ProjectSettings.get_setting(_autoload_key())).trim_prefix("*")
    if target.begins_with("uid://"):
        target = ResourceUID.uid_to_path(target)
    return target

## Registers the autoload on every editor start (not only when the box is ticked), so installing by
## adding the plugin to `[editor_plugins]` in project.godot works as well. An autoload of that name
## that points elsewhere is left alone and reported.
func _register_autoload() -> void:
    if ProjectSettings.has_setting(_autoload_key()):
        if _autoload_target() != _stub_path():
            printerr("[Godot Dev Bridge] The autoload '%s' already points at %s; leaving it alone. Remove it to let the plugin register its own." % [AUTOLOAD_NAME, _autoload_target()])
        return
    add_autoload_singleton(AUTOLOAD_NAME, _stub_path())

## Makes the adapter setting visible in Project Settings with a file picker for scripts.
func _register_adapter_setting() -> void:
    if not ProjectSettings.has_setting(ADAPTER_SETTING):
        ProjectSettings.set_setting(ADAPTER_SETTING, "")
    ProjectSettings.set_initial_value(ADAPTER_SETTING, "")
    ProjectSettings.add_property_info({
        "name": ADAPTER_SETTING,
        "type": TYPE_STRING,
        "hint": PROPERTY_HINT_FILE,
        "hint_string": "*.gd",
    })
