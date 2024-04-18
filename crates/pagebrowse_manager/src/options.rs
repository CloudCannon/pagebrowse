use clap::{arg, command, value_parser, ArgMatches};

pub fn get_cli_matches() -> ArgMatches {
    command!().get_matches()
}
