use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::{self, ErrorKind, Write};
use std::mem;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use chrono::prelude::*;
use common::speedy::{Endianness, Readable, Writable};

use common::event::DataId;
use common::get_local_ips;
use common::request::{BroadcastHeader, Request, Response, PROTOCOL_VERSION};

use crate::util::{on_ctrlc, ReadableDuration, Sigint};

struct Wrapper {
    sigint: Sigint,
    stream: TcpStream,
}

impl Wrapper {
    fn new(sigint: Sigint, stream: TcpStream) -> io::Result<Self> {
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        Ok(Wrapper { sigint, stream })
    }
}

impl io::Read for Wrapper {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            if self.sigint.was_sent() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "interrupted by SIGINT",
                ));
            }

            match self.stream.read(buffer) {
                Err(ref error)
                    if error.kind() == ErrorKind::WouldBlock
                        || error.kind() == ErrorKind::TimedOut =>
                {
                    Request::Ping.write_to_stream(Endianness::LittleEndian, &self.stream)?;
                    continue;
                }
                result => return result,
            }
        }
    }
}

fn client_loop(
    socket: TcpStream,
    mut fp: File,
    sigint: Sigint,
    mut ip_lock: Option<MutexGuard<()>>,
) -> Result<(), io::Error> {
    let timestamp = Instant::now();
    let address = socket.peer_addr().unwrap();
    let mut socket = Wrapper::new(sigint.clone(), socket)?;

    while !sigint.was_sent() {
        if ip_lock.is_some() && timestamp.elapsed() > Duration::from_secs(60) {
            ip_lock = None;
        }

        let response = Response::read_from_stream(Endianness::LittleEndian, &mut socket);
        match response {
            Ok(Response::Data(data)) => {
                fp.write_all(&data)?;
            }
            Ok(Response::FinishedInitialStreaming) => {
                info!(
                    "Initial data received from {}; starting online gathering...",
                    address
                );

                // This is here to avoid triggering an avalanche
                // of simultaneous downloads when we're gathering from
                // multiple sources which originate from the same machine.
                let ip_lock = ip_lock.take();
                mem::drop(ip_lock);
            }
            Ok(Response::Finished) => {
                info!("Received an explicit finish from {}", address);
                return Ok(());
            }
            Ok(_) => {}
            Err(ref error) if error.kind() == ErrorKind::UnexpectedEof => {
                return Ok(());
            }
            Err(ref error)
                if error.kind() == ErrorKind::Other
                    && format!("{}", error) == "interrupted by SIGINT" =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        }
    }

    Ok(())
}

fn connect<A: ToSocketAddrs>(target: A) -> Result<(TcpStream, File, String), io::Error> {
    let socket = TcpStream::connect(target)?;
    let target = socket.peer_addr().unwrap();
    let response = Response::read_from_stream(Endianness::LittleEndian, &socket)?;
    match response {
        Response::Start(BroadcastHeader {
            pid,
            executable,
            arch,
            timestamp,
            initial_timestamp,
            ..
        }) => {
            let executable = String::from_utf8_lossy(&executable);
            info!("Connection established to {}:", target);
            info!("  Executable: {}", executable);
            info!(
                "      Uptime: {}",
                ReadableDuration(timestamp.as_secs() - initial_timestamp.as_secs())
            );
            info!("         PID: {}", pid);
            info!("        Arch: {}", arch);

            let basename: String = executable
                [executable.rfind("/").map(|index| index + 1).unwrap_or(0)..]
                .chars()
                .map(|ch| if ch.is_alphanumeric() { ch } else { '_' })
                .collect();

            let now = Utc::now();
            let filename = format!(
                "{}{:02}{:02}_{:02}{:02}{:02}_{:05}_{}.dat",
                now.year(),
                now.month(),
                now.day(),
                now.hour(),
                now.minute(),
                now.second(),
                pid,
                basename
            );
            info!("Gathering events to '{}'...", filename);

            let fp = match File::create(&filename) {
                Ok(fp) => fp,
                Err(error) => {
                    error!("Unable to create '{}': {}", filename, error);
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "unable to create output file",
                    ));
                }
            };

            Request::StartStreaming.write_to_stream(Endianness::LittleEndian, &socket)?;

            Ok((socket, fp, filename))
        }
        _ => return Err(io::Error::new(io::ErrorKind::Other, "unexpected message")),
    }
}

