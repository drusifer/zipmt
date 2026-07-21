use super::{FocusedWidget, IoChartMode, ModeState, TuiState};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Borders, Chart, Clear, Dataset, Gauge, GraphType, Paragraph,
};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) struct RenderPalette {
    pub orange: Style,
    pub purple: Style,
    pub cyan: Style,
    pub yellow: Style,
}

impl Default for RenderPalette {
    fn default() -> Self {
        Self {
            orange: Style::default().fg(Color::Indexed(208)),
            purple: Style::default().fg(Color::Indexed(147)),
            cyan: Style::default().fg(Color::Indexed(117)),
            yellow: Style::default().fg(Color::Indexed(220)),
        }
    }
}

pub(super) struct DashboardLayout {
    pub header: Rect,
    pub body: Rect,
    pub logs: Rect,
    pub footer: Rect,
}

impl DashboardLayout {
    pub fn new(area: Rect) -> Self {
        let extra_rows = area.height.saturating_sub(22);
        let body_height = 11 + extra_rows / 2;
        let footer_height = super::footer_height_for_rows(area.height);
        let logs_height = area.height.saturating_sub(1 + body_height + footer_height);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(body_height),
                Constraint::Length(logs_height),
                Constraint::Length(footer_height),
            ])
            .split(area);
        Self {
            header: chunks[0],
            body: chunks[1],
            logs: chunks[2],
            footer: chunks[3],
        }
    }
}

pub(super) fn status(state: &TuiState) -> (&'static str, Color) {
    if state.is_paused {
        ("PAUSED", Color::Red)
    } else if state.is_complete {
        ("COMPLETE", Color::Green)
    } else {
        ("RUNNING", Color::Indexed(117))
    }
}

pub(super) fn render_small_terminal(frame: &mut ratatui::Frame<'_>, area: Rect) -> bool {
    if area.width >= 80 && area.height >= 22 {
        return false;
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Terminal size too small.",
                Style::default().fg(Color::Red),
            )),
            Line::from(Span::styled(
                format!("Current: {}x{}", area.width, area.height),
                Style::default().fg(Color::Red),
            )),
            Line::from(Span::styled(
                "Please resize to at least 80x22.",
                Style::default().fg(Color::Yellow),
            )),
        ]),
        area,
    );
    true
}

pub(super) fn render_header(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    status: &str,
    status_color: Color,
    orange: Style,
) {
    let title = "ZIPMT PIPELINE CONTROLLER ";
    let status = format!(" [STATUS: {status}]");
    let rule_width =
        (area.width as usize).saturating_sub(title.chars().count() + status.chars().count());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("═".repeat(rule_width), orange),
            Span::styled(
                status,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        area,
    );
}

