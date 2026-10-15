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
    var adapter := _plain_path(adapter_path)
    if not adapter.is_empty() and path == adapter:
        return true
    var prefix := _addon_directory + "/"
    if _addon_directory.is_empty() or not path.begins_with(prefix):
        return false
    var relative := path.trim_prefix(prefix)
    return relative.begins_with("runtime/") or relative in EDITOR_ONLY


## The adapter setting may hold a `uid://` (the file picker of Project Settings stores one), while
## the export asks by path. A uid that is not registered stays as it is and matches nothing.
func _plain_path(path: String) -> String:
    if not path.begins_with("uid://"):
        return path
    var id := ResourceUID.text_to_id(path)
    return ResourceUID.get_id_path(id) if ResourceUID.has_id(id) else path
