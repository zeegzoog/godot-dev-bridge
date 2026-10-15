extends RefCounted

## One bridge session: it announces itself (`ready.json`), answers the requests a driver writes into
## the spool directory (each guarded, so a malformed, unknown or failing one gets an error answer and
## changes nothing) and says why it ended (`closed.json`). The commands are handlers on one
## dispatcher; a project adds its own through the registry.
##
## GDScript has no try/catch and a script error before the reply leaves the driver waiting for its
## timeout, so every path below ends in an answer: values are read untyped and checked by hand, and a
## handler result that is not a dictionary becomes an `internal` error.

const TRANSPORT := preload("spool_transport.gd")
const REGISTRY := preload("registry.gd")
const STEP_CONTROLLER := preload("step_controller.gd")
const OBSERVER := preload("observer.gd")
const INPUT_INJECTOR := preload("input_injector.gd")

## The wire protocol version, written to `ready.json`.
const PROTOCOL: int = 1

## The launch argument that declares the fixed frame rate the engine was started with.
const FIXED_FPS_ARGUMENT := "--bridge-fixed-fps="

## How many answered request ids are remembered, for diagnostics and tests.
const SERVED_LOG_LIMIT: int = 64

## The widest screenshot a `shot` may be scaled to: the engine's largest image side.
const MAX_SHOT_WIDTH: int = 16384

## How many times a request file that cannot be read yet is looked at again, a few milliseconds
## apart, before it is answered as malformed.
const MAX_UNREADABLE_TRIES: int = 500

## How long a parked session sleeps between looks at the spool directory.
const POLL_MILLISECONDS: int = 2

## How long, with no request, before a session ends the game itself, and how many ticks a `step`
## with a condition may run when it names no limit.
const DEFAULT_IDLE_SECONDS: float = 1800.0
const DEFAULT_MAX_TICKS: int = 600

## How many ticks a `tap` holds an input when it names no number.
const TAP_TICKS: int = 2

var registry := REGISTRY.new()

## The ids of the requests answered so far, oldest first (the most recent `SERVED_LOG_LIMIT`).
var served_ids: Array[String] = []

var _transport: Variant
var _quit: Callable
var _options: Dictionary
var _handlers: Dictionary = {}  # name -> {callable, help}
var _quitting: bool = false
var _controller := STEP_CONTROLLER.new()
var _input := INPUT_INJECTOR.new()
var _observer: Variant
var _unreadable: Dictionary = {}  # request file name -> how many times it could not be read
var _pending_step: Dictionary = {}  # {entry, id, until} while a step runs, else empty
var _last_activity_msec: int = 0
var _frames_at_step_start: int = 0
var _adapter: Variant = null
var _adapter_state: Dictionary = {"configured": false}
var _unpaired_reason: String = ""

func _init(directory: String, quit: Callable, options: Dictionary = {}) -> void:
    _transport = TRANSPORT.new(directory)
    _quit = quit
    _options = options
    _observer = OBSERVER.new(options.get("tree"), self)
    _register_core_commands()
    registry.reserved = core_command_names()

## Announces the session: loads the project adapter and writes `ready.json`. From here the session
## is listening.
func start() -> void:
    _load_adapter()
    _transport.write_json("ready.json", ready_info())

## Loads the project adapter named by the project setting, if there is one, and lets it register
## what the game can show and do. No setting leaves the session generic; a script that cannot be
## loaded is reported and leaves it generic too. What happened is in `ready.json` and `status`
## (`adapter`), so a driver does not have to read the game's log to find out.
func _load_adapter() -> void:
    var path := str(_options.get("adapter", ""))
    if path.is_empty():
        return
    _adapter_state = {"configured": true, "path": path, "loaded": false}
    var script: Variant = load(path) if ResourceLoader.exists(path) else null
    if not script is GDScript or not (script as GDScript).can_instantiate():
        _adapter_state["error"] = "the script cannot be loaded (see the game's log)"
        push_warning("godot_dev_bridge: the adapter '%s' cannot be loaded" % path)
        return
    _adapter = script.new()
    if _adapter.has_method("register"):
        _adapter.register(registry)
        _adapter_state["loaded"] = true
    else:
        _adapter_state["error"] = "the script has no register(registry) method"
        push_warning("godot_dev_bridge: the adapter '%s' has no register(registry)" % path)

## An adapter that extends `Node` is not in the tree, so nothing else would free it.
func _release_adapter() -> void:
    if _adapter is Node and is_instance_valid(_adapter) and not (_adapter as Node).is_inside_tree():
        (_adapter as Node).free()
    _adapter = null

