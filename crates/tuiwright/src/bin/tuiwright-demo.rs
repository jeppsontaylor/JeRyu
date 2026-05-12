use std::io::{stdout, Write};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::Print,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

fn draw(count: i32) -> Result<()> {
    let mut out = stdout();
    queue!(
        out,
        MoveTo(0, 0),
        Clear(ClearType::All),
        Print("Counter\n"),
        Print(format!("Counter: {count}\n")),
        Print("Up/Down to change, q to quit\n")
    )?;
    out.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, Hide)?;

    let mut count = 0;
    draw(count)?;

    loop {
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match key.code {
                        KeyCode::Up => count += 1,
                        KeyCode::Down => count -= 1,
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        _ => {}
                    }
                    draw(count)?;
                }
                Event::Resize(_, _) => {
                    draw(count)?;
                }
                _ => {}
            }
        }
    }

    execute!(out, Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
