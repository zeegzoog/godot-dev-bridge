extends SceneTree

const Check := preload("res://tests/gd/check.gd")
const Session := preload("res://addons/godot_dev_bridge/runtime/session.gd")

var _directory: String = ""

func _write_request(id: String, sequence: int, text: String) -> void:
    var file := FileAccess.open(_directory.path_join("cmd-%d-%s.json" % [sequence, id]), FileAccess.WRITE)
    file.store_string(text)
    file.close()

func _ask(session: Variant, id: String, command: String, args: Dictionary = {}, sequence: int = 1) -> Dictionary:
    _write_request(id, sequence, JSON.stringify({"id": id, "cmd": command, "args": args}))
    session.serve_pending()
    return _answer(id)

func _answer(id: String) -> Dictionary:
    var path := _directory.path_join("resp-%s.json" % id)
    if not FileAccess.file_exists(path):
        return {}
    return JSON.parse_string(FileAccess.get_file_as_string(path))

func _clean() -> void:
    DirAccess.make_dir_recursive_absolute(_directory)
    for leftover in DirAccess.get_files_at(_directory):
        DirAccess.remove_absolute(_directory.path_join(leftover))

func _new_session(arguments: PackedStringArray = PackedStringArray()) -> Dictionary:
    var quits := {"count": 0}
    var quit := func() -> void: quits.count += 1
    var session: Variant = Session.new(_directory, quit, {"arguments": arguments, "tree": self})
    return {"session": session, "quits": quits}

