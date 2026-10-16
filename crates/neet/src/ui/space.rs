//! Space: where the disk's used space goes, past the home folder. The
//! volumes `diskutil` lists, the folders on the Data volume neet can read,
//! and what is left as not measured. View only.

use std::path::PathBuf;
use std::time::Instant;

use neet_core::space::{self, Area, Breakdown, Role, Volume};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Padding, Row, Table, TableState};

use super::app::{Action, Context, Screen};
use super::format;
use super::loading::Loading;
use super::tools;
use super::visual::{self, Columns};

/// From this width the selected part shows on the right.
const SIDE_WIDTH: u16 = 100;

/// How wide each bar is
const BAR: u16 = 12;

/// Width of the labels in the boxes on the right
const LABEL: usize = 15;

/// One row of the table
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// A group's title, with its size
    Heading(Group),
    Home,
    Area(Area),
    /// What is left of the Data volume after everything measured
    Unknown,
    /// An index into the container's volumes
    Volume(usize),
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Data,
    Volumes,
}

enum State {
    Looking(
        Instant,
        std::sync::mpsc::Receiver<Result<Breakdown, String>>,
    ),
    Ready(Breakdown),
    Failed(String),
}

/// The Space screen
pub struct Space {
    home: PathBuf,
    /// The home folder's size, from the home scan
    home_size: u64,
    state: State,
    table: TableState,
}

impl Space {
    pub fn new(home: PathBuf, home_size: u64) -> Self {
        let mut screen = Self {
            home,
            home_size,
            state: State::Failed(String::new()),
            table: TableState::default(),
        };
        screen.look();
        screen
    }

    #[cfg(test)]
    fn from_breakdown(home_size: u64, breakdown: Breakdown) -> Self {
        let mut screen = Self {
            home: PathBuf::from("/Users/test"),
            home_size,
            state: State::Ready(breakdown),
            table: TableState::default(),
        };
        screen.select_first();
        screen
    }

    /// Measures on its own thread, since walking the Data volume takes a while.
    fn look(&mut self) {
        let home = self.home.clone();
        let receiver = tools::spawn(move || space::look(&home).map_err(|error| error.to_string()));
        self.state = State::Looking(Instant::now(), receiver);
    }

