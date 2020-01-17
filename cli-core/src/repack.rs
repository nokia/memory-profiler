use std::io::{self, Read, Write};

use common::speedy::{Endianness, Writable};

use common::event::Event;
use common::lz4_stream::Lz4Writer;

use crate::reader::parse_events;

pub fn repack<F, G>(input_fp: F, output_fp: G) -> Result<(), io::Error>
where
    F: Read + Send + 'static,
    G: Write + Send + 'static,
{
    let (header, event_stream) = parse_events(input_fp)?;
    let mut output_fp = Lz4Writer::new(output_fp);

    Event::Header(header).write_to_stream(Endianness::LittleEndian, &mut output_fp)?;
    for event in event_stream {
        let event = event?;
        event.write_to_stream(Endianness::LittleEndian, &mut output_fp)?;
    }

    output_fp.flush()?;

    Ok(())
}
