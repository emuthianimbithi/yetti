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
                    crate::config::reload_config(&path);
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

    watch_config_file(yetii.file.clone()); // 🧠 start watching here

    // going through yetii commands
    commands::going_through_commands(&yetii);
}