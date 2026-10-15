use crate::{
    cli::Args,
    search::{Event, Line},
};
use serde_json::json;
use std::io::{self, Write};

pub struct Output<'a, W> {
    pub args: &'a Args,
    pub writer: W,
    pub color: bool,
}

impl<W: Write> Output<'_, W> {
    pub fn event(&mut self, event: Event) -> io::Result<()> {
        match event {
            Event::Line(line) => self.line(line),
            Event::File(path) => writeln!(self.writer, "{path}"),
            Event::Count(path, count) => writeln!(self.writer, "{path}:{count}"),
            Event::Found | Event::Error(_) => Ok(()),
        }
    }

    fn line(&mut self, line: Line) -> io::Result<()> {
        if self.args.json {
            let submatches: Vec<_> = line
                .ranges
                .iter()
                .map(|r| {
                    json!({
                        "start": r.start, "end": r.end, "text": &line.text[r.clone()],
                        "column": line.text[..r.start].chars().count() + 1,
                    })
                })
                .collect();
            let record = json!({
                "type": if line.selected { "match" } else { "context" },
                "path": *line.path, "line_number": line.number,
                "text": line.text, "submatches": submatches,
            });
            serde_json::to_writer(&mut self.writer, &record)?;
            return writeln!(self.writer);
        }
        if self.args.only_matching {
            if line.selected {
                for r in &line.ranges {
                    if r.is_empty() {
                        continue;
                    }
                    self.prefix(&line, Some(r.start))?;
                    if self.color {
                        write!(self.writer, "\x1b[1;31m")?;
                    }
                    write!(self.writer, "{}", &line.text[r.clone()])?;
                    if self.color {
                        write!(self.writer, "\x1b[0m")?;
                    }
                    writeln!(self.writer)?;
                }
            }
            return Ok(());
        }
        self.prefix(&line, line.ranges.first().map(|r| r.start))?;
        let mut previous = 0;
        for range in &line.ranges {
            write!(self.writer, "{}", &line.text[previous..range.start])?;
            if self.color {
                write!(self.writer, "\x1b[1;31m")?;
            }
            write!(self.writer, "{}", &line.text[range.clone()])?;
            if self.color {
                write!(self.writer, "\x1b[0m")?;
            }
            previous = range.end;
        }
        writeln!(self.writer, "{}", &line.text[previous..])
    }

    fn prefix(&mut self, line: &Line, start: Option<usize>) -> io::Result<()> {
        let sep = if line.selected { ':' } else { '-' };
        write!(self.writer, "{}{sep}", line.path)?;
        if !self.args.no_line_number {
            write!(self.writer, "{}{sep}", line.number)?;
        }
        if self.args.column {
            let column = start.map_or(1, |i| line.text[..i].chars().count() + 1);
            write!(self.writer, "{column}{sep}")?;
        }
        Ok(())
    }
}
