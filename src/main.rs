use clap::Parser;
mod cli;
mod utils;
mod commands;

mod database;
mod config;

fn main() {
    // going through the commands of yetii
    let yetii = cli::Yetii::parse();

    // going through yetii commands
    commands::going_through_commands(&yetii);
}