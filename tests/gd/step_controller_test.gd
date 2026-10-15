extends SceneTree

const Check := preload("res://tests/gd/check.gd")
const StepController := preload("res://addons/godot_dev_bridge/runtime/step_controller.gd")

func _initialize() -> void:
    var check := Check.new()

    var fresh := StepController.new()
    check.is_true(fresh.at_tick_start(), "a controller that was never begun parks")
    check.equal(fresh.total_ticks, 0, "no ticks yet")

    var stepping := StepController.new()
    stepping.begin(3)
    var let_through := 0
    while not stepping.at_tick_start():
        let_through += 1
        if let_through > 10:
            break
    check.equal(let_through, 3, "a step of 3 lets exactly 3 ticks through")
    check.equal(stepping.ticks_run, 3, "ticks_run counts them")
    check.equal(stepping.total_ticks, 3, "total_ticks counts them")
    check.is_true(not stepping.active, "the step ends")
    check.is_true(not stepping.until_met, "no condition was involved")

    var zero := StepController.new()
    zero.begin(0)
    check.is_true(zero.at_tick_start(), "a step of 0 parks at once")
    check.equal(zero.ticks_run, 0, "and runs nothing")

    var negative := StepController.new()
    negative.begin(-5)
    check.is_true(negative.at_tick_start(), "a negative limit counts as 0")

    var state := {"ticks": 0}
    var until := StepController.new()
    until.begin(100, func() -> bool: return state.ticks >= 2)
    var seen := 0
    while not until.at_tick_start():
        seen += 1
        state.ticks = seen
        if seen > 10:
            break
    check.equal(seen, 2, "a step with a condition stops the moment it holds")
    check.is_true(until.until_met, "and says so")

    var already := StepController.new()
    already.begin(10, func() -> bool: return true)
    check.is_true(already.at_tick_start(), "a condition that already holds parks before any tick")
    check.equal(already.ticks_run, 0, "and runs none")
    check.is_true(already.until_met, "and is met")

    var never := StepController.new()
    never.begin(4, func() -> bool: return false)
    var bounded := 0
    while not never.at_tick_start():
        bounded += 1
        if bounded > 10:
            break
    check.equal(bounded, 4, "a condition that never holds is bounded by the limit")
    check.is_true(not never.until_met, "and is not met")

    var twice := StepController.new()
    twice.begin(2)
    while not twice.at_tick_start():
        pass
    twice.begin(3)
    while not twice.at_tick_start():
        pass
    check.equal(twice.total_ticks, 5, "total_ticks accumulates over steps")
    check.equal(twice.ticks_run, 3, "ticks_run is per step")

    check.finish(self, "step_controller_test")
