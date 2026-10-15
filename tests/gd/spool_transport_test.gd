extends SceneTree

const Check := preload("res://tests/gd/check.gd")
const Transport := preload("res://addons/godot_dev_bridge/runtime/spool_transport.gd")

func _write(directory: String, name: String, text: String) -> void:
    var file := FileAccess.open(directory.path_join(name), FileAccess.WRITE)
    file.store_string(text)
    file.close()

func _initialize() -> void:
    var check := Check.new()
    var directory := ProjectSettings.globalize_path("user://spool_transport_test")
    DirAccess.make_dir_recursive_absolute(directory)
    for leftover in DirAccess.get_files_at(directory):
        DirAccess.remove_absolute(directory.path_join(leftover))

    var transport := Transport.new(directory)
    check.equal(transport.pending(), [], "an empty directory has no requests")

    _write(directory, "cmd-20-bb.json", "{\"id\": \"bb\", \"cmd\": \"ping\"}")
    _write(directory, "cmd-3-aa.json", "{\"id\": \"aa\", \"cmd\": \"ping\"}")
    _write(directory, "cmd-9-half.json.tmp", "{")
    _write(directory, "ready.json", "{}")
    _write(directory, "cmd-x-bad.json", "{}")
    _write(directory, "cmd-nodash.json", "{}")
    var pending: Array[Dictionary] = transport.pending()
    check.equal(pending.size(), 2, "only finished request files count")
    check.equal(pending[0].id, "aa", "oldest sequence first, numerically")
    check.equal(pending[1].id, "bb", "then the newer one")
    check.equal(pending[0].seq, 3, "the sequence number is read")

    var parsed: Variant = transport.read_request(pending[0])
    check.is_true(parsed is Dictionary and parsed.cmd == "ping", "a request object is parsed")

    _write(directory, "cmd-30-broken.json", "{not json")
    _write(directory, "cmd-31-array.json", "[1, 2]")
    var entries: Array[Dictionary] = transport.pending()
    check.is_true(transport.read_request(entries[2]) == null, "malformed JSON reads as null")
    check.is_true(transport.read_request(entries[3]) == null, "a JSON array reads as null")

    transport.respond(pending[0], "aa", {"id": "aa", "ok": true})
    check.is_true(FileAccess.file_exists(directory.path_join("resp-aa.json")), "the answer is written")
    check.is_true(not FileAccess.file_exists(directory.path_join("cmd-3-aa.json")), "and the request removed")
    check.is_true(not FileAccess.file_exists(directory.path_join("resp-aa.json.tmp")), "no temporary file is left")
    var answer: Variant = JSON.parse_string(FileAccess.get_file_as_string(directory.path_join("resp-aa.json")))
    check.equal(answer, {"id": "aa", "ok": true}, "the answer round-trips")

    transport.write_json("closed.json", {"reason": "quit"})
    transport.write_json("closed.json", {"reason": "idle"})
    var closed: Variant = JSON.parse_string(FileAccess.get_file_as_string(directory.path_join("closed.json")))
    check.equal(closed, {"reason": "idle"}, "write_json replaces an existing file")

    for leftover in DirAccess.get_files_at(directory):
        DirAccess.remove_absolute(directory.path_join(leftover))
    check.finish(self, "spool_transport_test")
