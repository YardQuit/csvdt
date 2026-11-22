//! --lines: running on some lines of the input rather than all of them.

use std::error::Error;

/// The lines '--lines' names, as ranges of physical line numbers counted from
/// 1, in order, with ranges that overlap or meet joined into one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LineSelection {
    /// Each range as its first and last line, both included. Never empty, and
    /// no two ranges overlap or meet.
    ranges: Vec<(u64, u64)>,
}

impl LineSelection {
    /// Reads RANGES as '--lines' takes it: comma-joined parts, each a line
    /// number or two joined by '-', the first no greater than the second.
    /// Lines are counted from 1. Parts may overlap and come in any order.
    pub(crate) fn parse(ranges: &str) -> Result<Self, Box<dyn Error>> {
        let mut parsed = ranges
            .split(',')
            .map(|part| parse_range(part, ranges))
            .collect::<Result<Vec<_>, _>>()?;
        parsed.sort_unstable();
        let mut joined: Vec<(u64, u64)> = Vec::with_capacity(parsed.len());
        for (first, last) in parsed {
            match joined.last_mut() {
                Some(previous) if first <= previous.1.saturating_add(1) => previous.1 = previous.1.max(last),
                _ => joined.push((first, last)),
            }
        }
        Ok(LineSelection { ranges: joined })
    }

    /// The last line selected, past which nothing more can be taken.
    pub(crate) fn last_line(&self) -> u64 {
        self.ranges.last().map_or(0, |&(_, last)| last)
    }
}

/// Reads one part of a '--lines' value, which is quoted whole in any refusal.
fn parse_range(part: &str, whole: &str) -> Result<(u64, u64), Box<dyn Error>> {
    let (first, last) = match part.split_once('-') {
        Some((first, last)) => (parse_line(first, whole)?, parse_line(last, whole)?),
        None => {
            let line = parse_line(part, whole)?;
            (line, line)
        },
    };
    if first > last {
        return Err(refusal(format!(
            "was given \"{whole}\", where {first}-{last} runs backwards. Write a \
             range first line first, as {last}-{first}.")));
    }
    Ok((first, last))
}

/// Reads one line number of a '--lines' value WHOLE.
fn parse_line(text: &str, whole: &str) -> Result<u64, Box<dyn Error>> {
    let detail = if text.is_empty() {
        format!("was given \"{whole}\", which leaves a line number empty. A stray \
                 comma or dash does that, as in \"3,\" or \"3-\", and so does an \
                 unset shell variable.")
    } else if text.chars().any(char::is_whitespace) {
        format!("was given \"{whole}\", which contains a space. Write it without \
                 one, as 3-5,9 rather than 3-5, 9.")
    } else if !text.chars().all(|character| character.is_ascii_digit()) {
        format!("was given \"{whole}\", which is not a list of lines. A part is \
                 a line number or a range of them, as in 12-40 or 3-5,9,20-22.")
    } else {
        match text.parse::<u64>() {
            Ok(0) => format!("was given \"{whole}\". Lines are counted from 1, \
                              as an editor counts them, so there is no line 0."),
            Ok(line) => return Ok(line),
            Err(_) => format!("was given \"{whole}\", which holds a number too \
                               large to be a line number."),
        }
    };
    Err(refusal(detail))
}

/// A refusal of a '--lines' value, saying what was wrong with it in DETAIL.
fn refusal(detail: String) -> Box<dyn Error> {
    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("'--lines' {detail}")))
}

/// Writes RANGES the way '--lines' reads them: "3-7,12".
pub(crate) fn render_ranges(ranges: &[(u64, u64)]) -> String {
    let parts: Vec<String> = ranges
        .iter()
        .map(|&(first, last)| match first == last {
            true => first.to_string(),
            false => format!("{first}-{last}"),
        })
        .collect();
    parts.join(",")
}

/// A run's way through a selection: which of the records it reads, in input
/// order, the selection takes, and the lines those records cover.
///
/// A record is taken when any line of it is selected, and then all of it is,
/// since a quoted field can carry a record across lines. Asking costs a
/// comparison or two, because the records come in order and so does the
/// selection: the walk only ever moves forward through it.
#[derive(Debug)]
pub(crate) struct SelectionWalk<'a> {
    /// What was selected.
    selection: &'a LineSelection,
    /// The first range not yet wholly behind the records read so far.
    next: usize,
    /// The lines of the records taken, joined into ranges as they come.
    taken: Vec<(u64, u64)>,
    /// Whether a record taken has a line the selection did not name.
    widened: bool,
}

impl<'a> SelectionWalk<'a> {
    /// Starts a walk through SELECTION from the top of the input.
    pub(crate) fn new(selection: &'a LineSelection) -> Self {
        SelectionWalk { selection, next: 0, taken: Vec::new(), widened: false }
    }

    /// Whether the record on lines FIRST to LAST, the next one in the input,
    /// is taken. When it is, its lines are counted as taken.
    pub(crate) fn takes(&mut self, first: u64, last: u64) -> bool {
        let ranges = &self.selection.ranges;
        while self.next < ranges.len() && ranges[self.next].1 < first {
            self.next += 1;
        }
        let Some(&(from, to)) = ranges.get(self.next) else { return false };
        if from > last {
            return false;
        }
        self.widened |= first < from || last > to;
        match self.taken.last_mut() {
            Some(previous) if first <= previous.1 + 1 => previous.1 = previous.1.max(last),
            _ => self.taken.push((first, last)),
        }
        true
    }

    /// Whether a record ending on line LAST leaves no selected line after it,
    /// so the run can stop reading.
    pub(crate) fn is_done_after(&self, last: u64) -> bool {
        last >= self.selection.last_line()
    }

    /// What the selection came to, where that is not simply the lines named:
    /// that records were widened to take whole ones, naming every line taken
    /// in the form '--lines' reads, or that no record was taken at all.
    /// Said on stderr at the end of the run.
    pub(crate) fn summary(&self) -> Option<String> {
        if self.taken.is_empty() {
            Some("'--lines' took no record: the lines selected hold only comment or \
                  blank lines, or lie past the end of the input".to_string())
        } else if self.widened {
            Some(format!("'--lines' widened to whole records, taking lines {}", render_ranges(&self.taken)))
        } else {
            None
        }
    }
}