    fn poll(&mut self) {
        let State::Looking(_, receiver) = &self.state else {
            return;
        };
        self.state = match receiver.try_recv() {
            Ok(Ok(breakdown)) => State::Ready(breakdown),
            Ok(Err(error)) => State::Failed(error),
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                State::Failed("The task ended without a result.".to_string())
            }
        };
        self.select_first();
    }

    fn breakdown(&self) -> Option<&Breakdown> {
        match &self.state {
            State::Ready(breakdown) => Some(breakdown),
            _ => None,
        }
    }

    /// Space on the Data volume neet could not measure: the volume's size
    /// less the home folder and every area
    fn unknown(&self, breakdown: &Breakdown) -> u64 {
        let measured: u64 = breakdown.areas.values().map(|area| area.size).sum();
        breakdown
            .container
            .data()
            .unwrap_or(0)
            .saturating_sub(self.home_size)
            .saturating_sub(measured)
    }

    fn rows(&self) -> Vec<Part> {
        let Some(breakdown) = self.breakdown() else {
            return Vec::new();
        };
        let mut rows = vec![Part::Heading(Group::Data), Part::Home];
        rows.extend(
            Area::ALL
                .into_iter()
                .filter(|area| breakdown.areas.get(area).is_some_and(|m| m.size > 0))
                .map(Part::Area),
        );
        rows.push(Part::Unknown);
        rows.push(Part::Heading(Group::Volumes));
        rows.extend(
            breakdown
                .container
                .volumes
                .iter()
                .enumerate()
                .filter(|(_, volume)| volume.role != Role::Data)
                .map(|(index, _)| Part::Volume(index)),
        );
        rows.push(Part::Free);
        rows
    }

    fn size(&self, breakdown: &Breakdown, part: Part) -> u64 {
        match part {
            Part::Heading(Group::Data) => breakdown.container.data().unwrap_or(0),
            Part::Heading(Group::Volumes) => breakdown
                .container
                .volumes
                .iter()
                .filter(|volume| volume.role != Role::Data)
                .map(|volume| volume.used)
                .sum(),
            Part::Home => self.home_size,
            Part::Area(area) => breakdown.areas.get(&area).map_or(0, |m| m.size),
            Part::Unknown => self.unknown(breakdown),
            Part::Volume(index) => breakdown.container.volumes[index].used,
            Part::Free => breakdown.container.free,
        }
    }

    fn selected(&self) -> Option<Part> {
        self.table
            .selected()
            .and_then(|index| self.rows().get(index).copied())
    }

    fn select_first(&mut self) {
        let first = self
            .rows()
            .iter()
            .position(|part| !matches!(part, Part::Heading(_)));
        self.table.select(first);
    }

    /// Moves to the next part up or down, past headings
    fn step(&mut self, down: bool) {
        let rows = self.rows();
        let Some(mut index) = self.table.selected() else {
            return;
        };
        loop {
            index = if down {
                index + 1
            } else {
                match index.checked_sub(1) {
                    Some(index) => index,
                    None => return,
                }
            };
            match rows.get(index) {
                None => return,
                Some(Part::Heading(_)) => {}
                Some(_) => break,
            }
        }
        self.table.select(Some(index));
    }

    fn jump(&mut self, last: bool) {
        if last {
            let rows = self.rows();
            self.table.select(rows.len().checked_sub(1));
        } else {
            self.select_first();
        }
    }

    /// The line at the top: used and free space
    fn top(breakdown: &Breakdown) -> Line<'static> {
        let container = &breakdown.container;
        Line::from(vec![
            Span::raw(" "),
            Span::raw(format!(
                "{} used of {}",
                format::size(container.used()),
                format::size(container.total)
            )),
            Span::raw(" · "),
            Span::raw(format!("{} free", format::size(container.free))).green(),
            Span::raw(" "),
        ])
    }

    fn draw_table(&mut self, frame: &mut Frame, area: Rect, breakdown: &Breakdown) {
        let rows = self.rows();
        let used = breakdown.container.used().max(1);
        let largest = rows
            .iter()
            .filter(|part| !matches!(part, Part::Heading(_) | Part::Free))
            .map(|&part| self.size(breakdown, part))
            .max()
            .unwrap_or(0);
        let names: Vec<String> = rows.iter().map(|&part| name(breakdown, part)).collect();
        let name_width = format::column_width("Part", names.iter().map(|name| format!("  {name}")));
        let columns = Columns::new(
            area.width,
            [(name_width, 1), (8, 0), (BAR, 0), (5, 0)],
            &[2, 3],
            true,
        );
        let header = columns
            .row([
                Cell::from("Part"),
                Cell::from(Line::from("Size").right_aligned()),
                Cell::from(""),
                Cell::from(Line::from("Share").right_aligned()),
            ])
            .style(visual::HEADING);
        let table_rows: Vec<Row> = rows
            .iter()
            .zip(&names)
            .map(|(&part, name)| {
                let size = self.size(breakdown, part);
                let text = format::size(size);
                if let Part::Heading(_) = part {
                    return columns.row([
                        Cell::from(Span::styled(name.clone(), visual::HEADING)),
                        Cell::from(Line::from(Span::raw(text).bold()).right_aligned()),
                        Cell::from(""),
                        Cell::from(Line::from(percent(size, used)).right_aligned()),
                    ]);
                }
                let (size_span, bar) = match part {
                    Part::Free => (
                        Span::raw(text).green(),
                        Span::raw(format::bar(
                            size,
                            breakdown.container.total,
                            usize::from(BAR),
                        ))
                        .green(),
                    ),
                    Part::Unknown => (
                        Span::raw(text).yellow(),
                        Span::raw(format::bar(size, largest, usize::from(BAR))).yellow(),
                    ),
                    _ => (
                        format::size_span(size, text),
                        format::size_bar(size, largest, usize::from(BAR)),
                    ),
                };
                let share = if part == Part::Free {
                    String::new()
                } else {
                    percent(size, used)
                };
                columns.row([
                    Cell::from(format!("  {name}")),
                    Cell::from(Line::from(size_span).right_aligned()),
                    Cell::from(bar),
                    Cell::from(Line::from(share).right_aligned()),
                ])
            })
            .collect();
        let table = Table::new(table_rows, columns.widths())
            .header(header)
            .column_spacing(2)
            .block(
                visual::block()
                    .title(" Space ")
                    .title(Self::top(breakdown).right_aligned())
                    .padding(Padding::horizontal(1)),
            )
            .highlight_symbol("▸ ")
            .row_highlight_style(visual::SELECTED);
        frame.render_stateful_widget(table, area, &mut self.table);
    }

    /// What the selected part is, where, and what you can do about it
    fn about(&self, breakdown: &Breakdown, part: Part, width: usize) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from(name(breakdown, part)).style(visual::HEADING)];
        let size = self.size(breakdown, part);
        lines.push(field("Size", format::size_span(size, format::size(size))));
        if let Some(place) = place(part) {
            lines.push(field("Where", Span::raw(place).fg(visual::ACCENT)));
        }
        if let Part::Area(area) = part
            && let Some(unreadable) = breakdown
                .areas
                .get(&area)
                .map(|m| m.unreadable)
                .filter(|&count| count > 0)
        {
            lines.push(field(
                "Unreadable",
                Span::raw(format!(
                    "{} {}, so it may be larger",
                    format::count(unreadable),
                    if unreadable == 1 { "folder" } else { "folders" }
                ))
                .yellow(),
            ));
        }
        lines.push(Line::default());
        lines.extend(visual::hanging(
            Vec::new(),
            0,
            &meaning(breakdown, part),
            ratatui::style::Style::default(),
            width,
        ));
        lines
    }

    /// How the parts add up to the used space
    fn adds_up(&self, breakdown: &Breakdown) -> Vec<Line<'static>> {
        let container = &breakdown.container;
        let data = container.data().unwrap_or(0);
        let measured = data.saturating_sub(self.unknown(breakdown));
        vec![
            Line::from("Adds up").style(visual::HEADING),
            field("Data", Span::raw(format::size(data))),
            field(
                "Measured",
                Span::raw(format!(
                    "{} · {} of Data",
                    format::size(measured),
                    percent(measured, data.max(1))
                ))
                .green(),
            ),
            field(
                "Not measured",
                Span::raw(format::size(self.unknown(breakdown))).yellow(),
            ),
            field(
                "Other volumes",
                Span::raw(format::size(
                    self.size(breakdown, Part::Heading(Group::Volumes)),
                )),
            ),
            field("Used", Span::raw(format::size(container.used())).bold()),
        ]
    }

    /// Free space, purgeable space, and snapshots
    fn free(breakdown: &Breakdown, purgeable: Option<u64>) -> Vec<Line<'static>> {
        let free = breakdown.container.free;
        let mut lines = vec![
            Line::from("Free space").style(visual::HEADING),
            field("Free now", Span::raw(format::size(free)).green()),
        ];
        match purgeable {
            Some(purgeable) => {
                lines.push(field("Purgeable", Span::raw(format::size(purgeable))));
                lines.push(field(
                    "Finder shows",
                    Span::raw(format::size(free + purgeable)),
                ));
            }
            None => lines.push(field("Purgeable", Span::raw("looking…"))),
        }
        lines.push(field(
            "Snapshots",
            Span::raw(match breakdown.snapshots {
                Some(0) => "none".to_string(),
                Some(1) => "1, its size is in Not measured".to_string(),
                Some(count) => format!("{count}, their size is in Not measured"),
                None => "unknown".to_string(),
            }),
        ));
        lines
    }

    fn draw_ready(&mut self, frame: &mut Frame, area: Rect, purgeable: Option<u64>) {
        let Some(breakdown) = self.breakdown().cloned() else {
            return;
        };
        let rows = u16::try_from(self.rows().len()).unwrap_or(u16::MAX);
        let table = (rows + 3).min(area.height);
        if area.width >= SIDE_WIDTH {
            let [left, right] =
                Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)]).areas(area);
            // The table only as tall as its rows, with how they add up below
            let adds_up = self.adds_up(&breakdown);
            let below = u16::try_from(adds_up.len()).unwrap_or(u16::MAX) + 1;
            if left.height >= table + below {
                let [list, rest] =
                    Layout::vertical([Constraint::Length(table), Constraint::Fill(1)]).areas(left);
                self.draw_table(frame, list, &breakdown);
                let width = usize::from(rest.width.saturating_sub(4));
                visual::sections(frame, rest, "", vec![adds_up, notes(width)]);
            } else {
                self.draw_table(frame, left, &breakdown);
            }
            let width = usize::from(right.width.saturating_sub(4));
            let mut sections = Vec::new();
            if let Some(part) = self.selected() {
                sections.push(self.about(&breakdown, part, width));
            }
            sections.push(Self::free(&breakdown, purgeable));
            visual::sections(frame, right, "", sections);
        } else {
            let [list, below] =
                Layout::vertical([Constraint::Length(table), Constraint::Fill(1)]).areas(area);
            self.draw_table(frame, list, &breakdown);
            if below.height > 2
                && let Some(part) = self.selected()
            {
                let width = usize::from(below.width.saturating_sub(4));
                visual::sections(frame, below, "", vec![self.about(&breakdown, part, width)]);
            }
        }
    }
}

