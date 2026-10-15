extends RefCounted

## Which files a non-debug export must leave out. Kept apart from the export plugin so that it can
## be tested without an editor: `EditorExportPlugin` cannot be instantiated outside the editor.

## Scripts of the addon that only the editor uses.
const EDITOR_ONLY: PackedStringArray = ["plugin.gd", "export_plugin.gd", "export_policy.gd"]

var _addon_directory: String = ""


func _init(addon_directory: String = "") -> void:
    _addon_directory = addon_directory


## Whether `path` must stay out of an export. Nothing is skipped in a debug export. In any other
## export the runtime, the editor-only scripts and the project adapter are skipped; the inert
## autoload stub stays, because the project's autoload list names it.
func should_skip(path: String, is_debug: bool, adapter_path: String) -> bool:
    if is_debug:
        return false
    if not adapter_path.is_empty() and path == adapter_path:
        return true
    var prefix := _addon_directory + "/"
    if _addon_directory.is_empty() or not path.begins_with(prefix):
        return false
    var relative := path.trim_prefix(prefix)
    return relative.begins_with("runtime/") or relative in EDITOR_ONLY
