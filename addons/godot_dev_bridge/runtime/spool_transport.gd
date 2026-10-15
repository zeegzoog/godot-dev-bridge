extends RefCounted

## The files a session talks through: one directory, one request per `cmd-<seq>-<id>.json`, one
## answer per `resp-<id>.json`, and the few session files (`ready.json`, `closed.json`). A writer
## creates `<name>.tmp` and renames it, a reader ignores `*.tmp`, so a half-written file is never
## read. Nothing outside the directory is read or written.

const REQUEST_PREFIX := "cmd-"
const RESPONSE_PREFIX := "resp-"
const SUFFIX := ".json"

var directory: String

func _init(spool_directory: String) -> void:
    directory = spool_directory

## The requests waiting, oldest first by their sequence number: each `{file, seq, id}`, the id being
## the one the file name carries. Files that are not finished (`*.tmp`) or not requests are left out.
func pending() -> Array[Dictionary]:
    var found: Array[Dictionary] = []
    for file in DirAccess.get_files_at(directory):
        if not file.begins_with(REQUEST_PREFIX) or not file.ends_with(SUFFIX):
            continue
        var stem := file.trim_prefix(REQUEST_PREFIX).trim_suffix(SUFFIX)
        var split := stem.find("-")
        if split <= 0:
            continue
        var sequence := stem.substr(0, split)
        if not sequence.is_valid_int():
            continue
        found.append({"file": file, "seq": sequence.to_int(), "id": stem.substr(split + 1)})
    found.sort_custom(
        func(a: Dictionary, b: Dictionary) -> bool:
            return a.seq < b.seq or (a.seq == b.seq and a.file < b.file)
    )
    return found

## The parsed content of a request file: a `Dictionary` when it is a JSON object, `null` when it is
## not valid JSON or not an object.
func read_request(entry: Dictionary) -> Variant:
    var text := FileAccess.get_file_as_string(directory.path_join(entry.file))
    var json := JSON.new()  # not `parse_string`: that logs an engine error for every malformed request
    if json.parse(text) != OK:
        return null
    return json.data if json.data is Dictionary else null

## Writes the answer to request `id` and removes the request.
func respond(entry: Dictionary, id: String, response: Dictionary) -> void:
    write_json(RESPONSE_PREFIX + id + SUFFIX, response)
    DirAccess.remove_absolute(directory.path_join(entry.file))

## Writes `data` as JSON to `name` in the directory, finished-or-absent: through a temporary name
## and a rename.
func write_json(name: String, data: Dictionary) -> void:
    var final_path := directory.path_join(name)
    var temporary := final_path + ".tmp"
    var file := FileAccess.open(temporary, FileAccess.WRITE)
    if file == null:
        push_warning("godot_dev_bridge: cannot write %s" % temporary)
        return
    file.store_string(JSON.stringify(data))
    file.close()
    if FileAccess.file_exists(final_path):
        DirAccess.remove_absolute(final_path)
    DirAccess.rename_absolute(temporary, final_path)