pub(super) fn render_logs(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    palette: RenderPalette,
) {
    let pool_size = if state.mode == ModeState::Split {
        state.stripes.len()
    } else {
        state.workers.len()
    };
    let chunk_size = match state.mode {
        ModeState::Split => {
            let size = if state
                .stripes
                .first()
                .is_some_and(|stripe| stripe.total_bytes > 0)
            {
                state.stripes[0].total_bytes
            } else if !state.stripes.is_empty() {
                state.total_input_size / state.stripes.len()
            } else {
                0
            };
            format!("{:.1}M", size as f64 / (1024.0 * 1024.0))
        }
        ModeState::Stream => format!("{:.1}M", state.chunk_size as f64 / (1024.0 * 1024.0)),
    };
    let average = state.get_avg_chunk_time_ms();
    let average = if average > 0.0 {
        format!("(avg:{average:.1}ms)")
    } else {
        "(avg:--)".to_string()
    };
    let title = format!(
        " Log Messages (▲/▼, wheel, Home/End) ── Knobs: P:{}/{} C:{} Q:{} L:{} {} ",
        state.active_workers.min(pool_size),
        state.max_workers.max(pool_size),
        chunk_size,
        state.queue_capacity,
        state.level,
        average
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(palette.orange)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    let logs = crate::get_log_buffer()
        .lock()
        .map_or_else(|_| Vec::new(), |buffer| buffer.clone());
    let height = area.height.saturating_sub(2) as usize;
    let lines_back = crate::LOG_SCROLL_OFFSET
        .load(Ordering::Relaxed)
        .min(logs.len().saturating_sub(height));
    let offset = super::log_window_start(logs.len(), height, lines_back);
    crate::LOG_SCROLL_OFFSET.store(lines_back, Ordering::Relaxed);
    let width = area.width.saturating_sub(4) as usize;
    let lines = (0..height)
        .map(|row| {
            logs.get(offset + row).map_or_else(
                || Line::from(""),
                |line| {
                    let text = if line.len() > width {
                        format!("{}...", &line[..width.saturating_sub(3)])
                    } else {
                        line.clone()
                    };
                    Line::from(Span::styled(text, palette.cyan))
                },
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

pub(super) fn render_io_panel(
    frame: &mut ratatui::Frame<'_>,
    chart_area: Rect,
    process_area: Rect,
    state: &TuiState,
    palette: RenderPalette,
) {
    let title = format!(
        " I/O Flow [I] {} ",
        match state.io_chart_mode {
            IoChartMode::Rate => "RATE MA10s",
            IoChartMode::Cumulative => "CUMULATIVE",
        }
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(palette.orange)
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    let data = super::prepare_io_chart(
        &state.io_history,
        state.io_chart_mode,
        chart_area.width.saturating_sub(10) as usize,
    );
    let mut datasets = vec![
        Dataset::default()
            .marker(Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Style::default().fg(Color::Indexed(238)))
            .data(&data.guides[0]),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Indexed(240)))
            .data(&data.guides[1]),
        Dataset::default()
            .marker(Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Style::default().fg(Color::Indexed(238)))
            .data(&data.guides[2]),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&data.input),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Yellow))
            .data(&data.output),
    ];
    if state.io_chart_mode == IoChartMode::Rate {
        let average_style = Style::default().fg(Color::Indexed(213));
        datasets.extend([
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(average_style)
                .data(&data.input_average),
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(average_style)
                .data(&data.output_average),
        ]);
    }
    let per_second = state.io_chart_mode == IoChartMode::Rate;
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(Axis::default().bounds([0.0, data.x_max]))
        .y_axis(
            Axis::default()
                .style(palette.purple)
                .bounds([-data.scale, data.scale])
                .labels(vec![
                    Span::styled(
                        format!("OUT {}", super::format_bytes(data.latest.1, per_second)),
                        palette.yellow,
                    ),
                    Span::styled("0", palette.purple),
                    Span::styled(
                        format!("IN {}", super::format_bytes(data.latest.0, per_second)),
                        palette.cyan,
                    ),
                ]),
        )
        .legend_position(None);
    frame.render_widget(chart, chart_area);

    let process_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(palette.orange)
        .title(Span::styled(" Process ", palette.yellow));
    let cpu = state
        .process_cpu_percent
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "--".to_string());
    let memory = state
        .process_memory_bytes
        .map(|value| super::format_bytes(value as f64, false))
        .unwrap_or_else(|| "--".to_string());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("CPU ", palette.purple),
            Span::styled(cpu, palette.cyan),
            Span::styled("  RSS ", palette.purple),
            Span::styled(memory, palette.cyan),
        ]))
        .block(process_block),
        process_area,
    );
}

fn control_block(title: &str, border: Style, title_style: Style) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Span::styled(title.to_string(), title_style))
}

fn knob_lines(
    height: usize,
    top: &str,
    value: &str,
    bottom: &str,
    fraction: f64,
    style: Style,
) -> Vec<Line<'static>> {
    super::format_knob_rows(height, top, value, bottom, fraction)
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style)))
        .collect()
}

