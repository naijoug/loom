//! Server-sent events framing (WHATWG `text/event-stream`): `event:` and
//! `data:` fields, events separated by a blank line, comments ignored.

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Event {
    pub name: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub(super) struct Decoder {
    pending: Vec<u8>,
    name: Option<String>,
    data: Vec<String>,
}

impl Decoder {
    /// Feeds response bytes and returns the events they complete. `limit`
    /// bounds one unterminated line, so a stream without newlines cannot grow
    /// the buffer without end.
    pub fn feed(&mut self, bytes: &[u8], limit: usize) -> Result<Vec<Event>, String> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        let mut start = 0;
        while let Some(offset) = self.pending[start..].iter().position(|b| *b == b'\n') {
            let end = start + offset;
            let line = std::str::from_utf8(&self.pending[start..end])
                .map_err(|_| "invalid_utf8: model API stream is not UTF-8")?
                .trim_end_matches('\r')
                .to_owned();
            start = end + 1;
            self.line(&line, &mut events);
        }
        self.pending.drain(..start);
        if self.pending.len() > limit {
            return Err("output_limit: model API event is too large".into());
        }
        Ok(events)
    }

    /// Ends the stream; an event without its closing blank line still counts.
    pub fn finish(&mut self) -> Option<Event> {
        let mut events = Vec::new();
        if !self.pending.is_empty() {
            let line = String::from_utf8_lossy(&std::mem::take(&mut self.pending))
                .trim_end_matches('\r')
                .to_owned();
            self.line(&line, &mut events);
        }
        self.line("", &mut events);
        events.pop()
    }

    fn line(&mut self, line: &str, events: &mut Vec<Event>) {
        if line.is_empty() {
            if !self.data.is_empty() {
                events.push(Event {
                    name: self.name.take(),
                    data: std::mem::take(&mut self.data).join("\n"),
                });
            }
            self.name = None;
            return;
        }
        if line.starts_with(':') {
            return;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.name = Some(value.to_owned()),
            "data" => self.data.push(value.to_owned()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_survive_arbitrary_chunk_boundaries() {
        let stream = b": keep-alive\r\nevent: delta\r\ndata: {\"a\":1}\r\n\r\ndata: one\ndata: two\n\ndata: [DONE]";
        for split in 0..stream.len() {
            let mut decoder = Decoder::default();
            let mut events = decoder.feed(&stream[..split], 1024).unwrap();
            events.extend(decoder.feed(&stream[split..], 1024).unwrap());
            events.extend(decoder.finish());
            assert_eq!(
                events,
                vec![
                    Event {
                        name: Some("delta".into()),
                        data: "{\"a\":1}".into()
                    },
                    Event {
                        name: None,
                        data: "one\ntwo".into()
                    },
                    Event {
                        name: None,
                        data: "[DONE]".into()
                    },
                ],
                "split at {split}"
            );
        }
    }

    #[test]
    fn an_unterminated_line_is_bounded() {
        let mut decoder = Decoder::default();
        assert!(decoder.feed(&[b'x'; 64], 32).is_err());
        assert!(Decoder::default().feed(b"data: \xff\n", 32).is_err());
    }
}