## What `ready.json` holds: who the game is and how it was started.
func ready_info() -> Dictionary:
    var determinism := determinism_of(
        _options.get("arguments", PackedStringArray()), Engine.physics_ticks_per_second
    )
    if determinism.deterministic and not _unpaired_reason.is_empty():
        determinism = {"deterministic": false, "reason": _unpaired_reason}
    var info := {
        "protocol": PROTOCOL,
        "pid": OS.get_process_id(),
        "project": str(ProjectSettings.get_setting("application/config/name", "")),
        "godot_version": str(Engine.get_version_info().get("string", "")),
        "debug_build": OS.is_debug_build(),
        "headless": DisplayServer.get_name() == "headless",
        "deterministic": determinism.deterministic,
        "physics_ticks_per_second": Engine.physics_ticks_per_second,
        "window_size": _window_size(),
        "adapter": _adapter_state,
    }
    if not determinism.deterministic:
        info["reason"] = determinism.reason
    return info

## Whether stepping is exact: every tick paired with one process frame, which a fixed frame rate
## equal to the physics tick rate gives. The engine consumes `--fixed-fps` itself and does not show
## it to the game, so the driver declares the rate it started the engine with as
## `--bridge-fixed-fps=<n>`; `arguments` are the game's own (user) arguments. A step that then finds
## the frames did not advance with its ticks overrides the declaration (`_unpaired_reason`).
static func determinism_of(arguments: PackedStringArray, ticks_per_second: int) -> Dictionary:
    var declared := ""
    for argument in arguments:
        if argument.begins_with(FIXED_FPS_ARGUMENT):
            declared = argument.substr(FIXED_FPS_ARGUMENT.length())
    if declared.is_empty():
        return {
            "deterministic": false,
            "reason":
            (
                "no fixed frame rate was declared (start the game with --fixed-fps and pass the same "
                + "rate as --bridge-fixed-fps), so ticks and frames are not paired"
            ),
        }
    if not declared.is_valid_int() or declared.to_int() != ticks_per_second:
        return {
            "deterministic": false,
            "reason":
            (
                "the declared fixed frame rate %s differs from the physics tick rate %d, so ticks and frames are not paired"
                % [declared, ticks_per_second]
            ),
        }
    return {"deterministic": true}

## The start of every physics tick, before any other node's callback: lets the tick run while a step
## allows it, and parks otherwise. The game stays inside this call, answering requests, until a
## `step` lets the loop go again. The first tick of a session finds no step and parks at once.
func physics_tick() -> void:
    if _quitting:
        return
    if not _controller.at_tick_start():
        _input.release_due(_controller.total_ticks)
        return
    park()

## Holds the game still: finishes the step that just ended (its answer is the state now, after
## exactly that many complete ticks), frees the pointer, then answers requests until one lets the
## loop go on or the session ends. The window is kept alive meanwhile, and nothing else in the game
## runs: no tick, no frame, no awaited signal, no timer.
func park() -> void:
    _complete_pending_step()
    if not _options.get("keep_mouse", false):
        Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
    _last_activity_msec = Time.get_ticks_msec()
    while not _quitting:
        DisplayServer.process_events()
        for entry in _transport.pending():
            _last_activity_msec = Time.get_ticks_msec()
            _serve(entry)
            if _quitting or not _pending_step.is_empty():
                return
        if _idle_expired():
            _end("idle")
            return
        OS.delay_msec(POLL_MILLISECONDS)

## Answers every request waiting, oldest first; returns how many it answered. Never raises: each one
## is answered, with an error when it cannot be served, and then removed.
func serve_pending() -> int:
    var served := 0
    for entry in _transport.pending():
        _serve(entry)
        served += 1
        if _quitting:
            break
    return served

## Whether the driver asked the game to end and the session has answered.
func is_quitting() -> bool:
    return _quitting

func core_command_names() -> Array[String]:
    var names: Array[String] = []
    for name: String in _handlers:
        names.append(name)
    return names

