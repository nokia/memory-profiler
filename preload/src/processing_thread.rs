use std::fs::{self, remove_file, File};
use std::hash::Hash;
use std::mem;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use std::os::unix::io::AsRawFd;

use std::io::{self, Seek, SeekFrom, Write};

use common::speedy::{Endianness, Readable, Writable};

use common::event::{AllocBody, DataId, Event};
use common::get_local_ips;
use common::lz4_stream::Lz4Writer;
use common::request::{BroadcastHeader, Request, Response, PROTOCOL_VERSION};

use crate::arch;
use crate::event::{send_event, timed_recv_all_events, InternalEvent};
use crate::global::{get_tls, AllocationLock};
use crate::opt;
use crate::timestamp::{get_timestamp, get_wall_clock, Timestamp};
use crate::utils::{copy, generate_filename, temporarily_change_umask};
use crate::writer_memory;
use crate::writers;
use crate::{CMDLINE, EXECUTABLE, PID};

fn get_hash<T: Hash>(value: T) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn generate_data_id() -> DataId {
    let pid = *PID;
    let cmdline = &*CMDLINE;
    let executable = &*EXECUTABLE;

    let mut timespec = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };

    unsafe {
        libc::clock_gettime(libc::CLOCK_REALTIME, &mut timespec);
    }

    let a = get_hash(&pid as *const _ as usize) ^ get_hash(pid) ^ get_hash(timespec.tv_sec);
    let b = get_hash(cmdline) ^ get_hash(executable) ^ get_hash(timespec.tv_nsec);

    DataId::new(a, b)
}

struct Output {
    file: Option<(PathBuf, File)>,
    clients: Vec<Client>,
}

impl Output {
    fn new() -> Self {
        Output {
            file: None,
            clients: Vec::new(),
        }
    }

    fn set_file(&mut self, fp: File, path: PathBuf) {
        self.file = Some((path, fp));
    }

    fn is_none(&self) -> bool {
        self.file.is_none() && self.clients.is_empty()
    }
}

fn poll_clients(
    id: DataId,
    initial_timestamp: Timestamp,
    poll_fds: &mut Vec<libc::pollfd>,
    output: &mut Lz4Writer<Output>,
) {
    poll_fds.clear();

    for client in output.inner().clients.iter() {
        poll_fds.push(libc::pollfd {
            fd: client.stream.as_raw_fd(),
            events: libc::POLLIN | libc::POLLHUP,
            revents: 0,
        });
    }

    let ok = unsafe { libc::poll(poll_fds.as_ptr() as *mut _, poll_fds.len() as _, 0) };
    if ok == -1 {
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::Interrupted {
            error!("Poll failed: {}", err);
            return;
        }
    }

    for (index, poll_fd) in poll_fds.iter().enumerate() {
        let pollin = poll_fd.revents & libc::POLLIN != 0;
        let pollhup = poll_fd.revents & libc::POLLHUP != 0;

        let client = &mut output.inner_mut_without_flush().clients[index];
        if pollhup {
            info!("A client was disconnected");
            client.running = false;
            continue;
        }

        if !pollin {
            continue;
        }

        trace!("Reading a client's request...");
        let request = match Request::read_from_stream(Endianness::LittleEndian, &mut client.stream)
        {
            Ok(request) => request,
            Err(error) => {
                info!("Failed to read a client request: {}", error);
                client.running = false;
                continue;
            }
        };
        trace!("Finished reading the request from client");

        match request {
            Request::StartStreaming => {
                let output = &mut output.inner_mut().unwrap();
                let client = &mut output.clients[index];
                if let Err(error) = client.start_streaming(id, initial_timestamp, &mut output.file)
                {
                    info!("Failed to start streaming to a client: {}", error);
                    client.running = false;
                } else {
                    client.streaming = true;
                }
            }
            Request::TriggerMemoryDump => {
                debug!("Received a TriggerMemoryDump request");
                send_event(InternalEvent::GrabMemoryDump);
            }
            Request::Ping => {
                trace!("Received a Ping request");
                if let Err(error) =
                    Response::Pong.write_to_stream(Endianness::LittleEndian, &mut client.stream)
                {
                    info!("Failed to respond to a client ping: {}", error);
                    client.running = false;
                }
            }
        }
    }

    output
        .inner_mut_without_flush()
        .clients
        .retain(|client| client.running);
}

