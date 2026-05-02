//! Owner: Interactive TUI subsystem (module root)
//! Proof: `cargo nextest run -p vgit -- tui`
//! Invariants: TUI entry points preserve terminal cleanup and keep operational actions policy-gated.
pub mod action_registry;
pub mod app;
pub mod events;
pub mod flow;
pub mod ui;

use anyhow::Result;
use app::App;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{error, warn};

pub async fn run_tui(
    db: crate::state::Db,
    docker_ctl: crate::docker::DockerCtl,
    client: crate::gitlab_client::GitlabClient,
) -> Result<()> {
    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let maintenance_docker = docker_ctl.clone();
    tokio::spawn(async move {
        cache_maintenance_loop(maintenance_docker).await;
    });

    let mut app = App::new(db, docker_ctl, client);
    hydrate_smoke_state(&mut app).await;

    // Start background sync
    app.start_background_sync();

    // Run loops
    let res = run_loop(&mut terminal, &mut app).await;

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

pub async fn run_tui_once(
    db: crate::state::Db,
    docker_ctl: crate::docker::DockerCtl,
    client: crate::gitlab_client::GitlabClient,
) -> Result<()> {
    use ratatui::backend::TestBackend;

    let mut app = App::new(db, docker_ctl, client);
    hydrate_smoke_state(&mut app).await;

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| ui::draw(f, &mut app))?;
    println!(
        "vgit TUI smoke render ok (live jobs: {})",
        app.state.recent_jobs.len()
    );
    Ok(())
}

pub async fn run_tui_screenshot(
    db: crate::state::Db,
    docker_ctl: crate::docker::DockerCtl,
    client: crate::gitlab_client::GitlabClient,
    tab: &str,
    hold_ms: u64,
) -> Result<()> {
    let mut app = App::new(db, docker_ctl, client);
    app.active_tab = parse_capture_tab(tab)?;
    app.apply_demo_fixture();

    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.draw(|f| ui::draw(f, &mut app))?;

    if let Ok(ready_file) = std::env::var("TUI_READY_FILE")
        && !ready_file.is_empty()
    {
        std::fs::write(std::path::Path::new(&ready_file), b"ready")?;
    }

    std::thread::sleep(Duration::from_millis(hold_ms));
    cleanup_screenshot_terminal(&mut terminal)
}

fn cleanup_screenshot_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    terminal.show_cursor()?;
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    Ok(())
}

/// Render one deterministic TUI frame into a PNG file.
pub async fn capture_tui_png(
    db: crate::state::Db,
    docker_ctl: crate::docker::DockerCtl,
    client: crate::gitlab_client::GitlabClient,
    tab: &str,
    output: &Path,
    width: u16,
    height: u16,
) -> Result<()> {
    use ratatui::backend::TestBackend;

    let mut app = App::new(db, docker_ctl, client);
    app.active_tab = parse_capture_tab(tab)?;
    hydrate_smoke_state(&mut app).await;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| ui::draw(f, &mut app))?;

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    write_buffer_png(terminal.backend().buffer(), output)?;
    Ok(())
}

fn parse_capture_tab(tab: &str) -> Result<app::ActiveTab> {
    match tab.to_ascii_lowercase().as_str() {
        "mission" => Ok(app::ActiveTab::Mission),
        "release" => Ok(app::ActiveTab::Release),
        "jobs" | "flow" => Ok(app::ActiveTab::Jobs),
        "agents" => Ok(app::ActiveTab::Agents),
        "tests" | "vti" => Ok(app::ActiveTab::Tests),
        "pools" => Ok(app::ActiveTab::Pools),
        "cache" => Ok(app::ActiveTab::Cache),
        "evidence" | "audit" => Ok(app::ActiveTab::Evidence),
        "secrets" => Ok(app::ActiveTab::Secrets),
        _ => anyhow::bail!(
            "unknown TUI tab '{}'; expected mission, release, jobs, agents, tests, pools, cache, evidence, or secrets",
            tab
        ),
    }
}

