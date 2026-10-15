extends SceneTree

const Check := preload("res://tests/gd/check.gd")
const STUB_PATH := "res://addons/godot_dev_bridge/dev_bridge.gd"

func _initialize() -> void:
    # The scene tree is not live inside _initialize(): the root is not yet "inside the tree".
    await process_frame
    var check := Check.new()
    var directory := ProjectSettings.globalize_path("user://stub_test")
    DirAccess.make_dir_recursive_absolute(directory)
    for leftover in DirAccess.get_files_at(directory):
        DirAccess.remove_absolute(directory.path_join(leftover))

    var stub: Node = load(STUB_PATH).new()
    check.is_true(stub.process_physics_priority < 0, "the stub runs before every other node's physics callback")
    check.equal(stub.process_mode, Node.PROCESS_MODE_ALWAYS, "and a game's pause does not stop it")

    root.add_child(stub)
    check.is_true(not stub.enabled, "started without launch arguments it is inert")
    check.is_true(not stub.is_physics_processing(), "and its processing is off")
    check.is_true(stub.session == null, "and it loaded nothing")

    check.is_true(not stub.enable(PackedStringArray(), true), "no --bridge-dir: inert")
    check.is_true(not stub.enable(PackedStringArray(["--bridge-dir=" + directory]), false), "a non-debug build ignores the request")
    check.is_true(not stub.enable(PackedStringArray(["--bridge-dir=" + directory + "/missing"]), true), "a missing directory is not enabled")
    check.is_true(stub.session == null and not stub.is_physics_processing(), "none of that loaded or switched anything on")

    var quits := {"count": 0}
    var quit := func() -> void: quits.count += 1
    var enabled: bool = stub.enable(
        PackedStringArray(["--bridge-dir=" + directory, "--bridge-fixed-fps=60"]), true, quit
    )
    check.is_true(enabled, "a debug build with a writable directory enables the session")
    check.is_true(stub.enabled and stub.session != null, "the session exists")
    check.is_true(FileAccess.file_exists(directory.path_join("ready.json")), "and has announced itself")
    check.is_true(stub.is_physics_processing(), "and the stub now processes physics")
    check.is_true(stub.enable(PackedStringArray(), true), "enabling again is a no-op that stays enabled")

    # Switch the stub off again before finishing: a physics tick would otherwise park the game.
    stub.set_physics_process(false)
    stub.session = null
    root.remove_child(stub)
    stub.free()
    for leftover in DirAccess.get_files_at(directory):
        DirAccess.remove_absolute(directory.path_join(leftover))
    check.finish(self, "stub_test")