impl io::Write for Output {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if let Some((ref path, ref mut fp)) = self.file {
            if let Err(error) = fp.write_all(data) {
                warn!("Write to {:?} failed: {}", path, error);
                self.file = None;
            }
        }

        for mut client in self.clients.iter_mut() {
            if !client.running || !client.streaming {
                continue;
            }

            let result = client.write_all(data);
            if let Err(error) = result {
                client.running = false;
                warn!("Write to client failed: {}", error);
            }
        }

        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some((ref path, ref mut fp)) = self.file {
            if let Err(error) = fp.flush() {
                warn!("Flush of {:?} failed: {}", path, error);
                self.file = None;
            }
        }

        Ok(())
    }
}

impl<'a> io::Write for &'a mut Client {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let length = data.len();
        let response = Response::Data(data.into());
        response
            .write_to_stream(Endianness::LittleEndian, &mut self.stream)
            .map(|_| length)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

struct Client {
    stream: TcpStream,
    running: bool,
    streaming: bool,
}

impl Client {
    fn new(
        id: DataId,
        initial_timestamp: Timestamp,
        listener_port: u16,
        stream: TcpStream,
    ) -> io::Result<Self> {
        let mut client = Client {
            stream,
            running: true,
            streaming: false,
        };

        Response::Start(broadcast_header(id, initial_timestamp, listener_port))
            .write_to_stream(Endianness::LittleEndian, &mut client.stream)?;
        Ok(client)
    }

    fn stream_initial_data(
        &mut self,
        id: DataId,
        initial_timestamp: Timestamp,
        path: &Path,
        file: &mut File,
    ) -> io::Result<()> {
        if !opt::get().write_binaries_to_output {
            info!("Streaming the binaries which were suppressed in the original output file...");
            let mut serializer = Lz4Writer::new(&mut *self);
            writers::write_header(id, initial_timestamp, &mut serializer)?;
            writers::write_binaries(&mut serializer)?;
            serializer.flush()?;
        }

        info!("Streaming initial data...");
        file.seek(SeekFrom::Start(0))?;
        copy(file, &mut *self)?;

        Response::FinishedInitialStreaming
            .write_to_stream(Endianness::LittleEndian, &mut self.stream)?;

        if let Err(error) = remove_file(&path) {
            warn!("Failed to remove {:?}: {}", path, error);
        }

        info!("Finished streaming initial data");
        Ok(())
    }

    fn start_streaming(
        &mut self,
        id: DataId,
        initial_timestamp: Timestamp,
        output: &mut Option<(PathBuf, File)>,
    ) -> io::Result<()> {
        // First client which connects to us gets streamed all of the data
        // which we've gathered up until this point.

        if let Some((path, mut fp)) = output.take() {
            match self.stream_initial_data(id, initial_timestamp, &path, &mut fp) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    fp.seek(SeekFrom::End(0))?;
                    *output = Some((path, fp));
                    return Err(error);
                }
            }
        }

        {
            let mut serializer = Lz4Writer::new(&mut *self);
            writers::write_header(id, initial_timestamp, &mut serializer)?;
            writers::write_maps(&mut serializer)?;
            writers::write_binaries(&mut serializer)?;
            serializer.flush()?;
        }

        Response::FinishedInitialStreaming
            .write_to_stream(Endianness::LittleEndian, &mut self.stream)?;

        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        info!("Removing client...");
    }
}

fn broadcast_header(
    id: DataId,
    initial_timestamp: Timestamp,
    listener_port: u16,
) -> BroadcastHeader {
    let (timestamp, wall_clock_secs, wall_clock_nsecs) = get_wall_clock();

    BroadcastHeader {
        id,
        initial_timestamp,
        timestamp,
        wall_clock_secs,
        wall_clock_nsecs,
        pid: *PID,
        listener_port,
        cmdline: CMDLINE.clone(),
        executable: EXECUTABLE.clone(),
        arch: arch::TARGET_ARCH.to_string(),
        protocol_version: PROTOCOL_VERSION,
    }
}

