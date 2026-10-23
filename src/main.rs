mod app;
mod ping;
mod ui;

use std::process::Child;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use app::App;
use ping::PingEvent;

/// Ping multiple hosts and graph their response times.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Seconds between pings (passed to `ping -i`).
    #[arg(short, long, default_value_t = 1.0)]
    interval: f64,

    /// Hosts or IP addresses to ping.
    #[arg(required = true)]
    hosts: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let (tx, rx) = mpsc::channel();
    let mut children: Vec<Child> = Vec::new();
    for (idx, host) in args.hosts.iter().enumerate() {
        match ping::spawn(idx, host, args.interval, tx.clone()) {
            Ok(child) => children.push(child),
            Err(e) => {
                kill_all(&mut children);
                return Err(e);
            }
        }
    }
    drop(tx);

    let terminal = ratatui::init();
    let result = run(terminal, App::new(args.hosts), rx);
    ratatui::restore();
    kill_all(&mut children);
    result
}

fn run(mut terminal: DefaultTerminal, mut app: App, rx: Receiver<(usize, PingEvent)>) -> Result<()> {
    loop {
        while let Ok((idx, event)) = rx.try_recv() {
            app.apply(idx, event);
        }
        terminal.draw(|frame| ui::render(frame, &app))?;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
            if ctrl_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(());
            }
        }
    }
}

fn kill_all(children: &mut [Child]) {
    for child in children {
        let _ = child.kill();
        let _ = child.wait();
    }
}
