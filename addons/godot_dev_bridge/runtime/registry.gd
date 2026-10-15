extends RefCounted

## What a project tells the generic core about its game: state it can read and commands it can run,
## registered by name by the project's adapter. The core knows nothing of the game; this is the one
## place the game's own names enter. A project command may not take the name of a core command.

## A project command refuses its arguments by returning a dictionary whose only key is this one,
## with the message as its value; the driver gets a `bad_args` error. A key of the adapter contract
## that no command result is likely to have: what an `error` entry means is the project's own.
const BAD_ARGS_KEY := "__bad_args__"

## Names the core owns; set by the session.
var reserved: Array[String] = []

var _states: Dictionary = {}  # name -> Callable
var _commands: Dictionary = {}  # name -> {callable, help}

## Registers a state provider: `provider` takes no arguments and returns any value. Refused (logged,
## not raised) when the name is empty or the provider is invalid or wants arguments: calling it
## would fail inside the engine, and the driver would be told nothing useful.
func add_state(name: String, provider: Callable) -> bool:
    if name.is_empty() or not provider.is_valid() or provider.get_argument_count() != 0:
        push_warning(
            "godot_dev_bridge: refused state provider '%s' (it must be valid and take no arguments)" % name
        )
        return false
    _states[name] = provider
    return true


## Registers a project command: `handler` takes the arguments dictionary and returns any value.
## Refused (logged, not raised) when the name is empty or taken by the core, or the handler is
## invalid or does not take exactly one argument. A handler that fails inside its own script cannot
## be told from one that returns nothing (the engine logs the script error and the call yields
## `null`), so a command that can fail should say so in what it returns: `{BAD_ARGS_KEY: message}`
## to refuse its arguments, or a value that describes the failure.
func add_command(name: String, handler: Callable, help: String = "") -> bool:
    if name.is_empty() or reserved.has(name):
        push_warning("godot_dev_bridge: refused project command '%s' (empty or core name)" % name)
        return false
    if not handler.is_valid() or handler.get_argument_count() != 1:
        push_warning(
            "godot_dev_bridge: refused project command '%s' (it must be valid and take the arguments dictionary)" % name
        )
        return false
    _commands[name] = {"callable": handler, "help": help}
    return true


func state_names() -> Array[String]:
    var names: Array[String] = []
    for name: String in _states:
        names.append(name)
    return names

func command_names() -> Array[String]:
    var names: Array[String] = []
    for name: String in _commands:
        names.append(name)
    return names

func has_command(name: String) -> bool:
    return _commands.has(name)

## The help text of every project command, by name.
func command_help() -> Dictionary:
    var help := {}
    for name: String in _commands:
        help[name] = _commands[name].help
    return help

## Runs project command `name` with `args`: `{value}` (any value, `null` included), `{bad_args}`
## (the message) when the command refused its arguments, or `{error}` when the command's handler is
## no longer valid. A handler that returns nothing is a `null` value.
func run_command(name: String, args: Dictionary) -> Dictionary:
    var handler: Callable = _commands[name].callable
    if not handler.is_valid():
        return {"error": "The handler of command '%s' is no longer valid." % name}
    var value: Variant = handler.call(args)
    if value is Dictionary:
        var answer: Dictionary = value
        if answer.size() == 1 and answer.has(BAD_ARGS_KEY):
            return {"bad_args": str(answer[BAD_ARGS_KEY])}
    return {"value": value}

## What the state providers say: for each name in `names` (all providers when it is empty), its
## value, or `{error}` for a provider that is missing or no longer valid; one failing never affects
## another. Values are as the providers return them; the caller converts them.
func read_state(names: Array) -> Dictionary:
    var asked: Array = names if not names.is_empty() else state_names()
    var values := {}
    for name: Variant in asked:
        var key := str(name)
        if not _states.has(key):
            values[key] = {"error": "No state provider named '%s'." % key}
            continue
        var provider: Callable = _states[key]
        if not provider.is_valid():
            values[key] = {"error": "The provider '%s' is no longer valid." % key}
            continue
        values[key] = provider.call()
    return values
