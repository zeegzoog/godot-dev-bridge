//! The command line: clap definitions and the one-to-one mapping from verbs to protocol commands.

use crate::config::Context;
use crate::error::{ClientError, Result};
use crate::msys::unmangle_node_path;
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "godot-dev-bridge",
    version,
    about = "Drive a running Godot 4 game from a shell: hold it still, step it, inject input, read state, take screenshots"
)]
pub struct Cli {
    /// Session name (default: $DEV_BRIDGE_SESSION, then "default")
    #[arg(long, global = true)]
    pub session: Option<String>,
    /// Seconds to wait for the game (default 30; 90 for start)
    #[arg(long, global = true)]
    pub timeout: Option<f64>,
    #[command(subcommand)]
    pub verb: Verb,
}

#[derive(Args, Debug)]
pub struct StartArgs {
    /// The Godot executable (default: GODOT_PATH, GODOT, then `godot` on PATH)
    #[arg(long)]
    pub godot: Option<String>,
    /// The project directory (default: found from the working directory)
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// A scene to run instead of the main scene, as a res:// path
    #[arg(long)]
    pub scene: Option<String>,
    /// The fixed frame rate, equal to the physics rate (default: the project's physics ticks per
    /// second, 60 when it sets none)
    #[arg(long)]
    pub fps: Option<u32>,
    /// No window (no screenshots)
    #[arg(long)]
    pub headless: bool,
    /// Window size WxH
    #[arg(long, default_value = "1152x648")]
    pub window: String,
    /// Vsync off, for quick fast-forward
    #[arg(long)]
    pub fast: bool,
    /// Seconds without a request before the game ends itself (0 disables)
    #[arg(long)]
    pub idle_quit: Option<u64>,
    /// Leave the pointer as the game sets it
    #[arg(long)]
    pub keep_mouse: bool,
    /// More engine arguments, after `--`
    #[arg(last = true)]
    pub engine_args: Vec<String>,
}

#[derive(Args, Debug)]
pub struct InputArgs {
    /// An action of the project's input map, for example ui_accept
    #[arg(long)]
    pub action: Option<String>,
    /// A keyboard key by its engine name, for example Space, A or Escape
    #[arg(long)]
    pub key: Option<String>,
    /// A mouse button: left, right or middle
    #[arg(long)]
    pub button: Option<String>,
    /// Release after this many ticks of stepping (press: held until released when omitted; tap: 2)
    #[arg(long)]
    pub ticks: Option<u64>,
}

impl InputArgs {
    fn to_args(&self) -> Value {
        let mut args = json!({});
        if let Some(action) = &self.action {
            args["action"] = json!(action);
        }
        if let Some(key) = &self.key {
            args["key"] = json!(key);
        }
        if let Some(button) = &self.button {
            args["button"] = json!(button);
        }
        if let Some(ticks) = self.ticks {
            args["ticks"] = json!(ticks);
        }
        args
    }
}

#[derive(Subcommand, Debug)]
pub enum Verb {
    /// Start the game with the bridge enabled and wait until it is ready
    Start(StartArgs),
    /// Ask the game to quit, ending it if it stays
    Stop,
    /// List the sessions on this machine, whether each game is alive and why an ended one closed
    Sessions {
        /// First remove the directories of the sessions that have ended, logs and screenshots
        /// included
        #[arg(long)]
        prune: bool,
    },
    /// Information about the session
    Status,
    /// Check that the game answers
    Ping,
    /// List the core and project commands
    Commands,
    /// Run N physics ticks, or until an expression holds, then hold still again
    Step {
        /// How many physics ticks to run
        ticks: Option<u64>,
        /// An expression; step until it holds
        #[arg(long)]
        until: Option<String>,
        /// Most ticks an --until step may run (default 600)
        #[arg(long, requires = "until")]
        max: Option<u64>,
    },
    /// Press an action, key or mouse button and hold it
    Press(InputArgs),
    /// Release an action, key or mouse button
    Release(InputArgs),
    /// Press and release after --ticks ticks (default 2)
    Tap(InputArgs),
    /// Release everything held
    ReleaseAll,
    /// Move the pointer in viewport pixels
    Pointer {
        /// Horizontal position in viewport pixels
        #[arg(allow_negative_numbers = true)]
        x: f64,
        /// Vertical position in viewport pixels
        #[arg(allow_negative_numbers = true)]
        y: f64,
        /// Move by x and y instead of to them
        #[arg(long)]
        relative: bool,
    },
    /// Save the window's last drawn frame as a PNG
    Shot {
        /// Where to save the PNG, relative to the current directory (default: shots/ in the session)
        path: Option<String>,
        /// Scale to this many pixels across (at most 16384), keeping the proportions
        #[arg(long)]
        width: Option<u64>,
    },
    /// Evaluate an expression (root, tree, bridge and every autoload are in scope)
    Eval {
        /// The expression; several words are joined with spaces
        #[arg(required = true, allow_hyphen_values = true)]
        expression: Vec<String>,
    },
    /// Read properties of a node
    Get {
        /// The node, absolute (/root/Player) or relative to the root
        path: String,
        /// The properties to read; a name may be indexed, as in position:x
        properties: Vec<String>,
    },
    /// List nodes
    Tree {
        /// The node to start from (default /root)
        path: Option<String>,
        /// How many levels to list (default 2)
        #[arg(long)]
        depth: Option<u64>,
        /// Add visible and process_mode
        #[arg(long)]
        props: bool,
    },
    /// Read the state the project's adapter registered
    State {
        /// The state providers to read (default: all)
        names: Vec<String>,
    },
    /// Run a command the project's adapter registered
    Do {
        /// The name of a project command
        name: String,
        /// The arguments, as a JSON object (default {})
        json: Option<String>,
    },
    /// Send a request as written
    Raw {
        /// The whole request, as JSON: {"cmd": "...", "args": {...}}
        json: String,
    },
}

