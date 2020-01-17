extern crate lz4_compress as lz4;

use std::io::{self, Read, Write};
use std::{env, process};

/// The help page for this command.
const HELP: &'static [u8] = br#"
Introduction:
    lz4 - an utility to decompress or compress a raw, headerless LZ4 stream.
Usage:
    lz4 [option]
Options:
    -c : Compress stdin and write the result to stdout.
    -d : Decompress stdin and write the result to stdout.
    -h : Write this manpage to stderr.
"#;

fn main() {
    let mut iter = env::args().skip(1);
    let mut flag = iter.next().unwrap_or(String::new());
    // If another argument is provided (e.g. the user passes a file name), we need to make sure we
    // issue an error properly, so we set back the flag to `""`.
    if iter.next().is_some() {
        flag = String::new();
    }

    match &*flag {
        "-c" => {
            // Read stream from stdin.
            let mut vec = Vec::new();
            io::stdin()
                .read_to_end(&mut vec)
                .expect("Failed to read stdin");

            // Compress it and write the result to stdout.
            io::stdout()
                .write(&lz4::compress(&vec))
                .expect("Failed to write to stdout");
        }
        "-d" => {
            // Read stream from stdin.
            let mut vec = Vec::new();
            io::stdin()
                .read_to_end(&mut vec)
                .expect("Failed to read stdin");

            // Decompress the input.
            let decompressed = lz4::decompress(&vec).expect("Compressed data contains errors");

            // Write the decompressed buffer to stdout.
            io::stdout()
                .write(&decompressed)
                .expect("Failed to write to stdout");
        }
        // If no valid arguments are given, we print the help page.
        _ => {
            io::stdout().write(HELP).expect("Failed to write to stdout");

            process::exit(1);
        }
    }
}