/// Why the parts are estimates
fn notes(width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("Notes").style(visual::HEADING)];
    for text in [
        "neet reads folders without admin rights, so some space stays Not measured.",
        "Copies on APFS can share space, so folders can add up to more than they use.",
        "Press r to measure again after a cleanup.",
    ] {
        lines.extend(visual::hanging(
            vec![Span::raw("• ").fg(visual::ACCENT)],
            2,
            text,
            ratatui::style::Style::default(),
            width,
        ));
    }
    lines
}

/// A part's name in the table
fn name(breakdown: &Breakdown, part: Part) -> String {
    match part {
        Part::Heading(Group::Data) => "Data volume".to_string(),
        Part::Heading(Group::Volumes) => "Other volumes".to_string(),
        Part::Home => "Your home folder".to_string(),
        Part::Area(area) => area_name(area).to_string(),
        Part::Unknown => "Not measured".to_string(),
        Part::Volume(index) => volume_name(&breakdown.container.volumes[index]),
        Part::Free => "Free".to_string(),
    }
}

fn area_name(area: Area) -> &'static str {
    match area {
        Area::Apps => "Apps",
        Area::Library => "Shared Library",
        Area::SystemDownloads => "macOS downloads",
        Area::SystemData => "macOS data",
        Area::Tools => "Command line tools",
        Area::OtherUsers => "Other users",
        Area::Other => "Other folders",
    }
}