fn parse_json(text: &str, what: &str) -> Result<Value> {
    serde_json::from_str(text)
        .map_err(|error| ClientError::msg(format!("{what} could not be parsed as JSON: {error}")))
}

/// The `(command, args)` a verb stands for, one for one. The lifecycle verbs have no command.
pub fn command_for(verb: &Verb, ctx: &Context) -> Result<(String, Value)> {
    Ok(match verb {
        Verb::Status => ("status".into(), json!({})),
        Verb::Ping => ("ping".into(), json!({})),
        Verb::Commands => ("commands".into(), json!({})),
        Verb::ReleaseAll => ("release_all".into(), json!({})),
        Verb::Step { ticks, until, max } => match (until, ticks) {
            (Some(expression), _) => {
                let mut args = json!({"until": expression});
                if let Some(max) = max {
                    args["max_ticks"] = json!(max);
                }
                ("step".into(), args)
            }
            (None, Some(ticks)) => ("step".into(), json!({"ticks": ticks})),
            (None, None) => {
                return Err(ClientError::msg(
                    "step needs a number of ticks, or --until EXPRESSION",
                ));
            }
        },
        Verb::Press(input) => ("press".into(), input.to_args()),
        Verb::Release(input) => ("release".into(), input.to_args()),
        Verb::Tap(input) => ("tap".into(), input.to_args()),
        Verb::Pointer { x, y, relative } => {
            let mut args = json!({"x": x, "y": y});
            if *relative {
                args["relative"] = json!(true);
            }
            ("pointer".into(), args)
        }
        Verb::Shot { path, width } => {
            let mut args = json!({});
            if let Some(path) = path {
                let path = PathBuf::from(path);
                let absolute = if path.is_absolute() {
                    path
                } else {
                    ctx.cwd.join(path)
                };
                args["path"] = json!(absolute.to_string_lossy());
            }
            if let Some(width) = width {
                args["width"] = json!(width);
            }
            ("shot".into(), args)
        }
        Verb::Eval { expression } => ("eval".into(), json!({"expr": expression.join(" ")})),
        Verb::Get { path, properties } => {
            let mut args = json!({"path": unmangle_node_path(path, ctx)});
            if !properties.is_empty() {
                args["properties"] = json!(properties);
            }
            ("get".into(), args)
        }
        Verb::Tree { path, depth, props } => {
            let mut args = json!({});
            if let Some(path) = path {
                args["path"] = json!(unmangle_node_path(path, ctx));
            }
            if let Some(depth) = depth {
                args["depth"] = json!(depth);
            }
            if *props {
                args["properties"] = json!(true);
            }
            ("tree".into(), args)
        }
        Verb::State { names } => (
            "state".into(),
            if names.is_empty() {
                json!({})
            } else {
                json!({"names": names})
            },
        ),
        Verb::Do { name, json: text } => {
            let args = match text {
                Some(text) => parse_json(text, "the arguments")?,
                None => json!({}),
            };
            (name.clone(), args)
        }
        Verb::Raw { json: text } => {
            let request = parse_json(text, "the request")?;
            let Some(command) = request.get("cmd").and_then(Value::as_str) else {
                return Err(ClientError::msg("the request has no \"cmd\" string"));
            };
            (
                command.to_string(),
                request.get("args").cloned().unwrap_or_else(|| json!({})),
            )
        }
        Verb::Start(_) | Verb::Stop | Verb::Sessions { .. } => {
            return Err(ClientError::msg("this verb is not a protocol command"));
        }
    })
}
