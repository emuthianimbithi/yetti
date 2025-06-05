use clap::Parser;
mod cli;
mod commands;
mod database;
mod config;
use notify::{Watcher, RecursiveMode, Config, RecommendedWatcher, EventKind};
use std::sync::mpsc::channel;
use std::thread;

fn watch_config_file(path: String) {
    thread::spawn(move || {
        let (tx, rx) = channel();

        let mut watcher = RecommendedWatcher::new(tx, Config::default()).expect("Watcher failed");
        watcher.watch((&path).as_ref(), RecursiveMode::NonRecursive).expect("Watch failed");

        println!("👀 Watching config file: {}", path);

        while let Ok(event) = rx.recv() {
            if let Ok(e) = event {
                if matches!(e.kind, EventKind::Modify(_)) {
                    config::reload_config(&path).
                        expect("Failed to reload config");
                }
            }
        }
    });
}

fn main() {
    let yetii = cli::Yetii::parse();

    // Handle init command separately since it doesn't need existing config
    if matches!(yetii.commands, cli::Commands::Init { .. }) {
        commands::going_through_commands(&yetii);
        return;
    }

    // For all other commands, load and validate config
    if let Err(e) = config::load_config_once(&yetii.file) {
        eprintln!("❌ Failed to load config: {}", e);
        std::process::exit(1);
    }

    if !config::is_config_initialized() {
        eprintln!("❌ Yetii configuration is not initialized. Please run `yetii init` first.");
        std::process::exit(1);
    }

    // Only start file watcher for the `run` command (assuming it's long-running)
    if matches!(yetii.commands, cli::Commands::Run { .. }) {
        watch_config_file(yetii.file.clone());
    }

    commands::going_through_commands(&yetii);
}