func _register_core_commands() -> void:
    _handlers = {
        "ping": {"callable": _ping, "help": "Answers {pong: true}."},
        "status": {"callable": _status, "help": "The game's state of play and how it was started."},
        "step":
        {
            "callable": _step,
            "help": "Runs ticks: {ticks} exactly, or {until, max_ticks?} until an expression holds.",
        },
        "press":
        {
            "callable": _press,
            "help": "Presses {action|key|button}, held until released or for {ticks} ticks of stepping.",
        },
        "release": {"callable": _release, "help": "Releases {action|key|button}."},
        "tap":
        {
            "callable": _tap,
            "help": "Presses {action|key|button} and releases it after {ticks} ticks (default 2).",
        },
        "release_all": {"callable": _release_all, "help": "Releases everything held."},
        "pointer":
        {
            "callable": _pointer,
            "help": "Moves the pointer to {x, y} viewport pixels, or by them when {relative}.",
        },
        "eval": {"callable": _eval, "help": "Evaluates {expr} against the live tree and answers {value}."},
        "get": {"callable": _get_properties, "help": "The {properties} of the node at {path}."},
        "tree":
        {
            "callable": _tree_listing,
            "help": "The node tree below {path?} to {depth?} levels, with {properties?}.",
        },
        "shot": {"callable": _shot, "help": "Saves the window's last frame as a PNG: {path?, width?}."},
        "state":
        {
            "callable": _state,
            "help": "The registered state providers' values: {names?} for some, all by default.",
        },
        "commands": {"callable": _commands_listing, "help": "The core and project commands, with help."},
        "quit": {"callable": _quit_game, "help": "Answers, then ends the game."},
    }

func _serve(entry: Dictionary) -> void:
    var parsed: Variant = _transport.read_request(entry)
    var id: String = entry.id
    var outcome: Dictionary
    if parsed is StringName and parsed == TRANSPORT.UNREADABLE:
        var tries: int = int(_unreadable.get(entry.file, 0)) + 1
        if tries < MAX_UNREADABLE_TRIES:
            _unreadable[entry.file] = tries
            return
        parsed = null
    _unreadable.erase(entry.file)
    if parsed == null:
        outcome = _error("bad_request", "The request is not a JSON object.")
    else:
        var request: Dictionary = parsed
        # An id from the request names the answer file, so it must be a plain file name; anything
        # else keeps the id the request file's own name carries.
        var requested := str(request.get("id", id))
        if not requested.is_empty() and requested.is_valid_filename():
            id = requested
        var name: Variant = request.get("cmd")
        if not name is String or (name as String).is_empty():
            outcome = _error("bad_request", "The request has no command name.")
        elif request.has("args") and not request.args is Dictionary:
            outcome = _error("bad_args", "The arguments must be a JSON object.")
        else:
            outcome = _guarded(name, request.get("args", {}))

    if outcome.has("defer"):
        # A step that has ticks to run is answered when it ends; the request is removed now so it
        # is not served twice.
        DirAccess.remove_absolute(_transport.directory.path_join(entry.file))
        _pending_step = {"id": id, "entry": entry, "until": outcome.get("until", "")}
        return

    _transport.respond(entry, id, _response(id, outcome))
    _remember(id)
    if _quitting:
        _close("quit")
        if _quit.is_valid():
            _quit.call()

## Remembers an answered request id, keeping only the most recent `SERVED_LOG_LIMIT`.
func _remember(id: String) -> void:
    served_ids.append(id)
    while served_ids.size() > SERVED_LOG_LIMIT:
        served_ids.pop_front()

## Runs the command and turns anything that is not a dictionary (a handler that failed inside its own
## script returns null) into an `internal` error, so the driver always gets an answer.
func _guarded(name: String, args: Dictionary) -> Dictionary:
    var raw: Variant = _run(name, args)
    if raw is Dictionary:
        return raw
    return _error(
        "internal",
        "The command '%s' failed inside the bridge; see the game's log for the script error." % name
    )

## Runs command `name`: a core handler, or a project command. An unknown name is an error answer.
func _run(name: String, args: Dictionary) -> Variant:
    if _handlers.has(name):
        var handler: Callable = _handlers[name].callable
        return handler.call(args)
    if registry.has_command(name):
        var outcome: Dictionary = registry.run_command(name, args)
        if outcome.has("error"):
            return _error("project_error", outcome.error)
        if outcome.has("bad_args"):
            return _error("bad_args", outcome.bad_args)
        return {"result": OBSERVER.to_json(outcome.value)}
    return _error("unknown_command", "No command named '%s'." % name)

func _response(id: String, outcome: Dictionary) -> Dictionary:
    return {
        "id": id,
        "ok": not outcome.has("error"),
        "result": outcome.get("result"),
        "error": outcome.get("error"),
        "physics_frames": Engine.get_physics_frames(),
        "process_frames": Engine.get_process_frames(),
    }

func _error(code: String, message: String) -> Dictionary:
    return {"error": {"code": code, "message": message}}

func _ping(_args: Dictionary) -> Dictionary:
    return {"result": {"pong": true}}

func _status(_args: Dictionary) -> Dictionary:
    var info := ready_info()
    info.erase("protocol")
    info.erase("pid")
    info["renderer"] = RenderingServer.get_current_rendering_method()
    info["physics_frames"] = Engine.get_physics_frames()
    info["process_frames"] = Engine.get_process_frames()
    info["held_inputs"] = _input.held()
    info["state_providers"] = registry.state_names()
    info["commands"] = core_command_names() + registry.command_names()
    return {"result": info}