fn create_listener() -> Option<TcpListener> {
    let base_port = opt::get().base_server_port;
    let mut port = base_port;

    let listener = loop {
        match TcpListener::bind(format!("0.0.0.0:{}", port)) {
            Ok(listener) => {
                info!("Created a TCP listener on port {}", port);
                break listener;
            }
            Err(error) => {
                port += 1;
                if port > base_port + 100 {
                    error!("Failed to create a TCP listener: {}", error);
                    return None;
                }
            }
        }
    };

    if let Err(error) = listener.set_nonblocking(true) {
        error!("Failed to set the TCP listener as non-blocking: {}", error);
        return None;
    }

    Some(listener)
}

fn send_broadcast_to(
    id: DataId,
    initial_timestamp: Timestamp,
    listener_port: u16,
    target: IpAddr,
) -> Result<(), io::Error> {
    let socket = UdpSocket::bind(SocketAddr::new(target, 0))?;
    socket.set_broadcast(true)?;

    let mut message = Vec::new();
    broadcast_header(id, initial_timestamp, listener_port)
        .write_to_stream(Endianness::LittleEndian, &mut message)
        .unwrap();

    socket.send_to(&message, "255.255.255.255:43512")?;
    Ok(())
}

fn send_broadcast(
    id: DataId,
    initial_timestamp: Timestamp,
    listener_port: u16,
) -> Result<(), io::Error> {
    use std::iter::once;
    use std::net::Ipv4Addr;

    let wildcard: IpAddr = Ipv4Addr::new(0, 0, 0, 0).into();
    let mut output = Ok(());
    for ip in get_local_ips().into_iter().chain(once(wildcard)) {
        let result = send_broadcast_to(id, initial_timestamp, listener_port, ip);
        if result.is_err() {
            output = result;
        }
    }

    output
}

fn initialize_output_file() -> Option<(File, PathBuf)> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let output_path = generate_filename(&opt::get().output_path_pattern, Some(&COUNTER));
    if output_path == "" {
        return None;
    }

    let fp = {
        let _handle = temporarily_change_umask(0o777);
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o777)
            .open(&output_path)
    };

    let fp = match fp {
        Ok(fp) => fp,
        Err(error) => {
            error!("Couldn't open '{}' for writing: {}", output_path, error);
            return None;
        }
    };

    // In the unlikely case of a race condition when setting the umask.
    let _ = fp.set_permissions(fs::Permissions::from_mode(0o777));

    info!("File '{}' opened for writing", output_path);
    if let Some(uid) = opt::get().chown_output_to {
        let gid = unsafe { libc::getgid() };
        let errcode = unsafe { libc::fchown(fp.as_raw_fd(), uid, gid) };
        if errcode != 0 {
            let err = io::Error::last_os_error();
            warn!("Couldn't chown '{}' to {}: {}", output_path, uid, err);
        } else {
            info!("File '{}' was chown'd to {}", output_path, uid);
        }
    }

    Some((fp, output_path.into()))
}