fn volume_name(volume: &Volume) -> String {
    match volume.role {
        Role::System => "macOS".to_string(),
        Role::Preboot => "Startup files".to_string(),
        Role::Recovery => "Recovery".to_string(),
        Role::Vm => "Swap".to_string(),
        Role::Update => "Update".to_string(),
        Role::Data | Role::Other => volume.name.clone(),
    }
}

/// Where a part is, when it is a folder
fn place(part: Part) -> Option<&'static str> {
    Some(match part {
        Part::Home => "~",
        Part::Area(Area::Apps) => "/Applications",
        Part::Area(Area::Library) => "/Library",
        Part::Area(Area::SystemDownloads) => "/System/Library/AssetsV2 and others",
        Part::Area(Area::SystemData) => "/private",
        Part::Area(Area::Tools) => "/opt, /usr/local",
        Part::Area(Area::OtherUsers) => "/Users",
        Part::Area(Area::Other) => space::DATA,
        _ => return None,
    })
}

/// What a part holds, and what you can do about it
fn meaning(breakdown: &Breakdown, part: Part) -> String {
    let text = match part {
        Part::Heading(Group::Data) => {
            "The volume that holds your files, apps, and everything else that changes."
        }
        Part::Heading(Group::Volumes) => "Volumes macOS keeps for itself on the same disk.",
        Part::Home => {
            "Measured by the home scan. Browse it in Disk, and clean it with Quick Clean, Deep Clean, and Large Files."
        }
        Part::Area(Area::Apps) => {
            "Apps installed for every user. Remove App removes an app and the files it keeps."
        }
        Part::Area(Area::Library) => {
            "Files apps keep for every user, such as fonts, plugins, sound libraries, and simulator runtimes. Remove App finds an app's files here."
        }
        Part::Area(Area::SystemDownloads) => {
            "Files macOS downloads, such as voices, language data, and simulator runtime downloads. macOS manages them."
        }
        Part::Area(Area::SystemData) => {
            "macOS's own databases, logs, sleep image, and temporary folders. macOS manages them, and a restart clears some temporary files."
        }
        Part::Area(Area::Tools) => {
            "Homebrew and other command line tools. `brew cleanup` removes old versions Homebrew keeps."
        }
        Part::Area(Area::OtherUsers) => {
            "Other people's home folders, and /Users/Shared. Their files are private, so most of their size is in Not measured."
        }
        Part::Area(Area::Other) => "Other folders at the top of the Data volume.",
        Part::Unknown => {
            let snapshots = match breakdown.snapshots {
                Some(0) | None => String::new(),
                Some(_) => " It also holds the space local snapshots keep.".to_string(),
            };
            return format!(
                "Space neet cannot read without admin rights: other people's files, the Spotlight index, saved file versions, and some macOS data.{snapshots} neet never asks for admin rights, so this stays an estimate."
            );
        }
        Part::Volume(index) => match breakdown.container.volumes[index].role {
            Role::System => {
                "macOS itself, on a volume that cannot change. Only a macOS update changes it."
            }
            Role::Preboot => "What the Mac needs to start up, including files for macOS updates.",
            Role::Recovery => "The recovery system you can start from when something goes wrong.",
            Role::Vm => "Memory macOS moved to the disk. It shrinks when you quit apps or restart.",
            Role::Update => "Room for the next macOS update.",
            Role::Data | Role::Other => "Another volume on the same disk.",
        },
        Part::Free => {
            "Space you can use now. macOS can also clear purgeable files when it runs low, so Finder may show more."
        }
    };
    text.to_string()
}

