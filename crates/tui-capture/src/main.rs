use anyhow::{Context, Result, bail};
use clap::Parser;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};
use vt100::Screen;

#[path = "support.rs"]
mod support;

use support::*;

#[derive(Parser, Debug)]
#[command(
    name = "tui-capture",
    about = "Capture a terminal TUI screen as a deterministic PNG"
)]
struct Args {
    #[arg(long, default_value_t = 160)]
    cols: u16,
    #[arg(long, default_value_t = 48)]
    rows: u16,
    #[arg(long, default_value = "tui.png")]
    out: PathBuf,
    #[arg(long)]
    font: Option<PathBuf>,
    #[arg(long = "font-size", default_value_t = 19.0)]
    font_px: f32,
    #[arg(long = "cell-w", default_value_t = 12)]
    cell_w: u32,
    #[arg(long = "cell-h", default_value_t = 23)]
    cell_h: u32,
    #[arg(long, default_value_t = 14)]
    padding: u32,
    #[arg(long, default_value = "#17212b")]
    bg: String,
    #[arg(long, default_value = "#f4fbff")]
    fg: String,
    #[arg(long, default_value_t = 1.35)]
    brighten: f32,
    #[arg(long = "respect-dim", default_value_t = false)]
    respect_dim: bool,
    #[arg(long = "min-wait-ms", default_value_t = 1200)]
    min_wait_ms: u64,
    #[arg(long = "max-wait-ms", default_value_t = 8000)]
    max_wait_ms: u64,
    #[arg(long = "quiet-ms", default_value_t = 300)]
    quiet_ms: u64,
    #[arg(long = "send-after-ms", default_value_t = 0)]
    send_after_ms: u64,
    #[arg(long = "send", action = clap::ArgAction::Append)]
    send: Vec<String>,
    #[arg(long = "dump-text")]
    dump_text: Option<PathBuf>,
    #[arg(long = "ready-file")]
    ready_file: Option<PathBuf>,
    #[arg(
        last = true,
        required = true,
        value_parser = clap::builder::OsStringValueParser::new()
    )]
    cmd: Vec<OsString>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.cmd.is_empty() {
        bail!("missing command; use: tui-capture [opts] -- ./your-tui");
    }

    let font_path = match args.font.clone() {
        Some(path) => path,
        None => find_font()
            .context("could not find a monospace font; pass --font /path/to/DejaVuSansMono.ttf")?,
    };
    let font_bytes = fs::read(&font_path)
        .with_context(|| format!("failed to read font {}", font_path.display()))?;
    let font = rusttype::Font::try_from_vec(font_bytes)
        .with_context(|| format!("failed to parse font {}", font_path.display()))?;
    validate_required_glyphs(&font)?;

    let cfg = RenderConfig {
        scale: rusttype::Scale::uniform(args.font_px),
        cell_w: args.cell_w,
        cell_h: args.cell_h,
        padding: args.padding,
        default_bg: parse_hex_rgb(&args.bg)?,
        default_fg: parse_hex_rgb(&args.fg)?,
        brighten: args.brighten,
        respect_dim: args.respect_dim,
    };

    eprintln!(
        "capture: {}x{} cells, {}x{} px cells, font={}, out={}",
        args.cols,
        args.rows,
        cfg.cell_w,
        cfg.cell_h,
        font_path.display(),
        args.out.display()
    );

    let screen = run_and_capture(&args, cfg.cell_w, cfg.cell_h)?;
    if let Some(path) = &args.dump_text {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, screen.contents())
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    let img = render_screen(&screen, &font, &cfg);
    if let Some(parent) = args.out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    img.save(&args.out)
        .with_context(|| format!("failed to save {}", args.out.display()))?;
    eprintln!(
        "wrote {} ({}x{} px)",
        args.out.display(),
        img.width(),
        img.height()
    );
    Ok(())
}

fn run_and_capture(args: &Args, cell_w: u32, cell_h: u32) -> Result<Screen> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: args.rows,
        cols: args.cols,
        pixel_width: clamp_u16(u32::from(args.cols) * cell_w),
        pixel_height: clamp_u16(u32::from(args.rows) * cell_h),
    })?;

    let mut cmd = CommandBuilder::new(&args.cmd[0]);
    for arg in args.cmd.iter().skip(1) {
        cmd.arg(arg);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("LANG", "C.UTF-8");
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.env("COLUMNS", args.cols.to_string());
    cmd.env("LINES", args.rows.to_string());
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.env_remove("NO_COLOR");
    if let Some(path) = &args.ready_file {
        cmd.env("TUI_READY_FILE", path);
    }

    let mut child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    thread::spawn(move || {
        let mut buf = [0u8; 16384];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut parser = vt100::Parser::new(args.rows, args.cols, 2000);
    let start = Instant::now();
    let mut last_output = Instant::now();
    let mut sent = args.send.is_empty();
    let min_wait = Duration::from_millis(args.min_wait_ms);
    let max_wait = Duration::from_millis(args.max_wait_ms);
    let quiet = Duration::from_millis(args.quiet_ms);
    let send_after = Duration::from_millis(args.send_after_ms);

    loop {
        if !sent && start.elapsed() >= send_after {
            for s in &args.send {
                writer.write_all(&decode_escapes(s))?;
                writer.flush()?;
                thread::sleep(Duration::from_millis(30));
            }
            sent = true;
        }

        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(bytes) => {
                parser.process(&bytes);
                last_output = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if start.elapsed() >= max_wait {
            break;
        }
        let ready = match &args.ready_file {
            Some(path) => path.metadata().is_ok_and(|meta| meta.len() > 0),
            None => true,
        };
        if ready && start.elapsed() >= min_wait && last_output.elapsed() >= quiet {
            break;
        }
    }

    let screen = parser.screen().clone();
    let _ = child.kill();
    let _ = child.wait();
    Ok(screen)
}