func _quit_game(_args: Dictionary) -> Dictionary:
    _quitting = true
    _input.release_all()
    return {"result": {"quitting": true}}

## The input commands: each turns the injector's `{held}` / `{position}` into a result and its
## `{error}` into a `bad_args` answer.
func _press(args: Dictionary) -> Dictionary:
    return _input_outcome(_input.press(args, _controller.total_ticks))

func _release(args: Dictionary) -> Dictionary:
    return _input_outcome(_input.release(args))

func _tap(args: Dictionary) -> Dictionary:
    return _input_outcome(_input.tap(args, _controller.total_ticks, TAP_TICKS))

func _release_all(_args: Dictionary) -> Dictionary:
    return {"result": {"released": _input.release_all()}}

func _pointer(args: Dictionary) -> Dictionary:
    return _input_outcome(_input.pointer(args))

func _input_outcome(outcome: Dictionary) -> Dictionary:
    if outcome.has("error"):
        return _error("bad_args", outcome.error)
    return {"result": OBSERVER.to_json(outcome)}

## `state`: the values of the registered providers (`{names?}` for some), converted for JSON; a
## provider that fails answers `{error}` for itself only.
func _state(args: Dictionary) -> Dictionary:
    var names: Variant = args.get("names", [])
    if not names is Array:
        return _error("bad_args", "`names` must be a list of provider names.")
    return {"result": OBSERVER.to_json(registry.read_state(names))}

## `commands`: the names and help text of the core's commands and the project's.
func _commands_listing(_args: Dictionary) -> Dictionary:
    var core := {}
    for name: String in _handlers:
        core[name] = _handlers[name].help
    return {"result": {"core": core, "project": registry.command_help()}}

## `eval`: the value of `{expr}`, converted for JSON.
func _eval(args: Dictionary) -> Dictionary:
    var expression: Variant = args.get("expr")
    if not expression is String or (expression as String).is_empty():
        return _error("bad_args", "`expr` must be an expression.")
    var outcome: Dictionary = _observer.evaluate(expression)
    if outcome.has("error"):
        return _error("eval_failed", outcome.error)
    return {"result": {"value": OBSERVER.to_json(outcome.value)}}

## `get`: the `{properties}` of the node at `{path}`; a path with no node is `not_found`.
func _get_properties(args: Dictionary) -> Dictionary:
    var path: Variant = args.get("path")
    if not path is String or (path as String).is_empty():
        return _error("bad_args", "`path` must be a node path.")
    var names: Variant = args.get("properties", [])
    if not names is Array:
        return _error("bad_args", "`properties` must be a list of names.")
    var node: Node = _observer.node_at(path)
    if node == null:
        return _error("not_found", "No node at '%s'." % path)
    return {"result": _observer.describe(node, names)}

## `tree`: the nodes below `{path}` (the root by default) to `{depth}` levels (2 by default).
func _tree_listing(args: Dictionary) -> Dictionary:
    var path: Variant = args.get("path", "/root")
    if not path is String:
        return _error("bad_args", "`path` must be a node path.")
    var depth: Variant = args.get("depth", 2)
    if not _is_whole_number(depth) or int(depth) < 0:
        return _error("bad_args", "`depth` must be a non-negative integer.")
    var node: Node = _observer.node_at(path)
    if node == null:
        return _error("not_found", "No node at '%s'." % path)
    return {"result": _observer.tree_of(node, int(depth), bool(args.get("properties", false)))}

## `shot`: the main window's last frame as a PNG, to `{path}` or to `shots/` in the session
## directory, scaled to `{width}` pixels across when that is given. A session with no window answers
## `no_renderer`, and a `{path}` that cannot be written `bad_args`.
func _shot(args: Dictionary) -> Dictionary:
    var width: Variant = args.get("width", 0)
    if not _is_whole_number(width) or int(width) < 0 or int(width) > MAX_SHOT_WIDTH:
        return _error("bad_args", "`width` must be an integer from 0 to %d." % MAX_SHOT_WIDTH)
    var path: Variant = args.get("path", "")
    if not path is String:
        return _error("bad_args", "`path` must be a file path.")
    var image: Image = _observer.window_image()
    if image == null:
        return _error("no_renderer", "This session draws no window, so there is no frame to save.")
    var target: String = path
    if target.is_empty():
        target = _transport.directory.path_join("shots/shot-%d.png" % Engine.get_physics_frames())
    var saved: Dictionary = OBSERVER.save_png(image, target, int(width))
    if saved.has("error"):
        # A path the driver named that cannot be written is the driver's to fix; the session's own
        # `shots/` directory failing is not.
        return _error("bad_args" if not (path as String).is_empty() else "internal", saved.error)
    return {"result": saved}