fn render_level_control(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    palette: RenderPalette,
) {
    let focused = state.focused_widget == FocusedWidget::CompressionLevelSlider;
    let title = if state.mode == ModeState::Split {
        " Level FIXED "
    } else if focused {
        "[ Level ]"
    } else {
        " Level "
    };
    let block = control_block(
        title,
        if focused {
            palette.yellow
        } else {
            palette.purple
        },
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    let height = area.height.saturating_sub(2) as usize;
    let lines = if state.mode == ModeState::Split {
        vec![
            "".to_string(),
            "ENCODER".to_string(),
            format!("LEVEL {}", state.level),
            "FIXED".to_string(),
        ]
        .into_iter()
        .map(|line| Line::from(Span::styled(line, palette.cyan)))
        .collect()
    } else {
        knob_lines(
            height,
            "9 MAX",
            &state.level.to_string(),
            "1 MIN",
            state.level.saturating_sub(1) as f64 / 8.0,
            palette.cyan,
        )
    };
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_throttle_control(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    palette: RenderPalette,
) {
    let focused = state.focused_widget == FocusedWidget::ThrottleDelaySlider;
    let block = control_block(
        if focused {
            "[ Throttle ]"
        } else {
            " Throttle "
        },
        if focused {
            palette.yellow
        } else {
            palette.purple
        },
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    let lines = knob_lines(
        area.height.saturating_sub(2) as usize,
        "500 MAX",
        &state.throttle_delay_ms.to_string(),
        "0 MIN",
        state.throttle_delay_ms as f64 / 500.0,
        palette.cyan,
    );
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_chunk_control(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    palette: RenderPalette,
) {
    let focused = state.focused_widget == FocusedWidget::ChunkSizeSlider;
    let block = control_block(
        if focused { "[ Chunk ]" } else { " Chunk " },
        if focused {
            palette.yellow
        } else {
            palette.purple
        },
        palette.yellow,
    );
    let kib = state.chunk_size / 1024;
    let lines = knob_lines(
        area.height.saturating_sub(2) as usize,
        "8192K",
        &format!("{kib}K"),
        "64K",
        (kib as f64 / 64.0).log2() / 7.0,
        palette.cyan,
    );
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_worker_control(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    palette: RenderPalette,
) {
    let focused = state.focused_widget == FocusedWidget::WorkerCountSlider;
    let title = if state.mode == ModeState::Split {
        " Pool FIXED "
    } else if focused {
        "[ Workers ]"
    } else {
        " Workers "
    };
    let block = control_block(
        title,
        if focused {
            palette.yellow
        } else {
            palette.purple
        },
        palette.yellow,
    );
    let fraction = if state.max_workers > 1 {
        state.active_workers.saturating_sub(1) as f64 / (state.max_workers - 1) as f64
    } else {
        1.0
    };
    let lines = if state.mode == ModeState::Split {
        vec![
            "".to_string(),
            "POOL".to_string(),
            state.max_workers.to_string(),
            "FIXED".to_string(),
        ]
        .into_iter()
        .map(|line| Line::from(Span::styled(line, palette.cyan)))
        .collect()
    } else {
        knob_lines(
            area.height.saturating_sub(2) as usize,
            &format!("{} MAX", state.max_workers),
            &format!("{}/{}", state.active_workers, state.max_workers),
            "1 MIN",
            fraction,
            palette.cyan,
        )
    };
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

pub(super) fn render_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    palette: RenderPalette,
) {
    let areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(28),
            Constraint::Length(13),
            Constraint::Length(13),
            Constraint::Length(13),
            Constraint::Length(13),
        ])
        .split(area);
    let guides = if state.is_complete {
        vec![
            Line::from(Span::styled("[Enter/Q/Esc]Close", palette.yellow)),
            Line::from(Span::styled("[I]RATE/CUMULATIVE", palette.purple)),
            Line::from(Span::styled("[PgUp/PgDn]Sectors", palette.purple)),
            Line::from(Span::styled("Final statistics frozen", palette.purple)),
        ]
    } else if state.mode == ModeState::Split {
        vec![
            Line::from(Span::styled("[P]Pause [-/+]Throttle", palette.purple)),
            Line::from(Span::styled("[PgUp/PgDn]Sectors [Q]Exit", palette.purple)),
            Line::from(Span::styled("[Tab]Throttle/Chunk [↑/↓]", palette.purple)),
            Line::from(Span::styled("[I]I/O mode Level/Pool fixed", palette.purple)),
        ]
    } else {
        vec![
            Line::from(Span::styled("[P]Pause [-/+]Throttle", palette.purple)),
            Line::from(Span::styled("[[]/[]]Level [Q]Exit", palette.purple)),
            Line::from(Span::styled("[Tab]Focus [↑/↓]Adjust", palette.purple)),
            Line::from(Span::styled("[I]I/O mode Click/Drag", palette.purple)),
        ]
    };
    let block = control_block(
        " Pipeline Controls ",
        palette.orange,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(Paragraph::new(guides).block(block), areas[0]);
    render_level_control(frame, areas[1], state, palette);
    render_throttle_control(frame, areas[2], state, palette);
    render_chunk_control(frame, areas[3], state, palette);
    render_worker_control(frame, areas[4], state, palette);
}

fn work_panel_block(title: String, palette: RenderPalette) -> Block<'static> {
    control_block(
        &title,
        palette.orange,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
}

fn split_title(state: &TuiState, start: usize, capacity: usize) -> String {
    if state.stripes.is_empty() {
        return " Slices -- [Pg↕] ".to_string();
    }
    let end = (start + capacity).min(state.stripes.len());
    format!(
        " Slices S{:02}-S{:02}/{} +{} [Pg↕] ",
        start + 1,
        end,
        state.stripes.len(),
        state.stripes.len().saturating_sub(end)
    )
}

pub(super) fn render_split_panel(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    elapsed: f64,
    palette: RenderPalette,
) {
    use crate::pipeline::SplitStage;

    let capacity = area.height.saturating_sub(4).max(2) as usize / 2;
    let start = state
        .split_sector_offset
        .min(state.stripes.len().saturating_sub(1));
    let aggregate = state.split_aggregate();
    let ratio = aggregate
        .ratio()
        .map(|value| format!("{value:.2}x"))
        .unwrap_or_else(|| "--".to_string());
    let timing = if state.is_complete {
        format!("TIME {elapsed:.1}s")
    } else {
        let eta = super::composite_eta(aggregate, Duration::from_secs_f64(elapsed))
            .map(|duration| format!("{:.0}s", duration.as_secs_f64()))
            .unwrap_or_else(|| "--".to_string());
        format!("ETA {eta}")
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(
                "TOTAL {:3.0}% {}/{} ",
                aggregate.percent(),
                aggregate.completed_sectors,
                state.stripes.len()
            ),
            palette.yellow,
        ),
        Span::styled(format!("RATIO {ratio} {timing}"), palette.cyan),
    ])];
    let output_total = aggregate
        .output_produced
        .saturating_add(state.split_final_bytes_written);
    let output_rate = if elapsed > 0.0 {
        output_total as f64 / elapsed
    } else {
        0.0
    };
    let summary = if state.is_complete {
        format!(
            "IN {} OUT {} IO {} AVG {}→{}",
            super::format_bytes(aggregate.input_processed as f64, false),
            super::format_bytes(aggregate.output_produced as f64, false),
            super::format_bytes(output_total as f64, false),
            super::format_bytes(
                aggregate.input_processed as f64 / elapsed.max(f64::EPSILON),
                true
            ),
            super::format_bytes(output_rate, true)
        )
    } else {
        format!(
            "IN {} / {}  OUT {}  ACTIVE {}",
            super::format_bytes(aggregate.input_processed as f64, false),
            super::format_bytes(aggregate.input_total as f64, false),
            super::format_bytes(aggregate.output_produced as f64, false),
            aggregate.active_sectors
        )
    };
    lines.push(Line::from(Span::styled(summary, palette.purple)));
    for stripe in state.stripes.iter().skip(start).take(capacity) {
        let (average_input, average_output) = stripe.average_rates(Instant::now());
        let total = stripe.total_bytes;
        let processed = stripe.bytes_processed.min(total);
        let percent = (processed * 100).checked_div(total).unwrap_or(0);
        let stage = match stripe.stage {
            SplitStage::Waiting => "WAIT",
            SplitStage::Running => "RUN ",
            SplitStage::Done => "DONE",
        };
        let ratio = if stripe.stage == SplitStage::Done && stripe.bytes_written > 0 {
            format!("{:.1}x", total as f64 / stripe.bytes_written as f64)
        } else {
            "--".to_string()
        };
        let width = area.width.saturating_sub(30).max(3) as usize;
        let filled = (processed * width)
            .checked_div(total)
            .unwrap_or(0)
            .min(width);
        let bar = std::iter::repeat_n('█', filled)
            .chain(std::iter::repeat_n('░', width - filled))
            .collect::<String>();
        lines.push(Line::from(vec![
            Span::styled(format!("S{:02} {stage} ", stripe.id + 1), palette.purple),
            Span::styled(format!("[{bar}] "), palette.cyan),
            Span::styled(format!("{percent:3}% "), palette.yellow),
            Span::styled(
                format!(
                    "{} / {}",
                    super::format_bytes(processed as f64, false),
                    super::format_bytes(total as f64, false)
                ),
                palette.cyan,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    AVG ", palette.purple),
            Span::styled(
                format!(
                    "{}→{}  OUT {}  R {ratio}",
                    super::format_bytes(average_input, true),
                    super::format_bytes(average_output, true),
                    super::format_bytes(stripe.bytes_written as f64, false),
                ),
                palette.cyan,
            ),
        ]));
    }
    while lines.len() < 10 {
        lines.push(Line::from(""));
    }
    let block = work_panel_block(split_title(state, start, capacity), palette);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn stream_title(state: &TuiState, start: usize, capacity: usize) -> String {
    if state.workers.is_empty() {
        return " WORKERS -- [Pg↕] ".to_string();
    }
    let end = (start + capacity).min(state.workers.len());
    format!(
        " WORKERS W{:02}-W{:02}/{} +{} [Pg↕] ",
        start + 1,
        end,
        state.workers.len(),
        state.workers.len().saturating_sub(end)
    )
}

fn render_worker_cards(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    start: usize,
    capacity: usize,
    palette: RenderPalette,
) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            state
                .workers
                .iter()
                .skip(start)
                .take(capacity)
                .map(|_| Constraint::Length(4))
                .collect::<Vec<_>>(),
        )
        .split(area);
    let now = Instant::now();
    for (worker, area) in state
        .workers
        .iter()
        .skip(start)
        .take(capacity)
        .zip(areas.iter().copied())
    {
        let progress = if worker.total_bytes > 0 {
            worker.bytes_processed as f64 / worker.total_bytes as f64
        } else {
            0.0
        };
        let ratio = super::format_worker_ratio(worker.average_ratio());
        let eta = worker
            .eta(now)
            .map(|duration| format!("{:.2}s", duration.as_secs_f64()))
            .unwrap_or_else(|| "--.--s".to_string());
        let status = if worker.completed_at.is_some() && worker.current_chunk.is_none() {
            "DONE"
        } else {
            worker.stage.label()
        };
        let chunk = worker
            .display_chunk
            .or(worker.current_chunk)
            .map(|chunk| format!("C{:03}", chunk + 1))
            .unwrap_or_else(|| "C---".to_string());
        let card = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(palette.purple)
            .title(Span::styled(
                format!(" W{:02} {status:<4} {chunk} ", worker.id + 1),
                palette.yellow,
            ));
        let inner = card.inner(area);
        frame.render_widget(card, area);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(inner);
        frame.render_widget(
            Gauge::default()
                .gauge_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .ratio(progress.clamp(0.0, 1.0))
                .label(format!("{:6.2}%", progress * 100.0)),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("AVG ", palette.purple),
                Span::styled(
                    super::format_rate_fixed(worker.average_input_rate(now)),
                    palette.cyan,
                ),
                Span::styled("  R ", palette.purple),
                Span::styled(format!("{ratio}x"), palette.cyan),
                Span::styled("  ETA ", palette.purple),
                Span::styled(format!("{eta:>7}"), palette.cyan),
            ])),
            rows[1],
        );
    }
}