fn write_buffer_png(buffer: &ratatui::buffer::Buffer, output: &Path) -> Result<()> {
    let cell_w = 8usize;
    let cell_h = 12usize;
    let width_cells = usize::from(buffer.area.width);
    let height_cells = usize::from(buffer.area.height);
    let image_w = width_cells * cell_w;
    let image_h = height_cells * cell_h;
    let mut pixels = vec![0u8; image_w * image_h * 3];

    for y in 0..height_cells {
        for x in 0..width_cells {
            let cell = &buffer.content[y * width_cells + x];
            let bg = color_to_rgb(cell.bg, true);
            fill_rect(
                &mut pixels,
                image_w,
                x * cell_w,
                y * cell_h,
                cell_w,
                cell_h,
                bg,
            );

            let symbol = cell.symbol();
            if symbol.trim().is_empty() {
                continue;
            }
            let fg = color_to_rgb(cell.fg, false);
            if let Some(ch) = symbol.chars().next() {
                draw_glyph(&mut pixels, image_w, x * cell_w + 1, y * cell_h + 2, ch, fg);
            }
        }
    }

    write_png_rgb(output, image_w as u32, image_h as u32, &pixels)
}

fn color_to_rgb(color: ratatui::style::Color, background: bool) -> [u8; 3] {
    use ratatui::style::Color;
    match color {
        Color::Black => [0, 0, 0],
        Color::Red => [205, 49, 49],
        Color::Green => [13, 188, 121],
        Color::Yellow => [229, 229, 16],
        Color::Blue => [36, 114, 200],
        Color::Magenta => [188, 63, 188],
        Color::Cyan => [17, 168, 205],
        Color::Gray => [180, 190, 200],
        Color::DarkGray => [95, 104, 117],
        Color::LightRed => [241, 76, 76],
        Color::LightGreen => [35, 209, 139],
        Color::LightYellow => [245, 245, 67],
        Color::LightBlue => [59, 142, 234],
        Color::LightMagenta => [214, 112, 214],
        Color::LightCyan => [41, 184, 219],
        Color::White => [238, 242, 247],
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Indexed(idx) => xterm_color(idx),
        Color::Reset => {
            if background {
                [10, 14, 20]
            } else {
                [228, 233, 240]
            }
        }
    }
}

fn xterm_color(idx: u8) -> [u8; 3] {
    const BASIC: [[u8; 3]; 16] = [
        [0, 0, 0],
        [128, 0, 0],
        [0, 128, 0],
        [128, 128, 0],
        [0, 0, 128],
        [128, 0, 128],
        [0, 128, 128],
        [192, 192, 192],
        [128, 128, 128],
        [255, 0, 0],
        [0, 255, 0],
        [255, 255, 0],
        [0, 0, 255],
        [255, 0, 255],
        [0, 255, 255],
        [255, 255, 255],
    ];
    if idx < 16 {
        return BASIC[usize::from(idx)];
    }
    if idx >= 232 {
        let level = 8 + (idx - 232) * 10;
        return [level, level, level];
    }
    let n = idx - 16;
    let r = n / 36;
    let g = (n % 36) / 6;
    let b = n % 6;
    [xterm_level(r), xterm_level(g), xterm_level(b)]
}

fn xterm_level(value: u8) -> u8 {
    if value == 0 { 0 } else { 55 + value * 40 }
}

fn fill_rect(
    pixels: &mut [u8],
    image_w: usize,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    color: [u8; 3],
) {
    for y in y0..(y0 + h) {
        for x in x0..(x0 + w) {
            put_pixel(pixels, image_w, x, y, color);
        }
    }
}

fn draw_glyph(pixels: &mut [u8], image_w: usize, x0: usize, y0: usize, ch: char, color: [u8; 3]) {
    for (row, bits) in glyph_5x7(ch).iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                put_pixel(pixels, image_w, x0 + col, y0 + row, color);
            }
        }
    }
}

fn put_pixel(pixels: &mut [u8], image_w: usize, x: usize, y: usize, color: [u8; 3]) {
    let offset = (y * image_w + x) * 3;
    if offset + 2 < pixels.len() {
        pixels[offset] = color[0];
        pixels[offset + 1] = color[1];
        pixels[offset + 2] = color[2];
    }
}

fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10011, 0b10101, 0b10101, 0b10101, 0b11001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        ':' => [0, 0b00100, 0b00100, 0, 0b00100, 0b00100, 0],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        '\\' => [
            0b10000, 0b01000, 0b01000, 0b00100, 0b00010, 0b00010, 0b00001,
        ],
        '[' => [
            0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
        ],
        ']' => [
            0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
        ],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100, 0],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        '=' => [0, 0, 0b11111, 0, 0b11111, 0, 0],
        '|' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        '*' => [0, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0],
        '#' => [0b01010, 0b11111, 0b01010, 0b01010, 0b11111, 0b01010, 0],
        '%' => [0b11001, 0b11010, 0b00100, 0b01000, 0b10110, 0b00110, 0],
        _ => [
            0b11111, 0b10001, 0b10101, 0b10001, 0b10101, 0b10001, 0b11111,
        ],
    }
}

fn write_png_rgb(output: &Path, width: u32, height: u32, rgb: &[u8]) -> Result<()> {
    let row_len = width as usize * 3;
    let mut scanlines = Vec::with_capacity((row_len + 1) * height as usize);
    for row in rgb.chunks(row_len) {
        scanlines.push(0);
        scanlines.extend_from_slice(row);
    }

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    write_png_chunk(&mut png, b"IHDR", &ihdr);
    write_png_chunk(&mut png, b"IDAT", &zlib_store(&scanlines));
    write_png_chunk(&mut png, b"IEND", &[]);

    std::fs::write(output, png)?;
    Ok(())
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut offset = 0usize;
    while offset < data.len() {
        let remaining = data.len() - offset;
        let block_len = remaining.min(65_535);
        let is_final = offset + block_len == data.len();
        out.push(u8::from(is_final));
        let len = block_len as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + block_len]);
        offset += block_len;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn write_png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_data = Vec::with_capacity(kind.len() + data.len());
    crc_data.extend_from_slice(kind);
    crc_data.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in data {
        a = (a + u32::from(*byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

async fn hydrate_smoke_state(app: &mut App) {
    app.refresh_now().await;
    if let Ok(report) = crate::release::build_release_status_report(
        &app.db,
        crate::release::ReleaseStatusQuery {
            project_id: Some(crate::release::DEFAULT_RELEASE_PROJECT_ID),
            ref_name: Some("main".to_string()),
            sha: None,
            limit: 1,
        },
    )
    .await
    {
        app.state.release_status_generated_at = Some(report.generated_at);
        app.state.release_status = report.latest;
    }
}

async fn cache_maintenance_loop(docker_ctl: crate::docker::DockerCtl) {
    static GC_RUNNING: AtomicBool = AtomicBool::new(false);

    struct GcGuard;
    impl Drop for GcGuard {
        fn drop(&mut self) {
            GC_RUNNING.store(false, Ordering::SeqCst);
        }
    }

    async fn run_pass(docker_ctl: &crate::docker::DockerCtl) {
        let usage_pct = match crate::cache::df_usage("/").await {
            Ok(fs) => fs.used_percent,
            Err(e) => {
                error!(error = %e, "failed to check disk usage");
                return;
            }
        };

        let is_critical = usage_pct > 85.0;
        let is_emergency = usage_pct > 93.0;
        let is_warning = usage_pct > 75.0;
        let manager = crate::cache::CacheManager;

        if !is_warning {
            if let Err(e) = manager.gc_disk_cache().await {
                error!(error = %e, "background cache GC failed");
            }
            return;
        }

        if GC_RUNNING.swap(true, Ordering::SeqCst) {
            warn!("background cache GC already in progress, skipping this cycle");
            return;
        }

        let _guard = GcGuard;

        if let Err(e) = crate::reclaim::run_auto_gc(docker_ctl, is_critical, is_emergency).await {
            error!(error = %e, "background auto_gc failed");
        }

        if let Err(e) = manager
            .gc_disk_cache_with_pressure(is_warning, is_critical, is_emergency)
            .await
        {
            error!(error = %e, "background cache GC failed");
        }
    }

    // Run once immediately so the TUI host can recover even when the daemon is absent.
    run_pass(&docker_ctl).await;

    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        run_pass(&docker_ctl).await;
    }
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode, KeyModifiers};
    use std::time::Duration;

    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if crossterm::event::poll(tick_rate)?
            && let Event::Key(key) = event::read()?
        {
            // ---- Command palette input routing ---------------------------
            if app.command_palette_open {
                match key.code {
                    KeyCode::Esc => {
                        app.command_palette_open = false;
                        app.command_palette_query.clear();
                        app.selected_palette_index = 0;
                    }
                    KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE => {
                        app.command_palette_query.push(c);
                        app.selected_palette_index = 0;
                    }
                    KeyCode::Backspace => {
                        app.command_palette_query.pop();
                        app.selected_palette_index = 0;
                    }
                    KeyCode::Up => {
                        app.selected_palette_index = app.selected_palette_index.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        let count = action_registry::filtered(&app.command_palette_query).count();
                        if count > 0 {
                            app.selected_palette_index =
                                (app.selected_palette_index + 1).min(count - 1);
                        }
                    }
                    KeyCode::Enter => {
                        execute_palette_action(app);
                        app.command_palette_open = false;
                        app.command_palette_query.clear();
                        app.selected_palette_index = 0;
                    }
                    _ => {}
                }
                app.tick().await;
                continue;
            }

            // ---- Normal key handlers -------------------------------------
            match key.code {
                // Open palette
                KeyCode::Char('k') if key.modifiers == KeyModifiers::CONTROL => {
                    app.command_palette_open = true;
                    app.command_palette_query.clear();
                    app.selected_palette_index = 0;
                }
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Esc => {
                    if app.maximize_logs {
                        app.close_log_view();
                    } else {
                        return Ok(());
                    }
                }
                KeyCode::Enter if app.active_pane == app::ActivePane::Jobs => {
                    app.open_selected_job_log();
                }
                KeyCode::Enter if app.active_tab == crate::tui::app::ActiveTab::Tests => {
                    app.fetch_selected_test_history().await;
                }
                // Toggle audit ledger in Evidence tab
                KeyCode::Char('a') if app.active_tab == crate::tui::app::ActiveTab::Evidence => {
                    app.evidence_view_mode = match app.evidence_view_mode {
                        app::EvidenceViewMode::Capsules => app::EvidenceViewMode::AuditLedger,
                        app::EvidenceViewMode::AuditLedger => app::EvidenceViewMode::Capsules,
                    };
                }
                KeyCode::Char('p') => app.toggle_pool_paused().await?,
                KeyCode::Char('d') | KeyCode::Delete => app.delete_selected_item().await?,
                KeyCode::Char('r') => app.retry_selected_job().await?,
                KeyCode::Tab => app.cycle_tab_next(),
                KeyCode::Right => app.cycle_pane_next(),
                KeyCode::Left => app.cycle_pane_prev(),
                KeyCode::Up => {
                    if app.maximize_logs {
                        app.scroll_logs_up(1);
                    } else {
                        app.up();
                    }
                }
                KeyCode::Down => {
                    if app.maximize_logs {
                        app.scroll_logs_down(1);
                    } else {
                        app.down();
                    }
                }
                KeyCode::PageUp if app.maximize_logs => {
                    app.scroll_logs_up(20);
                }
                KeyCode::PageDown | KeyCode::Char(' ') if app.maximize_logs => {
                    app.scroll_logs_down(20);
                }
                KeyCode::Char('G') | KeyCode::End if app.maximize_logs => {
                    app.follow_logs();
                }
                KeyCode::Home if app.maximize_logs => {
                    app.jump_logs_top();
                }
                KeyCode::Char('v') | KeyCode::Char('t')
                    if app.active_tab == crate::tui::app::ActiveTab::Tests =>
                {
                    app.toggle_test_view_mode();
                }
                KeyCode::Char('1') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(1) {
                        app.active_tab = t;
                    }
                }
                KeyCode::Char('2') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(2) {
                        app.active_tab = t;
                    }
                }
                KeyCode::Char('3') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(3) {
                        app.active_tab = t;
                    }
                }
                KeyCode::Char('4') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(4) {
                        app.active_tab = t;
                    }
                }
                KeyCode::Char('5') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(5) {
                        app.active_tab = t;
                    }
                }
                KeyCode::Char('6') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(6) {
                        app.active_tab = t;
                    }
                }
                KeyCode::Char('7') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(7) {
                        app.active_tab = t;
                    }
                }
                KeyCode::Char('8') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(8) {
                        app.active_tab = t;
                    }
                }
                KeyCode::Char('9') => {
                    if let Some(t) = crate::tui::app::ActiveTab::from_number(9) {
                        app.active_tab = t;
                    }
                }
                _ => {}
            }
        }

        app.tick().await;
    }
}

