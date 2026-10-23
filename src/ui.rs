use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Chart, Dataset, GraphType};

use crate::app::{App, Host, Sample};

/// Approximate width taken by the y-axis labels and borders.
const CHART_OVERHEAD: u16 = 10;

pub fn render(frame: &mut Frame, app: &App) {
    let panes = grid(frame.area(), app.hosts.len());
    for (host, area) in app.hosts.iter().zip(panes) {
        render_host(frame, host, area);
    }
}

/// Splits `area` into `n` panes laid out as a near-square grid. Rows have equal
/// height; panes within a row have equal width (a short last row gets wider panes).
fn grid(area: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = n.div_ceil(cols);
    let row_areas = Layout::vertical(vec![Constraint::Ratio(1, rows as u32); rows]).split(area);

    let mut panes = Vec::with_capacity(n);
    for (r, row) in row_areas.iter().enumerate() {
        let in_row = cols.min(n - r * cols);
        let cells = Layout::horizontal(vec![Constraint::Ratio(1, in_row as u32); in_row]).split(*row);
        panes.extend(cells.iter().copied());
    }
    panes
}

fn render_host(frame: &mut Frame, host: &Host, area: Rect) {
    let block = Block::bordered().title(title(host));

    // Braille gives two horizontal dots per cell.
    let window = (area.width.saturating_sub(CHART_OVERHEAD) as usize * 2).max(2);
    let skip = host.samples.len().saturating_sub(window);
    let visible: Vec<Sample> = host.samples.iter().skip(skip).copied().collect();

    let x_lo = visible.first().map_or(0, Sample::seq) as f64;
    let x_hi = x_lo + window as f64 - 1.0;
    let y_max = visible
        .iter()
        .filter_map(|s| match s {
            Sample::Rtt { ms, .. } => Some(*ms),
            Sample::Lost { .. } => None,
        })
        .fold(0.0, f64::max)
        .max(1.0)
        * 1.2;

    // Contiguous runs of replies, broken at each loss, so lines never bridge a gap.
    let mut runs: Vec<Vec<(f64, f64)>> = vec![Vec::new()];
    let mut losses: Vec<[(f64, f64); 2]> = Vec::new();
    for s in &visible {
        match *s {
            Sample::Rtt { seq, ms } => runs.last_mut().unwrap().push((seq as f64, ms)),
            Sample::Lost { seq } => {
                losses.push([(seq as f64, 0.0), (seq as f64, y_max)]);
                runs.push(Vec::new());
            }
        }
    }

    let lost_style = Style::new().fg(Color::Red);
    let rtt_style = Style::new().fg(Color::Green);
    let datasets = losses
        .iter()
        .map(|pts| Dataset::default().marker(Marker::Braille).graph_type(GraphType::Line).style(lost_style).data(pts))
        .chain(runs.iter().filter(|r| !r.is_empty()).map(|run| {
            // A lone point has no segment to draw, so show it as a dot.
            let kind = if run.len() == 1 { GraphType::Scatter } else { GraphType::Line };
            Dataset::default().marker(Marker::Braille).graph_type(kind).style(rtt_style).data(run)
        }))
        .collect();

    let y_labels = [0.0, y_max / 2.0, y_max].map(|v| Span::raw(format!("{v:.1}")));
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(Axis::default().bounds([x_lo, x_hi]))
        .y_axis(Axis::default().title("ms").bounds([0.0, y_max]).labels(y_labels));
    frame.render_widget(chart, area);
}

fn title(host: &Host) -> Line<'static> {
    let mut spans = vec![Span::raw(format!(" {} ", host.addr)).bold()];
    if let Some(status) = &host.status {
        spans.push(Span::raw(format!(" {status} ")).red().bold());
        return Line::from(spans);
    }
    let fmt = |v: Option<f64>| v.map_or("-".to_string(), |v| format!("{v:.1}"));
    let min = (host.received > 0).then_some(host.min);
    let max = (host.received > 0).then_some(host.max);
    spans.push(Span::raw(format!(
        " last {}ms  min/avg/max {}/{}/{}ms ",
        fmt(host.last),
        fmt(min),
        fmt(host.avg()),
        fmt(max),
    )));
    let loss = Span::raw(format!(" loss {:.1}% ", host.loss_pct()));
    spans.push(if host.lost > 0 { loss.red() } else { loss });
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(n: usize) -> Vec<usize> {
        let panes = grid(Rect::new(0, 0, 120, 60), n);
        assert_eq!(panes.len(), n);
        let mut rows: Vec<usize> = Vec::new();
        let mut last_y = None;
        for p in panes {
            if last_y != Some(p.y) {
                rows.push(0);
                last_y = Some(p.y);
            }
            *rows.last_mut().unwrap() += 1;
        }
        rows
    }

    #[test]
    fn grid_shapes() {
        assert_eq!(shape(1), [1]);
        assert_eq!(shape(2), [2]);
        assert_eq!(shape(3), [2, 1]);
        assert_eq!(shape(4), [2, 2]);
        assert_eq!(shape(5), [3, 2]);
        assert_eq!(shape(6), [3, 3]);
        assert_eq!(shape(7), [3, 3, 1]);
    }
}