/// `part` as a percent of `total`, with `<1%` for a part too small to round
/// up to one
fn percent(part: u64, total: u64) -> String {
    match format::percent(part, total) {
        0 if part > 0 => "<1%".to_string(),
        percent => format!("{percent}%"),
    }
}

fn field(label: &str, value: Span<'static>) -> Line<'static> {
    Line::from(vec![Span::raw(format!("{label:<LABEL$}")).bold(), value])
}

impl Screen for Space {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: &Context) {
        self.poll();
        match &self.state {
            State::Looking(started, _) => Loading {
                title: "Space",
                doing: "Measuring folders outside your home folder",
                progress: format!("{}s", started.elapsed().as_secs()),
                note: "Nothing is changed while neet looks.",
            }
            .draw(frame, area),
            State::Failed(reason) => tools::message(
                frame,
                area,
                "Space",
                vec![
                    Line::from(format!("The disk's volumes could not be read: {reason}")),
                    Line::from("Press r to try again, or Esc to go back."),
                ],
                Color::Red,
            ),
            State::Ready(_) => self.draw_ready(frame, area, context.disk.and_then(|d| d.purgeable)),
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _context: &Context) -> Action {
        match key.code {
            KeyCode::Char('r') if !matches!(self.state, State::Looking(..)) => self.look(),
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Char('g') | KeyCode::Home => self.jump(false),
            KeyCode::Char('G') | KeyCode::End => self.jump(true),
            _ => {}
        }
        Action::None
    }

    fn hints(&self) -> &'static str {
        "↑↓ move · r look again · esc back · ? help"
    }

    fn help(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("↑ ↓  j k", "Move between parts"),
            ("g  G", "Jump to the first or last part"),
            ("r", "Measure again"),
            ("Esc", "Back to Disk"),
            ("q", "Quit"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::ScanStatus;
    use crate::ui::visual::tests as view;
    use neet_core::disk::DiskSpace;
    use neet_core::space::{Container, Measured};
    use std::collections::HashMap;

    const GB: u64 = 1_000_000_000;

    fn sample() -> Breakdown {
        let volume = |name: &str, role, used| Volume {
            name: name.to_string(),
            role,
            used,
        };
        let mut areas = HashMap::new();
        areas.insert(
            Area::Apps,
            Measured {
                size: 13 * GB,
                unreadable: 0,
            },
        );
        areas.insert(
            Area::SystemData,
            Measured {
                size: 8 * GB,
                unreadable: 170,
            },
        );
        areas.insert(Area::OtherUsers, Measured::default());
        Breakdown {
            container: Container {
                total: 245 * GB,
                free: 76 * GB,
                volumes: vec![
                    volume("Data", Role::Data, 140 * GB),
                    volume("Macintosh HD", Role::System, 12 * GB),
                    volume("Preboot", Role::Preboot, 11 * GB),
                    volume("Recovery", Role::Recovery, 3 * GB),
                    volume("VM", Role::Vm, 3 * GB),
                ],
            },
            snapshots: Some(0),
            areas,
        }
    }

    fn render(screen: &mut Space, width: u16, height: u16) -> String {
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: Some(DiskSpace {
                total: 245 * GB,
                available: 76 * GB,
                purgeable: Some(7 * GB),
            }),
            cleanable: None,
            plan: None,
        };
        view::text(&view::render("space", screen, &context, width, height))
    }

    fn press(screen: &mut Space, code: KeyCode) {
        let scan = ScanStatus::Failed(String::new());
        let context = Context {
            scan: &scan,
            disk: None,
            cleanable: None,
            plan: None,
        };
        screen.handle_key(
            KeyEvent::new(code, ratatui::crossterm::event::KeyModifiers::NONE),
            &context,
        );
    }

    #[test]
    fn the_parts_add_up_to_the_data_volume() {
        let screen = Space::from_breakdown(76 * GB, sample());
        let breakdown = screen.breakdown().unwrap();

        // 140 less home 76, apps 13, and macOS data 8
        assert_eq!(screen.unknown(breakdown), 43 * GB);
    }

    #[test]
    fn not_measured_is_never_below_zero() {
        let screen = Space::from_breakdown(200 * GB, sample());

        assert_eq!(screen.unknown(screen.breakdown().unwrap()), 0);
    }

    #[test]
    fn lists_parts_by_volume_and_skips_empty_areas() {
        let mut screen = Space::from_breakdown(76 * GB, sample());
        let text = render(&mut screen, 120, 30);

        for part in [
            "Data volume",
            "Your home folder",
            "Apps",
            "macOS data",
            "Not measured",
            "Other volumes",
            "Startup files",
            "Swap",
            "Free",
        ] {
            assert!(text.contains(part), "{part} missing:\n{text}");
        }
        assert!(!text.contains("Other users"), "{text}");
        assert!(text.contains("43.0 GB"), "{text}");
        assert!(text.contains("Finder shows"), "{text}");
    }

    #[test]
    fn moving_skips_headings_and_shows_the_selected_part() {
        let mut screen = Space::from_breakdown(76 * GB, sample());
        assert_eq!(screen.selected(), Some(Part::Home));

        press(&mut screen, KeyCode::Up);
        assert_eq!(screen.selected(), Some(Part::Home));
        for _ in 0..4 {
            press(&mut screen, KeyCode::Down);
        }
        assert_eq!(screen.selected(), Some(Part::Volume(1)));

        press(&mut screen, KeyCode::Char('G'));
        assert_eq!(screen.selected(), Some(Part::Free));
        let text = render(&mut screen, 120, 34);
        assert!(text.contains("Space you can use now"), "{text}");
    }

    #[test]
    fn says_how_many_folders_could_not_be_read() {
        let mut screen = Space::from_breakdown(76 * GB, sample());
        press(&mut screen, KeyCode::Down);
        press(&mut screen, KeyCode::Down);
        assert_eq!(screen.selected(), Some(Part::Area(Area::SystemData)));

        let text = render(&mut screen, 120, 34);

        assert!(text.contains("170 folders"), "{text}");
    }

    #[test]
    fn fits_every_screen_size() {
        for (width, height) in view::SIZES {
            let mut screen = Space::from_breakdown(76 * GB, sample());
            let text = render(&mut screen, width, height);
            assert!(
                text.contains("Your home folder"),
                "{width}x{height}\n{text}"
            );
        }
    }

    #[test]
    fn a_lost_result_stops_loading() {
        let mut screen = Space::from_breakdown(76 * GB, sample());
        let (sender, receiver) = std::sync::mpsc::channel();
        screen.state = State::Looking(Instant::now(), receiver);
        drop(sender);

        screen.poll();

        assert!(matches!(screen.state, State::Failed(_)));
    }
}