/// Execute the currently selected command palette action.
fn execute_palette_action(app: &mut App) {
    let matches: Vec<&action_registry::ActionEntry> =
        action_registry::filtered(&app.command_palette_query).collect();
    let Some(entry) = matches.get(app.selected_palette_index) else {
        return;
    };
    match entry.id {
        "tab_mission" => app.active_tab = app::ActiveTab::Mission,
        "tab_release" => app.active_tab = app::ActiveTab::Release,
        "tab_jobs" => app.active_tab = app::ActiveTab::Jobs,
        "tab_agents" => app.active_tab = app::ActiveTab::Agents,
        "tab_tests" => app.active_tab = app::ActiveTab::Tests,
        "tab_pools" => app.active_tab = app::ActiveTab::Pools,
        "tab_cache" => app.active_tab = app::ActiveTab::Cache,
        "tab_evidence" => app.active_tab = app::ActiveTab::Evidence,
        "tab_secrets" => app.active_tab = app::ActiveTab::Secrets,
        "toggle_audit_ledger" => {
            app.evidence_view_mode = match app.evidence_view_mode {
                app::EvidenceViewMode::Capsules => app::EvidenceViewMode::AuditLedger,
                app::EvidenceViewMode::AuditLedger => app::EvidenceViewMode::Capsules,
            };
        }
        _ => {} // Other actions: user uses key binding or capability API
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    async fn test_app() -> Result<App> {
        let db = crate::state::Db::open_memory().await?;
        let docker = crate::docker::DockerCtl::connect()?;
        let gitlab = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:9", None);
        Ok(App::new(db, docker, gitlab))
    }

    fn draw_once(app: &mut App) -> Result<()> {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|f| ui::draw(f, app))?;
        Ok(())
    }

    fn job(job_id: i64, status: &str) -> crate::state::JobEvent {
        crate::state::JobEvent {
            job_id,
            project_id: 2,
            pipeline_id: Some(10),
            status: status.into(),
            job_name: Some(format!("test-job-{job_id}")),
            pool_name: Some("default".into()),
            system_id: None,
            queued_duration: None,
            received_at: "2026-04-23T19:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn renders_all_primary_tabs_with_empty_state() -> Result<()> {
        let mut app = test_app().await?;
        for tab in [
            app::ActiveTab::Mission,
            app::ActiveTab::Release,
            app::ActiveTab::Jobs,
            app::ActiveTab::Agents,
            app::ActiveTab::Tests,
            app::ActiveTab::Pools,
            app::ActiveTab::Cache,
            app::ActiveTab::Evidence,
            app::ActiveTab::Secrets,
        ] {
            app.active_tab = tab;
            draw_once(&mut app)?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn renders_maximized_logs_empty_state() -> Result<()> {
        let mut app = test_app().await?;
        app.active_pane = app::ActivePane::Jobs;
        app.maximize_logs = true;
        draw_once(&mut app)?;
        Ok(())
    }

    #[tokio::test]
    async fn renders_flow_with_jobs_list_and_live_log() -> Result<()> {
        let mut app = test_app().await?;
        app.state.recent_jobs = vec![job(1, "running"), job(2, "pending")];
        app.state.live_log.text = "cargo test\nwarning: slow test\nerror: sample failure".into();
        app.state.flow.active_pipelines = vec![crate::tui::flow::PipelineFlow {
            pipeline_id: 10,
            project_id: 2,
            ref_name: "main".into(),
            sha: Some("abc123".into()),
            status: "running".into(),
            graph: crate::tui::flow::build_graph(10, app.state.recent_jobs.clone()),
            current_blocker: Some(1),
            critical_path: vec![],
            eta: None,
            progress_pct: 50,
        }];

        app.active_tab = app::ActiveTab::Jobs;
        app.active_pane = app::ActivePane::Jobs;
        draw_once(&mut app)?;
        Ok(())
    }

    #[tokio::test]
    async fn navigation_cycles_tabs_and_panes() -> Result<()> {
        let mut app = test_app().await?;
        assert_eq!(app.active_tab, app::ActiveTab::Mission);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Release);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Jobs);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Agents);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Tests);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Pools);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Cache);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Evidence);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Secrets);
        app.cycle_tab_next();
        assert_eq!(app.active_tab, app::ActiveTab::Mission);

        assert_eq!(app.active_pane, app::ActivePane::Jobs);
        app.cycle_pane_next();
        assert_eq!(app.active_pane, app::ActivePane::Jobs);
        Ok(())
    }
}
