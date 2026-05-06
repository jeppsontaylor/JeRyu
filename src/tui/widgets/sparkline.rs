//! Owner: Interactive TUI subsystem — terminal sparkline widget
//! Proof: `cargo nextest run -p jeryu -- tui::widgets::sparkline`
//! Invariants: Sparklines are pure rendering; values are not modified.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

const BLOCKS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render a sparkline string from a slice of values.
/// Returns a styled `Span` using the provided color.
pub fn spark(values: &[f64], width: usize, color: Color) -> Span<'static> {
    Span::styled(spark_str(values, width), Style::default().fg(color))
}

/// Render a sparkline string from integer values.
pub fn spark_i64(values: &[i64], width: usize, color: Color) -> Span<'static> {
    let floats: Vec<f64> = values.iter().map(|v| *v as f64).collect();
    spark(&floats, width, color)
}

/// Generate the raw sparkline string.
pub fn spark_str(values: &[f64], width: usize) -> String {
    if values.is_empty() || width == 0 {
        return "n/a".to_string();
    }

    let take = width.min(values.len());
    let slice = &values[values.len() - take..];
    let min = slice.iter().copied().fold(f64::INFINITY, f64::min);
    let max = slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    if (max - min).abs() < f64::EPSILON {
        return BLOCKS[3].to_string().repeat(take);
    }

    slice
        .iter()
        .map(|v| {
            let normalized = ((v - min) / (max - min) * 7.0).round() as usize;
            BLOCKS[normalized.min(7)]
        })
        .collect()
}

/// Create a labeled sparkline Line: "label: ▁▂▃▅▇█ value"
pub fn labeled_spark(
    label: &str,
    values: &[f64],
    width: usize,
    current: &str,
    label_color: Color,
    spark_color: Color,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {label:<12}"),
            Style::default().fg(label_color),
        ),
        spark(values, width, spark_color),
        Span::styled(
            format!(" {current}"),
            Style::default().fg(spark_color),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_empty() {
        assert_eq!(spark_str(&[], 10), "n/a");
    }

    #[test]
    fn sparkline_constant() {
        let s = spark_str(&[5.0, 5.0, 5.0], 3);
        assert_eq!(s.chars().count(), 3);
        // All same value = all mid-level blocks
        assert!(s.chars().all(|c| c == '▄'));
    }

    #[test]
    fn sparkline_ascending() {
        let s = spark_str(&[0.0, 50.0, 100.0], 3);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[0], '▁'); // min
        assert_eq!(chars[2], '█'); // max
    }

    #[test]
    fn sparkline_respects_width() {
        let vals: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let s = spark_str(&vals, 5);
        assert_eq!(s.chars().count(), 5);
    }
}