struct ClientLifetime {
    id: DataId,
    clients: Arc<Mutex<HashSet<DataId>>>,
}

impl ClientLifetime {
    fn new(clients: &Arc<Mutex<HashSet<DataId>>>, id: DataId) -> Option<ClientLifetime> {
        let mut guard = clients.lock().unwrap();
        if guard.contains(&id) {
            return None;
        }

        guard.insert(id);
        Some(ClientLifetime {
            id,
            clients: clients.clone(),
        })
    }
}

impl Drop for ClientLifetime {
    fn drop(&mut self) {
        self.clients.lock().unwrap().remove(&self.id);
    }
}

pub fn main(target: Option<&str>) -> Result<(), Box<dyn Error>> {
    let clients: Arc<Mutex<HashSet<DataId>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut locks: HashMap<IpAddr, Arc<Mutex<()>>> = HashMap::new();
    let sigint = on_ctrlc();
    match target {
        None => {
            let mut buffer = Vec::new();
            buffer.resize(1024 * 8, 0);
            let socket = UdpSocket::bind("0.0.0.0:43512").expect("cannot bind the UDP socket");
            socket
                .set_read_timeout(Some(Duration::from_millis(100)))
                .expect("cannot set read timeout");

            info!("Scanning...");
            while !sigint.was_sent() {
                if let Ok((byte_count, addr)) = socket.recv_from(&mut buffer) {
                    let ip = if get_local_ips()
                        .iter()
                        .any(|&local_ip| addr.ip() == local_ip)
                    {
                        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
                    } else {
                        addr.ip()
                    };

                    let start_body = match BroadcastHeader::read_from_buffer(
                        Endianness::LittleEndian,
                        &buffer[..byte_count],
                    ) {
                        Ok(start_body) => start_body,
                        Err(err) => {
                            error!(
                                "Failed to deserialize broadcast handshake packet from '{}': {:?}",
                                addr.ip(),
                                err
                            );
                            continue;
                        }
                    };

                    if start_body.protocol_version > PROTOCOL_VERSION {
                        error!(
                            "The client at '{}' is using a newer protocol version ({}) than expected ({}); you need to update",
                            addr.ip(),
                            start_body.protocol_version,
                            PROTOCOL_VERSION
                        );

                        continue;
                    }

                    let id = start_body.id;
                    let lifetime = match ClientLifetime::new(&clients, id) {
                        Some(lifetime) => lifetime,
                        None => continue,
                    };

                    let addr = SocketAddr::new(ip, start_body.listener_port);
                    info!("Found a new client {}", addr);

                    let sigint = sigint.clone();
                    let ip_lock = locks
                        .entry(addr.ip())
                        .or_insert_with(|| Arc::new(Mutex::new(())))
                        .clone();
                    thread::spawn(move || {
                        let _lifetime = lifetime;
                        let ip_lock = ip_lock.lock().unwrap();

                        info!("Trying to connect to {}...", addr);
                        let (socket, fp, filename) = match connect(addr) {
                            Ok(value) => value,
                            Err(err) => {
                                error!("Failed to connect to '{}': {}", addr, err);
                                return;
                            }
                        };
                        match client_loop(socket, fp, sigint, Some(ip_lock)) {
                            Ok(()) => info!(
                                "Gathering finished for {}; '{}' is now complete",
                                addr, filename
                            ),
                            Err(err) => error!("Gathering failed for {}: {:?}", addr, err),
                        }
                    });
                }
            }
        }
        Some(target) => {
            let (socket, fp, _) = connect(target)?;
            match client_loop(socket, fp, sigint, None) {
                Ok(()) => info!("Gathering finished successfully!"),
                Err(err) => error!("Gathering failed: {:?}", err),
            }
        }
    }

    info!("Finished!");
    Ok(())
}
