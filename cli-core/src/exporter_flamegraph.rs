use std::fmt;
use std::sync::Mutex;

use inferno::flamegraph;

use super::{Allocation, Data};

use crate::exporter_flamegraph_pl::dump_collation;
use crate::io_adapter::IoAdapter;

pub fn export_as_flamegraph<T, F>(data: &Data, output: T, filter: F)
where
    T: fmt::Write,
    F: Fn(&Allocation) -> bool,
{
    let mut lines = Vec::new();
    dump_collation(data, filter, |line| {
        lines.push(line.to_owned());
        let result: Result<(), ()> = Ok(());
        result
    })
    .unwrap();

    lines.sort_unstable();

    lazy_static::lazy_static! {
        pub static ref PALETTE_MAP: Mutex< flamegraph::color::PaletteMap > = Mutex::new( flamegraph::color::PaletteMap::default() );
    }

    let mut options = flamegraph::Options::default();
    options.colors = flamegraph::color::Palette::Basic(flamegraph::color::BasicPalette::Mem);
    options.bgcolors = Some(flamegraph::color::BackgroundColor::Flat(
        (255, 255, 255).into(),
    ));
    options.font_type =
        r#""Segoe UI", "Source Sans Pro", Calibri, Candara, Arial, sans-serif"#.to_owned();
    options.title = "".to_owned();
    options.count_name = "bytes".to_owned();

    let mut palette_map = PALETTE_MAP.lock();
    if let Ok(ref mut palette_map) = palette_map {
        options.palette_map = Some(palette_map);
    }

    // We explicitly ignore the error to prevent a panic in case
    // we didn't match any allocations.
    let _ = flamegraph::from_lines(
        &mut options,
        lines.iter().map(|line| line.as_str()),
        IoAdapter::new(output),
    );
}
