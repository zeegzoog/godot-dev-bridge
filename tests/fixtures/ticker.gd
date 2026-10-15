extends Node

## A scene that makes the passing of game time observable from outside: it counts physics ticks and
## process frames and records when the watched action is seen pressed. Whatever holds the game still
## shows up as counts that do not move.

## The built-in action the scene watches, to show when an injected press is seen and for how long.
const WATCHED_ACTION := "ui_accept"

var ticks: int = 0
var frames: int = 0
var pressed_ticks: int = 0
var just_pressed_at: Array[int] = []

func _physics_process(_delta: float) -> void:
    ticks += 1
    if Input.is_action_just_pressed(WATCHED_ACTION):
        just_pressed_at.append(ticks)
    if Input.is_action_pressed(WATCHED_ACTION):
        pressed_ticks += 1

func _process(_delta: float) -> void:
    frames += 1