func _initialize() -> void:
    # The scene tree is not live inside _initialize(): the root is not yet "inside the tree".
    await process_frame
    var check := Check.new()
    _directory = ProjectSettings.globalize_path("user://session_test")
    _clean()

    # Determinism is declared by the driver, not read from the engine.
    check.equal(Session.determinism_of(PackedStringArray(["--bridge-fixed-fps=60"]), 60), {"deterministic": true}, "a declared rate equal to the tick rate is deterministic")
    var mismatch: Dictionary = Session.determinism_of(PackedStringArray(["--bridge-fixed-fps=30"]), 60)
    check.is_true(not mismatch.deterministic and "differs" in mismatch.reason, "a different rate is not deterministic and says why")
    var undeclared: Dictionary = Session.determinism_of(PackedStringArray(), 60)
    check.is_true(not undeclared.deterministic and "no fixed frame rate" in undeclared.reason, "no declaration is not deterministic and says why")

    var made := _new_session(PackedStringArray(["--bridge-fixed-fps=%d" % Engine.physics_ticks_per_second]))
    var session: Variant = made.session
    session.start()
    var ready: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(_directory.path_join("ready.json")))
    check.equal(ready.protocol, 1, "ready.json carries protocol 1")
    check.equal(ready.pid, OS.get_process_id(), "and the game's pid")
    check.is_true(ready.has("godot_version") and ready.has("window_size") and ready.has("headless"), "and how the game was started")
    check.equal(ready.deterministic, true, "and the declared determinism")

    var pong := _ask(session, "p1", "ping")
    check.equal(pong.ok, true, "ping is answered")
    check.equal(pong.result, {"pong": true}, "with pong")
    check.equal(pong.id, "p1", "the response echoes the request id")
    check.is_true(pong.has("physics_frames") and pong.has("process_frames"), "every answer carries the frame counters")
    check.is_true(not FileAccess.file_exists(_directory.path_join("cmd-1-p1.json")), "the request file is removed")

    _write_request("broken", 1, "{not json")
    _write_request("array", 2, "[1, 2]")
    _write_request("nocmd", 3, JSON.stringify({"id": "nocmd"}))
    _write_request("badargs", 4, JSON.stringify({"id": "badargs", "cmd": "ping", "args": [1]}))
    _write_request("unknown", 5, JSON.stringify({"id": "unknown", "cmd": "no_such_command"}))
    session.serve_pending()
    check.equal(_answer("broken").error.code, "bad_request", "malformed JSON is bad_request")
    check.equal(_answer("array").error.code, "bad_request", "a JSON array is bad_request")
    check.equal(_answer("nocmd").error.code, "bad_request", "a missing command is bad_request")
    check.equal(_answer("badargs").error.code, "bad_args", "non-object arguments are bad_args")
    check.equal(_answer("unknown").error.code, "unknown_command", "an unknown command is unknown_command")
    check.equal(_answer("unknown").ok, false, "and not ok")

    _write_request("escape", 6, JSON.stringify({"id": "../escape", "cmd": "ping"}))
    session.serve_pending()
    check.is_true(FileAccess.file_exists(_directory.path_join("resp-escape.json")), "an id with path separators is replaced by the id of the request file, so the answer stays inside the directory")

    check.equal(_ask(session, "e1", "eval", {"expr": "1 + 2"}).result, {"value": 3}, "eval answers the value")
    check.equal(_ask(session, "e2", "eval", {"expr": "1 +"}).error.code, "eval_failed", "a bad expression is eval_failed")
    check.equal(_ask(session, "e3", "eval", {}).error.code, "bad_args", "a missing expression is bad_args")

    check.equal(_ask(session, "g1", "get", {"path": "/root/Nope"}).error.code, "not_found", "a missing node is not_found")
    check.equal(_ask(session, "g2", "get", {"path": "/root", "properties": ["name"]}).result.properties, {"name": "root"}, "get reads properties")
    check.equal(_ask(session, "t1", "tree", {"path": "/root", "depth": 0}).result.name, "root", "tree lists nodes")
    check.equal(_ask(session, "t2", "tree", {"depth": -1}).error.code, "bad_args", "a negative depth is bad_args")
    check.equal(_ask(session, "s1", "shot", {"width": "wide"}).error.code, "bad_args", "a bad width is bad_args")

    session.registry.add_state("answer", func() -> int: return 42)
    var echo := func(args: Dictionary) -> Dictionary: return args
    session.registry.add_command("echo", echo, "Echoes.")
    session.registry.add_command("five", func(_args: Dictionary) -> int: return 5)
    session.registry.add_command("quiet", func(_args: Dictionary) -> void: pass)
    session.registry.add_command("list", func(_args: Dictionary) -> Array: return [1, Vector2.ONE])
    session.registry.add_command(
        "broken",
        func(_args: Dictionary) -> Variant:
            var nothing = null
            return nothing.no_such_method()
    )
    check.is_true(not session.registry.add_command("ping", func(_args: Dictionary) -> int: return 1), "a core name is refused to the project")

    check.equal(_ask(session, "st1", "state").result, {"answer": 42}, "state reads the registered providers")
    check.equal(_ask(session, "c1", "commands").result.project, {"echo": "Echoes.", "five": "", "quiet": "", "list": "", "broken": ""}, "commands lists the project commands")
    check.is_true(_ask(session, "c2", "commands").result.core.has("step"), "and the core ones")
    check.equal(_ask(session, "d1", "echo", {"a": 1}).result, {"a": 1}, "a project command gets its arguments")
    check.equal(_ask(session, "d2", "five").result, 5, "a project command may return a non-dictionary")
    check.equal(_ask(session, "d3", "quiet").ok, true, "a project command that returns nothing is a success")
    check.equal(_ask(session, "d4", "list").result, [1, [1.0, 1.0]], "its value is converted for JSON")
    var failing := _ask(session, "d5", "broken")
    check.is_true(failing.has("ok"), "a project command that fails inside its own script still gets an answer (the engine logs the script error)")

    session._handlers["rogue"] = {"callable": func(_args: Dictionary) -> int: return 5, "help": ""}
    var rogue := _ask(session, "r1", "rogue")
    check.equal(rogue.ok, false, "a core handler that returns a non-dictionary is not a success")
    check.equal(rogue.error.code, "internal", "it is answered as an internal error instead of leaving the caller waiting")

    var status := _ask(session, "s2", "status")
    check.is_true(status.result.commands.has("step") and status.result.commands.has("echo"), "status lists core and project commands")
    check.equal(status.result.state_providers, ["answer"], "and the state providers")
    check.equal(status.result.held_inputs, [], "and what is held")

    _write_request("second", 20, JSON.stringify({"id": "second", "cmd": "ping"}))
    _write_request("first", 10, JSON.stringify({"id": "first", "cmd": "ping"}))
    session.served_ids.clear()
    session.serve_pending()
    check.equal(session.served_ids, ["first", "second"], "requests are answered oldest first by sequence")

    check.equal(_ask(session, "z1", "step", {"ticks": 0}).result.ticks_run, 0, "a step of 0 ticks is answered at once")
    check.equal(_ask(session, "z2", "step", {"ticks": -1}).error.code, "bad_args", "a negative tick count is bad_args")
    var immediate := _ask(session, "z3", "step", {"until": "true"})
    check.equal(immediate.result.ticks_run, 0, "a condition that already holds runs no tick")
    check.equal(immediate.result.until_met, true, "and is met")
    check.equal(_ask(session, "z4", "step", {"until": "1 +"}).error.code, "eval_failed", "a bad condition is eval_failed")

    _write_request("z5", 30, JSON.stringify({"id": "z5", "cmd": "step", "args": {"ticks": 3}}))
    session.serve_pending()
    check.is_true(_answer("z5").is_empty(), "a step with ticks to run is not answered yet")
    check.is_true(not FileAccess.file_exists(_directory.path_join("cmd-30-z5.json")), "and its request is gone so it is not served twice")
    # `step` counted this very tick as the first of the three, so two more ticks remain; a third call
    # would park the loop (and wait for requests forever).
    for tick in 2:
        session.physics_tick()
    session._complete_pending_step()
    check.equal(_answer("z5").result.ticks_run, 3, "the answer arrives when the step has run its ticks")

    # A tap is released by the ticks of stepping, however the steps are split. The first tick of a step
    # is counted inside `step` itself, so it has to release what falls due on it as well.
    _ask(session, "i1", "tap", {"action": "ui_accept"})
    check.equal(_ask(session, "i2", "status").result.held_inputs, ["action:ui_accept"], "a tap is held until its ticks are stepped")
    _ask(session, "i3", "step", {"ticks": 1})
    session._complete_pending_step()
    check.equal(_ask(session, "i4", "status").result.held_inputs, ["action:ui_accept"], "one tick of a two-tick tap is not enough")
    _ask(session, "i5", "step", {"ticks": 1})
    session._complete_pending_step()
    check.equal(_ask(session, "i6", "status").result.held_inputs, [], "the second one-tick step releases it")

    var failed_pointer := _ask(session, "pt1", "pointer", {"x": 1.0e308, "y": 0})
    check.equal(failed_pointer.ok, false, "a pointer position that is not finite is refused")
    check.equal(failed_pointer.error.code, "bad_args", "as bad arguments")
    var wide_shot := _ask(session, "sw1", "shot", {"width": 100000})
    check.equal(wide_shot.error.code, "bad_args", "a screenshot wider than the engine allows is refused")
    var status_now: Dictionary = _ask(session, "st1", "status").result
    check.equal(status_now.adapter, {"configured": false}, "a session with no adapter says so")

    # The memory of answered ids is bounded for every kind of answer, deferred steps included.
    for index in Session.SERVED_LOG_LIMIT + 20:
        _ask(session, "m%d" % index, "ping", {}, 100 + index)
        _ask(session, "n%d" % index, "step", {"ticks": 1}, 1000 + index)
        session._complete_pending_step()
    check.equal(session.served_ids.size(), Session.SERVED_LOG_LIMIT, "only the most recent answers are remembered")

    # A request file that cannot be read yet (another process still holds a fresh file, as an
    # antivirus scan does) is not answered with an error: it is looked at again.
    _write_request("late", 5000, "")
    session.serve_pending()
    check.is_true(_answer("late").is_empty(), "an unreadable request is not answered at once")
    check.is_true(FileAccess.file_exists(_directory.path_join("cmd-5000-late.json")), "and is left where it is")
    _write_request("late", 5000, JSON.stringify({"id": "late", "cmd": "ping"}))
    session.serve_pending()
    check.equal(_answer("late").result, {"pong": true}, "once it can be read it is served as usual")
    _write_request("never", 5001, "")
    for attempt in Session.MAX_UNREADABLE_TRIES + 1:
        session.serve_pending()
    check.equal(_answer("never").error.code, "bad_request", "one that never becomes readable is answered with an error in the end")

    var quit_answer := _ask(session, "q1", "quit")
    check.equal(quit_answer.result, {"quitting": true}, "quit is answered")
    check.is_true(session.is_quitting(), "the session knows it is ending")
    check.equal(made.quits.count, 1, "the quit action ran")
    var closed: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(_directory.path_join("closed.json")))
    check.equal(closed, {"reason": "quit"}, "closed.json says why")

    # The driver can tell from `ready.json` what became of the project adapter.
    var adapter_cases := {
        "res://tests/fixtures/adapter.gd": {"loaded": true},
        "res://tests/fixtures/no_such_adapter.gd": {"loaded": false},
        "res://tests/fixtures/ticker.gd": {"loaded": false},
    }
    for path: String in adapter_cases:
        _clean()
        var quits := {"count": 0}
        var quit := func() -> void: quits.count += 1
        var options := {"arguments": PackedStringArray(), "tree": self, "adapter": path}
        var adapter_session: Variant = Session.new(_directory, quit, options)
        adapter_session.start()
        var info: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(_directory.path_join("ready.json"))).adapter
        check.equal(info.path, path, "ready.json names the adapter %s" % path)
        check.equal(info.loaded, adapter_cases[path].loaded, "and whether it registered")
        check.equal(info.has("error"), not adapter_cases[path].loaded, "and why not, when it did not")
        adapter_session._release_adapter()
        check.is_true(adapter_session._adapter == null, "ending the session lets the adapter go (a Node adapter would leak)")

    _clean()
    check.finish(self, "session_test")
