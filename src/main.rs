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
    // going through the commands of yetii
    let yetii = cli::Yetii::parse();

    if let Err(e) = config::load_config_once(&yetii.file) {
        eprintln!("❌ Failed to load config: {}", e);
        std::process::exit(1);
    }

    // check if config initialized
    if !config::is_config_initialized() {
        eprintln!("❌ Yetii configuration is not initialized. Please run `yetii init` first.");
        std::process::exit(1);
    }

    watch_config_file(yetii.file.clone()); // 🧠 start watching here

    // going through yetii commands
    commands::going_through_commands(&yetii);
}