//! The README documents every verb the client has.

use clap::CommandFactory;
use godot_dev_bridge::cli::Cli;

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
