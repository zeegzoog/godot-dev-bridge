extends RefCounted

## The counting behind `step`, with no engine in it: how many ticks a step may still run, whether an
## optional condition has come true, and how many ticks have been let through in all. The game's loop
## asks it at the start of every tick whether to park; the answer is "no" for exactly the ticks the step
## allows. The condition is asked before every tick, the first included, so a step whose condition
## already holds runs none, and a step is bounded even when its condition never holds.

## Whether a step is running.
var active: bool = false

## The ticks the running (or last) step let through, and whether its condition came true.
var ticks_run: int = 0
var until_met: bool = false

## Ticks let through by every step since this was made: the clock scheduled releases count in.
var total_ticks: int = 0

var _limit: int = 0
var _condition: Callable = Callable()

## Starts a step of at most `limit` ticks that also ends when `condition` (a callable returning a
## bool) holds. Nothing runs until `at_tick_start()` allows it.
func begin(limit: int, condition: Callable = Callable()) -> void:
    active = true
    ticks_run = 0
    until_met = false
    _limit = maxi(limit, 0)
    _condition = condition

## Asked at the start of a tick that no other node has run yet: `true` means park now, `false` means
## let this tick run (and count it). A step that has just ended, or was never begun, parks.
func at_tick_start() -> bool:
    if not active:
        return true
    if _condition.is_valid() and bool(_condition.call()):
        until_met = true
        active = false
        return true
    if ticks_run >= _limit:
        active = false
        return true
    ticks_run += 1
    total_ticks += 1
    return false