pub(super) fn render_stream_panel(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    state: &TuiState,
    elapsed: f64,
    palette: RenderPalette,
) {
    let capacity = area.height.saturating_sub(4).max(4) as usize / 4;
    let start = state
        .worker_offset
        .min(state.workers.len().saturating_sub(1));
    let block = work_panel_block(stream_title(state, start, capacity), palette);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .split(inner);
    let input_rate = if elapsed > 0.0 {
        state.bytes_read as f64 / elapsed
    } else {
        0.0
    };
    let output_rate = if elapsed > 0.0 {
        state.bytes_written as f64 / elapsed
    } else {
        0.0
    };
    let ratio = if state.bytes_written > 0 {
        state.bytes_read as f64 / state.bytes_written as f64
    } else {
        1.0
    };
    let header = if state.is_complete {
        Line::from(Span::styled(
            format!(
                "DONE {elapsed:.1}s I{} O{} R{ratio:.2}x",
                super::format_rate_fixed(input_rate),
                super::format_rate_fixed(output_rate)
            ),
            palette.yellow,
        ))
    } else {
        Line::from(vec![
            Span::styled("IN ", palette.purple),
            Span::styled(
                format!("{:.2}M ", state.bytes_read as f64 / (1024.0 * 1024.0)),
                palette.cyan,
            ),
            Span::styled(super::format_bytes(input_rate, true), palette.cyan),
            Span::styled("  OUT ", palette.purple),
            Span::styled(
                format!("{:.2}M ", state.bytes_written as f64 / (1024.0 * 1024.0)),
                palette.cyan,
            ),
            Span::styled(super::format_bytes(output_rate, true), palette.cyan),
            Span::styled("  R ", palette.purple),
            Span::styled(format!("{ratio:.2}x"), palette.yellow),
        ])
    };
    frame.render_widget(Paragraph::new(header), areas[0]);
    render_worker_cards(frame, areas[1], state, start, capacity, palette);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Q ", palette.purple),
            Span::styled(
                super::format_chunk_slots(&state.input_queue, 4),
                palette.cyan,
            ),
            Span::styled(
                format!(" ({}/{})", state.queue_depth, state.queue_capacity),
                palette.yellow,
            ),
            Span::styled("  PEND ", palette.purple),
            Span::styled(
                super::format_chunk_slots(&state.output_buffer, 8),
                palette.cyan,
            ),
            Span::styled(
                format!("  N #{}", state.next_expected_seq + 1),
                palette.yellow,
            ),
        ])),
        areas[2],
    );
}

pub fn draw_tui<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    state: &TuiState,
) -> Result<(), std::io::Error> {
    #[cfg(test)]
    let elapsed = state
        .final_elapsed
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(1.234);
    #[cfg(not(test))]
    let elapsed = state.elapsed().as_secs_f64();
    let palette = RenderPalette::default();
    let (status, status_color) = status(state);
    terminal.draw(|frame| {
        let area = frame.size();
        frame.render_widget(Clear, area);
        if render_small_terminal(frame, area) {
            return;
        }
        let dashboard = DashboardLayout::new(area);
        render_header(
            frame,
            dashboard.header,
            status,
            status_color,
            palette.orange,
        );
        let profile = super::body_layout_profile(state.mode);
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(profile.left_percent),
                Constraint::Percentage(profile.right_percent),
            ])
            .split(dashboard.body);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .split(body[1]);
        match state.mode {
            ModeState::Split => render_split_panel(frame, body[0], state, elapsed, palette),
            ModeState::Stream => render_stream_panel(frame, body[0], state, elapsed, palette),
        }
        render_io_panel(frame, right[0], right[1], state, palette);
        render_logs(frame, dashboard.logs, state, palette);
        render_footer(frame, dashboard.footer, state, palette);
    })?;
    Ok(())
}