## `step`: `{ticks}` runs exactly that many ticks; `{until, max_ticks?}` runs until the expression
## holds, checked before every tick (so one that already holds runs none), or `max_ticks` ticks. A
## step with nothing to run is answered at once; otherwise the answer waits for the step to end and
## the loop is let go, this very tick being the first.
func _step(args: Dictionary) -> Dictionary:
    var condition := Callable()
    var limit := 0
    var until := ""
    if args.has("until"):
        if not args.until is String or (args.until as String).is_empty():
            return _error("bad_args", "`until` must be an expression.")
        until = args.until
        var limit_value: Variant = args.get("max_ticks", DEFAULT_MAX_TICKS)
        if not _is_whole_number(limit_value) or int(limit_value) < 0:
            return _error("bad_args", "`max_ticks` must be a non-negative integer.")
        limit = int(limit_value)
        var probe: Dictionary = _observer.evaluate(until)
        if probe.has("error"):
            return _error("eval_failed", probe.error)
        condition = func() -> bool: return _until_value(until)
    else:
        var ticks: Variant = args.get("ticks")
        if not _is_whole_number(ticks) or int(ticks) < 0:
            return _error("bad_args", "`ticks` must be a non-negative integer.")
        limit = int(ticks)

    _frames_at_step_start = Engine.get_process_frames()
    _controller.begin(limit, condition)
    if _controller.at_tick_start():
        return {"result": _step_result(until)}
    # This very tick is the first of the step and was counted just now, not by a physics callback:
    # what is scheduled to be released on it must be released here, or a step of one tick would never
    # release anything and splitting a step would change what the game sees.
    _input.release_due(_controller.total_ticks)
    return {"defer": true, "until": until}

func _is_whole_number(value: Variant) -> bool:
    return (value is int) or (value is float and floorf(value) == value)

## The truth of an `until` expression right now; one that fails at run time counts as not holding.
func _until_value(expression: String) -> bool:
    var outcome: Dictionary = _observer.evaluate(expression)
    return outcome.has("value") and bool(outcome.value)

func _step_result(until: String) -> Dictionary:
    var result := {
        "ticks_run": _controller.ticks_run,
        "physics_frames": Engine.get_physics_frames(),
        "process_frames": Engine.get_process_frames(),
    }
    if not until.is_empty():
        result["until_met"] = _controller.until_met
    return result

## Answers the step that has just ended, if one was running.
func _complete_pending_step() -> void:
    if _pending_step.is_empty():
        return
    var pending := _pending_step
    _pending_step = {}
    _verify_pairing()
    var response := _response(pending.id, {"result": _step_result(pending.until)})
    _transport.write_json("resp-%s.json" % pending.id, response)
    _remember(pending.id)

## A finished step must have advanced the process frames by exactly its ticks; if not, the frame
## rate is not fixed after all and the session says so from now on.
func _verify_pairing() -> void:
    var advanced := Engine.get_process_frames() - _frames_at_step_start
    if _controller.ticks_run > 0 and advanced != _controller.ticks_run:
        _unpaired_reason = (
            "a step of %d ticks advanced the process frames by %d, so ticks and frames are not paired"
            % [_controller.ticks_run, advanced]
        )

func _idle_expired() -> bool:
    var raw: Variant = _options.get("idle_quit_seconds", "")
    var seconds := DEFAULT_IDLE_SECONDS
    if str(raw).is_valid_float():
        seconds = str(raw).to_float()
    if seconds <= 0.0:
        return false
    return (Time.get_ticks_msec() - _last_activity_msec) / 1000.0 >= seconds

## Ends the session for `reason`: says so in `closed.json` and asks the game to end.
func _end(reason: String) -> void:
    _quitting = true
    _input.release_all()
    _close(reason)
    if _quit.is_valid():
        _quit.call()

## The game is ending without the session having ended it (its own code quit, or its window was
## closed): says so in `closed.json`. A session that ended the game itself has said why already.
func game_exiting() -> void:
    if _quitting:
        return
    _quitting = true
    _close("game")

## Says why the session is over (`closed.json`) and lets the adapter go.
func _close(reason: String) -> void:
    _transport.write_json("closed.json", {"reason": reason})
    _release_adapter()

func _window_size() -> Array:
    var size := DisplayServer.window_get_size()
    return [size.x, size.y]
