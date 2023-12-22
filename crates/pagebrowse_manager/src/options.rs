use clap::{arg, command, value_parser, ArgMatches};

pub fn get_cli_matches() -> ArgMatches {
    command!()
        .arg(
            arg!(
                -c --count <NUM> "How many windows should exist in PageBrowse's pool"
            )
            .required(true)
            .value_parser(value_parser!(usize)),
        )
        .arg(
            arg!(
                --visible ... "Show windows"
            )
            .action(clap::ArgAction::SetTrue),
        )
        .get_matches()
}
