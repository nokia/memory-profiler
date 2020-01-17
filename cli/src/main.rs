#[macro_use]
extern crate log;

use std::env;
use std::error::Error;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process;

use structopt::StructOpt;

use cli_core::{export_as_heaptrack, export_as_replay, postprocess, Loader};

#[derive(StructOpt, Debug)]
enum Opt {
    /// Generates a raw data file which can be used to replay all of the allocations
    #[structopt(name = "export-replay")]
    ExportReplay {
        #[structopt(short = "o", long = "output", parse(from_os_str))]
        output: PathBuf,
        #[structopt(parse(from_os_str))]
        input: PathBuf,
    },
    /// Generates a raw data file which can be loaded into heaptrack GUI
    #[structopt(name = "export-heaptrack")]
    ExportHeaptrack {
        /// A file or directory with extra debugging symbols; can be specified multiple times
        #[structopt(short = "d", long = "debug-symbols", parse(from_os_str))]
        debug_symbols: Vec<PathBuf>,
        #[structopt(short = "o", long = "output", parse(from_os_str))]
        output: PathBuf,
        #[structopt(parse(from_os_str))]
        input: PathBuf,
    },
    /// Gathers memory tracking data from a given machine
    #[structopt(name = "gather")]
    Gather { target: Option<String> },
    /// Launches a server with all of the data exposed through a REST API
    #[cfg(feature = "subcommand-server")]
    #[structopt(name = "server")]
    Server {
        /// A file or directory with extra debugging symbols; can be specified multiple times
        #[structopt(short = "d", long = "debug-symbols", parse(from_os_str))]
        debug_symbols: Vec<PathBuf>,
        /// The network interface on which to start the HTTP server
        #[structopt(short = "i", long = "interface", default_value = "127.0.0.1")]
        interface: String,
        /// The port on which to start the HTTP server
        #[structopt(short = "p", long = "port", default_value = "8080")]
        port: u16,
        #[structopt(parse(from_os_str), required = false)]
        input: Vec<PathBuf>,
    },
    /// Generates a new data file with all of the stack traces decoded and deduplicated
    #[structopt(name = "postprocess")]
    Postprocess {
        /// A file or directory with extra debugging symbols; can be specified multiple times
        #[structopt(short = "d", long = "debug-symbols", parse(from_os_str))]
        debug_symbols: Vec<PathBuf>,

        /// The file to which the postprocessed data will be written
        #[structopt(long, short = "o", parse(from_os_str))]
        output: PathBuf,

        #[structopt(parse(from_os_str), required = false)]
        input: PathBuf,
    },
    /// Generates a new data file with all temporary allocations stripped away
    #[structopt(name = "squeeze")]
    Squeeze {
        /// The file to which the squeezed data will be written
        #[structopt(long, short = "o", parse(from_os_str))]
        output: PathBuf,

        #[structopt(long, parse(from_os_str))]
        tmpfile: Option<PathBuf>,

        #[structopt(parse(from_os_str), required = false)]
        input: PathBuf,
    },
    #[structopt(name = "repack", raw(setting = "structopt::clap::AppSettings::Hidden"))]
    Repack {
        #[structopt(long, short = "o", parse(from_os_str))]
        output: PathBuf,

        #[structopt(parse(from_os_str), required = false)]
        input: PathBuf,
    },
}

fn run(opt: Opt) -> Result<(), Box<dyn Error>> {
    match opt {
        Opt::ExportReplay { output, input } => {
            let fp = File::open(input)?;
            let data = Loader::load_from_stream_without_debug_info(fp)?;
            let data_out = File::create(output)?;
            let data_out = io::BufWriter::new(data_out);

            export_as_replay(&data, data_out, |_| true)?;
        }
        Opt::ExportHeaptrack {
            debug_symbols,
            output,
            input,
        } => {
            let fp = File::open(input)?;
            let data = Loader::load_from_stream(fp, debug_symbols)?;
            let data_out = File::create(output)?;
            let data_out = io::BufWriter::new(data_out);

            export_as_heaptrack(&data, data_out, |_| true)?;
        }
        Opt::Gather { target } => {
            cli_core::cmd_gather::main(target.as_ref().map(|target| target.as_str()))?;
        }
        #[cfg(feature = "subcommand-server")]
        Opt::Server {
            debug_symbols,
            input,
            interface,
            port,
        } => {
            server_core::main(input, debug_symbols, false, &interface, port)?;
        }
        Opt::Postprocess {
            debug_symbols,
            output,
            input,
        } => {
            let ifp = File::open(input)?;
            let ofp = File::create(output)?;
            postprocess(ifp, ofp, debug_symbols)?;
        }
        Opt::Squeeze {
            output,
            tmpfile,
            input,
        } => {
            let ifp = File::open(&input)?;
            let ofp = File::create(output)?;
            let tmpfile = match tmpfile {
                Some(tmpfile) => tmpfile,
                None => format!("{}.tmp", input.to_str().unwrap()).into(),
            };
            cli_core::squeeze_data(ifp, ofp, tmpfile.as_ref())?;
        }
        Opt::Repack { input, output } => {
            let ifp = File::open(&input)?;
            let ofp = File::create(output)?;
            cli_core::repack(ifp, ofp)?;
        }
    }

    Ok(())
}

fn main() {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }

    env_logger::init();

    let opt = Opt::from_args();
    let result = run(opt);
    if let Err(error) = result {
        error!("{}", error);
        if !log_enabled!(log::Level::Error) {
            println!("ERROR: {}", error);
        }

        process::exit(1);
    }
}
