//! The README documents every verb the client has.

use clap::CommandFactory;
use godot_dev_bridge::cli::Cli;

/// `--help` is what an agent reads first, so no argument may be left without a description.
#[test]
fn every_argument_of_every_verb_has_help_text() {
    let mut missing = Vec::new();
    for command in Cli::command().get_subcommands() {
        if command.get_name() == "help" {
            continue;
        }
        for argument in command.get_arguments() {
            let id = argument.get_id().as_str();
            if id != "help" && id != "version" && argument.get_help().is_none() {
                missing.push(format!("{} {id}", command.get_name()));
            }
        }
    }
    assert!(missing.is_empty(), "arguments without help: {missing:?}");
}

#[test]
fn the_readme_documents_every_verb() {
    let readme =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
    for verb in Cli::command()
        .get_subcommands()
        .map(|command| command.get_name().to_string())
        .filter(|name| name != "help")
    {
        assert!(
            readme.contains(&format!("`{verb}")),
            "README.md does not document `{verb}`"
        );
    }
}