pub(crate) fn thread_main() {
    assert!(!get_tls().unwrap().is_enabled());

    info!("Starting event thread...");

    let uuid = generate_data_id();
    let initial_timestamp = get_timestamp();
    info!("Data ID: {}", uuid);

    let mut output_writer = Lz4Writer::new(Output::new());
    if let Some((fp, path)) = initialize_output_file() {
        let mut fp = Lz4Writer::new(fp);
        match writers::write_initial_data(uuid, initial_timestamp, &mut fp) {
            Ok(()) => {
                let fp = fp.into_inner().unwrap();

                let mut output = Output::new();
                output.set_file(fp, path);
                output_writer.replace_inner(output).unwrap();
            }
            Err(error) => {
                warn!("Failed to write initial data: {}", error);
            }
        }
    }

    let mut listener = None;

    if opt::get().enable_server {
        if let Some(listener_instance) = create_listener() {
            let listener_port = listener_instance
                .local_addr()
                .expect("couldn't grab the local address of the listener")
                .port();
            listener = Some((listener_instance, listener_port));
        }
    }

    let mut events = Vec::new();
    let mut next_backtrace_id = 0;
    let mut last_flush_timestamp = get_timestamp();
    let mut coarse_timestamp = get_timestamp();
    let mut running = true;
    let mut allocation_lock_for_memory_dump = None;
    let mut last_broadcast = coarse_timestamp;
    let mut last_server_poll = coarse_timestamp;
    let mut timestamp_override = None;
    let mut poll_fds = Vec::new();
    'main_loop: loop {
        timed_recv_all_events(&mut events, Duration::from_millis(250));

        crate::global::try_disable_if_requested();
        coarse_timestamp = get_timestamp();
        if let Some((ref mut listener, listener_port)) = listener {
            if (coarse_timestamp - last_broadcast).as_secs() >= 1 {
                last_broadcast = coarse_timestamp;
                if opt::get().enable_broadcasts {
                    let _ = send_broadcast(uuid, initial_timestamp, listener_port);
                }
            }

            if (coarse_timestamp - last_server_poll).as_msecs() >= 250 {
                last_server_poll = coarse_timestamp;
                match listener.accept() {
                    Ok((stream, _)) => {
                        match Client::new(uuid, initial_timestamp, listener_port, stream) {
                            Ok(client) => {
                                output_writer.inner_mut_without_flush().clients.push(client);
                            }
                            Err(error) => {
                                info!("Failed to initialize client: {}", error);
                            }
                        }
                    }
                    Err(ref error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => {}
                }

                poll_clients(uuid, initial_timestamp, &mut poll_fds, &mut output_writer);
            }
        }

        if events.is_empty() && !running {
            break;
        }

        if events.is_empty() {
            if let Some(_lock) = allocation_lock_for_memory_dump.take() {
                if !output_writer.inner().is_none() {
                    let _ = writer_memory::write_memory_dump(&mut output_writer);
                }
            }
        }

        let serializer = &mut output_writer;
        let skip = serializer.inner().is_none();
        for event in events.drain(..) {
            match event {
                InternalEvent::Alloc {
                    ptr,
                    size,
                    backtrace,
                    thread,
                    flags,
                    extra_usable_space,
                    preceding_free_space,
                    mut timestamp,
                    throttle,
                } => {
                    if skip {
                        continue;
                    }

                    if timestamp == Timestamp::min() {
                        timestamp = coarse_timestamp;
                    }

                    timestamp = timestamp_override.take().unwrap_or(timestamp);
                    if let Ok(backtrace) = writers::write_backtrace(
                        &mut *serializer,
                        thread,
                        backtrace,
                        &mut next_backtrace_id,
                    ) {
                        mem::drop(throttle);
                        let event = Event::Alloc {
                            timestamp,
                            allocation: AllocBody {
                                pointer: ptr as u64,
                                size: size as u64,
                                backtrace,
                                thread,
                                flags,
                                extra_usable_space,
                                preceding_free_space,
                            },
                        };
                        let _ = event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                    }
                }
                InternalEvent::Realloc {
                    old_ptr,
                    new_ptr,
                    size,
                    backtrace,
                    thread,
                    flags,
                    extra_usable_space,
                    preceding_free_space,
                    mut timestamp,
                    throttle,
                } => {
                    if skip {
                        continue;
                    }

                    if timestamp == Timestamp::min() {
                        timestamp = coarse_timestamp;
                    }

                    timestamp = timestamp_override.take().unwrap_or(timestamp);
                    if let Ok(backtrace) = writers::write_backtrace(
                        &mut *serializer,
                        thread,
                        backtrace,
                        &mut next_backtrace_id,
                    ) {
                        mem::drop(throttle);
                        let event = Event::Realloc {
                            timestamp,
                            old_pointer: old_ptr as u64,
                            allocation: AllocBody {
                                pointer: new_ptr as u64,
                                size: size as u64,
                                backtrace,
                                thread,
                                flags,
                                extra_usable_space,
                                preceding_free_space,
                            },
                        };
                        let _ = event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                    }
                }
                InternalEvent::Free {
                    ptr,
                    backtrace,
                    thread,
                    mut timestamp,
                    throttle,
                } => {
                    if skip {
                        continue;
                    }

                    if timestamp == Timestamp::min() {
                        timestamp = coarse_timestamp;
                    }

                    timestamp = timestamp_override.take().unwrap_or(timestamp);
                    if let Ok(backtrace) = writers::write_backtrace(
                        &mut *serializer,
                        thread,
                        backtrace,
                        &mut next_backtrace_id,
                    ) {
                        mem::drop(throttle);
                        let event = Event::Free {
                            timestamp,
                            pointer: ptr as u64,
                            backtrace,
                            thread,
                        };
                        let _ = event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                    }
                }
                InternalEvent::Mmap {
                    pointer,
                    length,
                    backtrace,
                    thread,
                    requested_address,
                    mmap_protection,
                    mmap_flags,
                    file_descriptor,
                    offset,
                    mut timestamp,
                    throttle,
                } => {
                    if skip {
                        continue;
                    }

                    if timestamp == Timestamp::min() {
                        timestamp = coarse_timestamp;
                    }

                    timestamp = timestamp_override.take().unwrap_or(timestamp);
                    if let Ok(backtrace) = writers::write_backtrace(
                        &mut *serializer,
                        thread,
                        backtrace,
                        &mut next_backtrace_id,
                    ) {
                        mem::drop(throttle);
                        let event = Event::MemoryMap {
                            timestamp,
                            pointer: pointer as u64,
                            length: length as u64,
                            backtrace,
                            thread,
                            requested_address: requested_address as u64,
                            mmap_protection,
                            mmap_flags,
                            file_descriptor,
                            offset,
                        };

                        let _ = event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                    }
                }
                InternalEvent::Munmap {
                    ptr,
                    len,
                    backtrace,
                    thread,
                    mut timestamp,
                    throttle,
                } => {
                    if skip {
                        continue;
                    }

                    if timestamp == Timestamp::min() {
                        timestamp = coarse_timestamp;
                    }

                    let timestamp = timestamp_override.take().unwrap_or(timestamp);
                    if let Ok(backtrace) = writers::write_backtrace(
                        &mut *serializer,
                        thread,
                        backtrace,
                        &mut next_backtrace_id,
                    ) {
                        mem::drop(throttle);
                        let event = Event::MemoryUnmap {
                            timestamp,
                            pointer: ptr as u64,
                            length: len as u64,
                            backtrace,
                            thread,
                        };
                        let _ = event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                    }
                }
                InternalEvent::Mallopt {
                    param,
                    value,
                    result,
                    mut timestamp,
                    backtrace,
                    thread,
                    throttle,
                } => {
                    if skip {
                        continue;
                    }

                    if timestamp == Timestamp::min() {
                        timestamp = coarse_timestamp;
                    }

                    let timestamp = timestamp_override.take().unwrap_or(timestamp);
                    if let Ok(backtrace) = writers::write_backtrace(
                        &mut *serializer,
                        thread,
                        backtrace,
                        &mut next_backtrace_id,
                    ) {
                        mem::drop(throttle);
                        let event = Event::Mallopt {
                            timestamp,
                            param,
                            value,
                            result,
                            backtrace,
                            thread,
                        };
                        let _ = event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                    }
                }
                InternalEvent::Exit => {
                    running = false;
                }
                InternalEvent::GrabMemoryDump => {
                    // Block any further allocations.
                    if allocation_lock_for_memory_dump.is_none() {
                        debug!("Locking allocations to prepare for a memory dump");
                        allocation_lock_for_memory_dump = Some(AllocationLock::new());
                    }
                }
                InternalEvent::SetMarker { value } => {
                    if skip {
                        continue;
                    }

                    let event = Event::Marker { value };
                    let _ = event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                }
                InternalEvent::OverrideNextTimestamp { timestamp } => {
                    timestamp_override = Some(timestamp);
                }
                InternalEvent::AddressSpaceUpdated { maps, new_binaries } => {
                    let timestamp = get_timestamp();
                    if opt::get().write_binaries_to_output
                        || serializer.inner_mut_without_flush().file.is_none()
                    {
                        for binary in new_binaries {
                            debug!("Writing new binary: {}", binary.name());
                            let event = Event::File {
                                timestamp,
                                path: binary.name().into(),
                                contents: binary.as_bytes().into(),
                            };

                            let _ =
                                event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                        }
                    }

                    debug!("Writing new maps...");
                    let event = Event::File {
                        timestamp,
                        path: "/proc/self/maps".into(),
                        contents: maps.as_bytes().into(),
                    };

                    let _ = event.write_to_stream(Endianness::LittleEndian, &mut *serializer);
                }
            }
        }

        if (coarse_timestamp - last_flush_timestamp).as_secs() > 30 {
            last_flush_timestamp = get_timestamp();
            let _ = serializer.flush();
        }
    }

    let _ = output_writer.flush();
    for client in &mut output_writer.inner_mut_without_flush().clients {
        let _ = Response::Finished.write_to_stream(Endianness::LittleEndian, &mut client.stream);
        let _ = client.stream.flush();
    }

    info!("Event thread finished");
}
