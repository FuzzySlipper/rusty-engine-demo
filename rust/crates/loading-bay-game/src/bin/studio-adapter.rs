use std::io::{self, BufRead, Write};

use loading_bay_game::{StudioAdapterService, MAX_STUDIO_ADAPTER_REQUEST_BYTES};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = io::BufWriter::new(stdout.lock());
    let mut service = StudioAdapterService::new();

    while let Some(line) = read_bounded_line(&mut input, MAX_STUDIO_ADAPTER_REQUEST_BYTES)? {
        let response = match line {
            BoundedLine::Line(bytes) => {
                let request = String::from_utf8_lossy(&bytes);
                service.handle_json(&request)
            }
            BoundedLine::TooLong => {
                let oversized = " ".repeat(MAX_STUDIO_ADAPTER_REQUEST_BYTES + 1);
                service.handle_json(&oversized)
            }
        };
        output.write_all(response.as_bytes())?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

enum BoundedLine {
    Line(Vec<u8>),
    TooLong,
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> io::Result<Option<BoundedLine>> {
    let mut output = Vec::new();
    let mut too_long = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if output.is_empty() && !too_long {
                Ok(None)
            } else if too_long {
                Ok(Some(BoundedLine::TooLong))
            } else {
                Ok(Some(BoundedLine::Line(output)))
            };
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        if !too_long {
            let payload = &available[..consumed];
            if output.len().saturating_add(payload.len()) > max_bytes {
                too_long = true;
                output.clear();
            } else {
                output.extend_from_slice(payload);
            }
        }
        let ended = available[..consumed].ends_with(b"\n");
        reader.consume(consumed);
        if ended {
            if output.ends_with(b"\n") {
                output.pop();
                if output.ends_with(b"\r") {
                    output.pop();
                }
            }
            return Ok(Some(if too_long {
                BoundedLine::TooLong
            } else {
                BoundedLine::Line(output)
            }));
        }
    }
}
