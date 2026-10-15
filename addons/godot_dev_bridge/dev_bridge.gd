extends Node

## The autoload stub: always loaded, inert unless a launch argument enables it in a debug build.
## Enabled, it loads the runtime and serves the requests a driver writes into a spool directory;
## otherwise it switches its own processing off, reads nothing further, loads nothing and says
## nothing. Only the stub is meant to ship in a release: the runtime under `runtime/` is loaded by
## path, and the export plugin leaves it out of non-debug exports, so there is nothing left for a
## flag to switch on.

## The runtime's entry point, relative to this script and loaded only on enablement.
const RUNTIME_SESSION := "runtime/session.gd"

## Launch arguments, after the `--` that separates the application's own from the engine's.
const DIRECTORY_ARGUMENT := "--bridge-dir="
const IDLE_ARGUMENT := "--bridge-idle-quit="
const KEEP_MOUSE_ARGUMENT := "--bridge-keep-mouse"

## The project setting that names the project adapter script (read only when enabled).
const ADAPTER_SETTING := "godot_dev_bridge/adapter"

## The lowest physics priority: lower runs earlier, so this node's physics callback comes before
## every other node's, wherever it stands in the tree.
const LOWEST_PHYSICS_PRIORITY: int = -(1 << 30)

var enabled: bool = false

## The running session, or `null` while inert.
var session: Variant = null

func _init() -> void:
    process_physics_priority = LOWEST_PHYSICS_PRIORITY
    process_mode = Node.PROCESS_MODE_ALWAYS  # a game's own pause must not stop the bridge

func _ready() -> void:
    set_process(false)
    set_physics_process(false)
    enable(OS.get_cmdline_user_args(), OS.is_debug_build())

## Enables the session if `arguments` carry a spool directory, `debug_build` is true and the
## directory can be written; returns whether it did. Nothing else happens otherwise: no log line, no
## file, no processing. The arguments and the build flag are parameters so a test can stand in for
## both. `quit` is what ends the game when the driver asks (the tree's own quit by default).
func enable(arguments: PackedStringArray, debug_build: bool, quit: Callable = Callable()) -> bool:
    if enabled:
        return true
    var directory := _value_of(arguments, DIRECTORY_ARGUMENT)
    if directory.is_empty() or not debug_build:
        return false
    if not _can_write_in(directory):
        return false

    var runtime_path := _runtime_path()
    if not ResourceLoader.exists(runtime_path):
        return false
    var script: GDScript = load(runtime_path)
    if script == null:
        return false
    var options := {
        "idle_quit_seconds": _value_of(arguments, IDLE_ARGUMENT),
        "keep_mouse": arguments.has(KEEP_MOUSE_ARGUMENT),
        "adapter": str(ProjectSettings.get_setting(ADAPTER_SETTING, "")),
        "tree": get_tree(),
        "arguments": arguments,
    }
    session = script.new(directory, quit if quit.is_valid() else _quit_the_tree, options)
    session.start()
    enabled = true
    set_physics_process(true)
    return true

func _physics_process(_delta: float) -> void:
    if session != null:
        session.physics_tick()

## The tree takes every node out when the game ends, whoever ended it; the session says so when the
## game ended on its own.
func _exit_tree() -> void:
    if session != null:
        session.game_exiting()

func _quit_the_tree() -> void:
    get_tree().quit()

func _runtime_path() -> String:
    return (get_script() as Script).resource_path.get_base_dir().path_join(RUNTIME_SESSION)

## The value after `prefix` in the first argument that starts with it, or an empty string.
func _value_of(arguments: PackedStringArray, prefix: String) -> String:
    for argument in arguments:
        if argument.begins_with(prefix):
            return argument.substr(prefix.length())
    return ""

func _can_write_in(directory: String) -> bool:
    if not DirAccess.dir_exists_absolute(directory):
        return false
    var probe := directory.path_join(".probe")
    var file := FileAccess.open(probe, FileAccess.WRITE)
    if file == null:
        return false
    file.close()
    DirAccess.remove_absolute(probe)
    return true
