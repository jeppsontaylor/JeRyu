use anyhow::Result;
use image::{Rgb, RgbImage};
use std::path::Path;

pub(crate) fn write_buffer_png(buffer: &ratatui::buffer::Buffer, output: &Path) -> Result<()> {
    let cell_w = 8u32;
    let cell_h = 12u32;
    let width_cells = u32::from(buffer.area.width);
    let height_cells = u32::from(buffer.area.height);
    let image_w = width_cells * cell_w;
    let image_h = height_cells * cell_h;
    let mut image = RgbImage::new(image_w, image_h);

    for y in 0..height_cells {
        for x in 0..width_cells {
            let cell = &buffer.content[(y * width_cells + x) as usize];
            fill_rect(
                &mut image,
                x * cell_w,
                y * cell_h,
                cell_w,
                cell_h,
                color_to_rgb(cell.bg, true),
            );

            let symbol = cell.symbol();
            if !symbol.trim().is_empty() {
                fill_rect(
                    &mut image,
                    x * cell_w + 2,
                    y * cell_h + 3,
                    4,
                    6,
                    color_to_rgb(cell.fg, false),
                );
            }
        }
    }

    image.save(output)?;
    Ok(())
}

fn fill_rect(image: &mut RgbImage, x0: u32, y0: u32, w: u32, h: u32, color: [u8; 3]) {
    let x1 = (x0 + w).min(image.width());
    let y1 = (y0 + h).min(image.height());
    for y in y0..y1 {
        for x in x0..x1 {
            image.put_pixel(x, y, Rgb(color));
        }
    }
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
