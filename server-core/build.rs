use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

fn grab_paths<P: AsRef<Path>>(path: P, output: &mut Vec<PathBuf>) {
    let path = path.as_ref();
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            _ => continue,
        };

        output.push(entry.path().into());
    }
}

fn main() {
    let src_out_dir: PathBuf = env::var_os("OUT_DIR").expect("missing OUT_DIR").into();
    let crate_root: PathBuf = env::var_os("CARGO_MANIFEST_DIR")
        .expect("missing CARGO_MANIFEST_DIR")
        .into();

    let webui_dir = crate_root.join("..").join("webui");
    let webui_out_dir = crate_root.join("..").join("target").join("webui");

    struct Lock {
        semaphore: Option<semalock::Semalock>,
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            let _ = self.semaphore.take().unwrap().unlink();
        }
    }

    let lock_path = crate_root.join("..").join("target").join(".webui-lock");
    let mut lock = Lock {
        semaphore: Some(
            semalock::Semalock::new(&lock_path).expect("failed to acquire a semaphore"),
        ),
    };

    lock.semaphore.as_mut().unwrap().with( |_| {
        let _ = fs::remove_dir_all( &webui_out_dir );

        match Command::new( "yarn" ).args( &[ "--version" ] ).status() {
            Err( ref error ) if error.kind() == io::ErrorKind::NotFound => {
                panic!( "Yarn not found; you need to install it before you can build the WebUI" );
            },
            Err( error ) => {
                panic!( "Cannot launch Yarn: {}", error );
            },
            Ok( _ ) => {}
        }

        if !webui_dir.join( "node_modules" ).exists() {
            let mut child = Command::new( "yarn" )
                .args( &[ "install" ] )
                .current_dir( &webui_dir )
                .spawn()
                .expect( "cannot launch a child process to install the dependencies for the WebUI" );

            match child.wait() {
                Err( _ ) => {
                    panic!( "Failed to install the dependencies for the WebUI!" );
                },
                Ok( status ) if !status.success() => {
                    panic!( "Failed to install the dependencies for the WebUI; child process exited with error code {:?}! You might want to try to run 'rm -Rf ~/.cache/yarn' and try again.", status.code() );
                },
                Ok( _ ) => {}
            }
        }

        assert!( webui_dir.join( "node_modules" ).exists() );

        let mut child = Command::new( "/bin/sh" )
            .args( &[ "-c", "$(yarn bin)/parcel build src/index.html -d ../target/webui" ] )
            .current_dir( &webui_dir )
            .spawn()
            .expect( "cannot launch a child process to build the WebUI" );

        match child.wait() {
            Err( _ ) => {
                panic!( "Failed to build WebUI!" );
            },
            Ok( status ) if !status.success() => {
                panic!( "Failed to build WebUI; child process exited with error code {:?}!", status.code() );
            },
            Ok( _ ) => {}
        }

        let webui_out_dir = webui_out_dir.canonicalize().unwrap();
        let mut assets: Vec< PathBuf > = Vec::new();
        grab_paths( &webui_out_dir, &mut assets );

        let mut fp = File::create( src_out_dir.join( "webui_assets.rs" ) ).unwrap();
        writeln!( fp, "#[cfg(not(test))]" ).unwrap();
        writeln!( fp, "static WEBUI_ASSETS: &'static [(&'static str, &'static [u8])] = &[" ).unwrap();
        for asset in &assets {
            let target_path = asset.canonicalize().unwrap();
            let key = target_path.strip_prefix( &webui_out_dir ).unwrap();
            writeln!( fp, r#"    ("{}", include_bytes!( "{}" )),"#, key.to_str().unwrap(), target_path.to_str().unwrap() ).unwrap();
        }
        writeln!( fp, "];" ).unwrap();

        writeln!( fp, "#[cfg(test)]" ).unwrap();
        writeln!( fp, "static WEBUI_ASSETS: &'static [(&'static str, &'static [u8])] = &[" ).unwrap();
        writeln!( fp, "];" ).unwrap();
    }).unwrap();

    let mut paths: Vec<PathBuf> = Vec::new();
    paths.push(webui_dir.join(".babelrc"));
    paths.push(webui_dir.join("node_modules"));
    paths.push(webui_dir.join("package.json"));
    grab_paths(webui_dir.join("src"), &mut paths);

    for path in paths {
        println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
    }
}
