use anyhow::{Context, Result, bail};
use clap::Parser;
use image::{Rgba, RgbaImage};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use rusttype::{Font, PositionedGlyph, Scale, point};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};
use vt100::{Color as VtColor, Screen};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rgb8 {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Clone, Copy)]
struct RenderConfig {
    scale: Scale,
    cell_w: u32,
    cell_h: u32,
    padding: u32,
    default_bg: Rgb8,
    default_fg: Rgb8,
    brighten: f32,
    respect_dim: bool,
}

struct CellText<'a> {
    x: u32,
    y: u32,
    text: &'a str,
    color: Rgb8,
    bold: bool,
    underline: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.cmd.is_empty() {
        bail!("missing command; use: tui-capture [opts] -- ./your-tui");
    }

    let font_path = match args.font.clone() {
        Some(path) => path,
        None => find_font().context(
            "could not find a monospace font; pass --font /path/to/DejaVuSansMono.ttf",
        )?,
    };
    let font_bytes = fs::read(&font_path)
        .with_context(|| format!("failed to read font {}", font_path.display()))?;
    let font = Font::try_from_vec(font_bytes)
        .with_context(|| format!("failed to parse font {}", font_path.display()))?;
    validate_required_glyphs(&font)?;

    let cfg = RenderConfig {
        scale: Scale::uniform(args.font_px),
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

fn render_screen(screen: &Screen, font: &Font<'_>, cfg: &RenderConfig) -> RgbaImage {
    let (rows, cols) = screen.size();
    let width = cfg.padding * 2 + u32::from(cols) * cfg.cell_w;
    let height = cfg.padding * 2 + u32::from(rows) * cfg.cell_h;
    let mut img = RgbaImage::from_pixel(width, height, rgba(cfg.default_bg));

    for row in 0..rows {
        for col in 0..cols {
            if let Some(cell) = screen.cell(row, col) {
                let (_, bg) = cell_colors(cell, cfg);
                fill_rect(
                    &mut img,
                    cfg.padding + u32::from(col) * cfg.cell_w,
                    cfg.padding + u32::from(row) * cfg.cell_h,
                    cfg.cell_w,
                    cfg.cell_h,
                    bg,
                );
            }
        }
    }

    for row in 0..rows {
        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() || !cell.has_contents() {
                continue;
            }
            let text = cell.contents();
            if text.is_empty() || text == " " {
                continue;
            }
            let (fg, _) = cell_colors(cell, cfg);
            draw_cell_text(
                &mut img,
                font,
                cfg,
                CellText {
                    x: cfg.padding + u32::from(col) * cfg.cell_w,
                    y: cfg.padding + u32::from(row) * cfg.cell_h,
                    text,
                    color: fg,
                    bold: cell.bold(),
                    underline: cell.underline(),
                },
            );
        }
    }

    img
}

fn cell_colors(cell: &vt100::Cell, cfg: &RenderConfig) -> (Rgb8, Rgb8) {
    let mut fg = resolve_vt_color(cell.fgcolor(), false, cfg);
    let mut bg = resolve_vt_color(cell.bgcolor(), true, cfg);
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.dim() && cfg.respect_dim {
        fg = scale_rgb(fg, 0.62);
    }
    if cell.bold() {
        fg = boost_rgb(fg, 1.08, 4.0);
    }
    (fg, bg)
}

fn draw_cell_text(img: &mut RgbaImage, font: &Font<'_>, cfg: &RenderConfig, cell: CellText<'_>) {
    let v_metrics = font.v_metrics(cfg.scale);
    let line_h = v_metrics.ascent - v_metrics.descent;
    let baseline = cell.y as f32 + ((cfg.cell_h as f32 - line_h) / 2.0).floor() + v_metrics.ascent;
    let mut caret = cell.x as f32;

    for ch in cell.text.chars().filter(|ch| !ch.is_control()) {
        let glyph = font.glyph(ch).scaled(cfg.scale);
        let advance = glyph.h_metrics().advance_width;
        draw_glyph(img, glyph.positioned(point(caret, baseline)), cell.color);
        if cell.bold {
            let bold_glyph = font
                .glyph(ch)
                .scaled(cfg.scale)
                .positioned(point(caret + 0.7, baseline));
            draw_glyph(img, bold_glyph, cell.color);
        }
        caret += advance;
    }

    if cell.underline {
        fill_rect(
            img,
            cell.x,
            cell.y + cfg.cell_h.saturating_sub(3),
            cfg.cell_w,
            2,
            cell.color,
        );
    }
}

fn draw_glyph(img: &mut RgbaImage, glyph: PositionedGlyph<'_>, color: Rgb8) {
    let Some(bb) = glyph.pixel_bounding_box() else {
        return;
    };
    glyph.draw(|gx, gy, coverage| {
        let px = bb.min.x + gx as i32;
        let py = bb.min.y + gy as i32;
        if px < 0 || py < 0 {
            return;
        }
        let px = px as u32;
        let py = py as u32;
        if px >= img.width() || py >= img.height() {
            return;
        }
        alpha_blend(img.get_pixel_mut(px, py), color, coverage);
    });
}

fn alpha_blend(dst: &mut Rgba<u8>, src: Rgb8, alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - alpha;
    dst.0[0] = (src.r as f32 * alpha + dst.0[0] as f32 * inv).round() as u8;
    dst.0[1] = (src.g as f32 * alpha + dst.0[1] as f32 * inv).round() as u8;
    dst.0[2] = (src.b as f32 * alpha + dst.0[2] as f32 * inv).round() as u8;
    dst.0[3] = 255;
}

fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgb8) {
    let x2 = x.saturating_add(w).min(img.width());
    let y2 = y.saturating_add(h).min(img.height());
    for yy in y..y2 {
        for xx in x..x2 {
            img.put_pixel(xx, yy, rgba(color));
        }
    }
}

fn rgba(c: Rgb8) -> Rgba<u8> {
    Rgba([c.r, c.g, c.b, 255])
}

fn resolve_vt_color(c: VtColor, is_bg: bool, cfg: &RenderConfig) -> Rgb8 {
    let raw = match c {
        VtColor::Default => {
            if is_bg {
                cfg.default_bg
            } else {
                cfg.default_fg
            }
        }
        VtColor::Idx(i) => xterm_256_color(i, cfg.default_bg),
        VtColor::Rgb(r, g, b) => Rgb8 { r, g, b },
    };
    if is_bg {
        match c {
            VtColor::Default | VtColor::Idx(0) => cfg.default_bg,
            _ => boost_rgb(raw, 1.06, 0.0),
        }
    } else {
        boost_rgb(raw, cfg.brighten, 6.0)
    }
}

fn xterm_256_color(idx: u8, default_black: Rgb8) -> Rgb8 {
    const ANSI16: [Rgb8; 16] = [
        Rgb8 {
            r: 18,
            g: 25,
            b: 35,
        },
        Rgb8 {
            r: 255,
            g: 86,
            b: 96,
        },
        Rgb8 {
            r: 64,
            g: 230,
            b: 125,
        },
        Rgb8 {
            r: 255,
            g: 238,
            b: 88,
        },
        Rgb8 {
            r: 95,
            g: 174,
            b: 255,
        },
        Rgb8 {
            r: 255,
            g: 108,
            b: 255,
        },
        Rgb8 {
            r: 70,
            g: 235,
            b: 255,
        },
        Rgb8 {
            r: 232,
            g: 238,
            b: 245,
        },
        Rgb8 {
            r: 120,
            g: 132,
            b: 148,
        },
        Rgb8 {
            r: 255,
            g: 112,
            b: 122,
        },
        Rgb8 {
            r: 88,
            g: 255,
            b: 145,
        },
        Rgb8 {
            r: 255,
            g: 255,
            b: 112,
        },
        Rgb8 {
            r: 125,
            g: 195,
            b: 255,
        },
        Rgb8 {
            r: 255,
            g: 140,
            b: 255,
        },
        Rgb8 {
            r: 105,
            g: 250,
            b: 255,
        },
        Rgb8 {
            r: 255,
            g: 255,
            b: 255,
        },
    ];
    match idx {
        0 => default_black,
        1..=15 => ANSI16[usize::from(idx)],
        16..=231 => {
            let i = idx - 16;
            let conv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Rgb8 {
                r: conv(i / 36),
                g: conv((i % 36) / 6),
                b: conv(i % 6),
            }
        }
        232..=255 => {
            let v = 8 + (idx - 232) * 10;
            Rgb8 { r: v, g: v, b: v }
        }
    }
}

fn boost_rgb(c: Rgb8, factor: f32, add: f32) -> Rgb8 {
    let f = |v: u8| (v as f32 * factor + add).round().clamp(0.0, 255.0) as u8;
    Rgb8 {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

fn scale_rgb(c: Rgb8, factor: f32) -> Rgb8 {
    let f = |v: u8| (v as f32 * factor).round().clamp(0.0, 255.0) as u8;
    Rgb8 {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

fn parse_hex_rgb(s: &str) -> Result<Rgb8> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        bail!("expected #RRGGBB color, got {s:?}");
    }
    Ok(Rgb8 {
        r: u8::from_str_radix(&s[0..2], 16)?,
        g: u8::from_str_radix(&s[2..4], 16)?,
        b: u8::from_str_radix(&s[4..6], 16)?,
    })
}

fn decode_escapes(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut it = s.as_bytes().iter().copied();
    while let Some(b) = it.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match it.next() {
            Some(b'r') => out.push(b'\r'),
            Some(b'n') => out.push(b'\n'),
            Some(b't') => out.push(b'\t'),
            Some(b'e') => out.push(0x1b),
            Some(b'\\') => out.push(b'\\'),
            Some(other) => {
                out.push(b'\\');
                out.push(other);
            }
            None => out.push(b'\\'),
        }
    }
    out
}

fn clamp_u16(v: u32) -> u16 {
    v.min(u32::from(u16::MAX)) as u16
}

fn find_font() -> Option<PathBuf> {
    [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoMono-Regular.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

fn validate_required_glyphs(font: &Font<'_>) -> Result<()> {
    let required = [
        '─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '█', '░', '▒', '▓', '▁', '▂', '▃',
        '▄', '▅', '▆', '▇', '●',
    ];
    let missing: String = required
        .into_iter()
        .filter(|ch| font.glyph(*ch).id().0 == 0)
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        bail!("selected font is missing required TUI glyphs: {missing:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_color_parser_accepts_hash() {
        assert_eq!(
            parse_hex_rgb("#17212b").unwrap(),
            Rgb8 {
                r: 0x17,
                g: 0x21,
                b: 0x2b
            }
        );
    }

    #[test]
    fn escape_decoder_handles_terminal_sequences() {
        assert_eq!(
            decode_escapes(r"\e[2J\r\n\t\\"),
            b"\x1b[2J\r\n\t\\".to_vec()
        );
    }

    #[test]
    fn default_font_has_required_glyphs_when_installed() {
        let Some(path) = find_font() else {
            return;
        };
        let bytes = fs::read(path).unwrap();
        let font = Font::try_from_vec(bytes).unwrap();
        validate_required_glyphs(&font).unwrap();
    }
}
