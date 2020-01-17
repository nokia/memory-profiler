use std::cmp::min;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ptr;

use nwind::proc_maps::parse as parse_maps;

use common::event::Event;
use common::speedy::{Endianness, Writable};

use crate::syscall;
use crate::writers::write_maps;
use crate::PAGE_SIZE;

fn is_accessible<U: Read + Seek>(mut fp: U, address: u64) -> bool {
    if let Err(_) = fp.seek(SeekFrom::Start(address)) {
        return false;
    }

    let mut dummy: [u8; 1] = [0];
    match fp.read(&mut dummy) {
        Ok(1) => true,
        _ => false,
    }
}

fn memory_dump_body<U: Write>(mut serializer: &mut U) -> io::Result<()> {
    let mut buffer = Vec::new();
    buffer.resize(1024 * 128, 0);
    let mut buffer = buffer.into_boxed_slice();
    let maps = write_maps(serializer)?;
    let maps = String::from_utf8_lossy(&maps);
    let maps = parse_maps(&maps);
    let mut fp = File::open("/proc/self/mem")?;
    let page_size = PAGE_SIZE as u64;

    for region in maps {
        if !region.is_write && region.inode != 0 {
            continue;
        }

        let mut end = {
            let total_length = (region.end - region.start) / page_size;

            let mut start = 0;
            let mut end = total_length;
            loop {
                if start == end {
                    break;
                }

                let current = start + (end - start) / 2;
                let accessible =
                    is_accessible(&mut fp, region.start + current * page_size + page_size - 1);
                if !accessible {
                    end = current;
                } else {
                    start = current + 1;
                }
            }

            region.start + end * page_size
        };

        loop {
            let chunk_size = min(buffer.len() as u64, end - region.start);
            if chunk_size == 0 {
                break;
            }

            let address = end - chunk_size;
            fp.seek(SeekFrom::Start(address))?;
            fp.read_exact(&mut buffer[0..chunk_size as usize])?;
            let data = &buffer[0..chunk_size as usize];
            Event::MemoryDump {
                address,
                length: chunk_size as u64,
                data: data.into(),
            }
            .write_to_stream(Endianness::LittleEndian, &mut serializer)?;

            end -= chunk_size;
        }

        /*
                let mut page: [u8; 4096] = [0; 4096];
                while address > region.start {
                    fp.seek( SeekFrom::Start( address - page_size ) )?;
                    fp.read_exact( &mut page )?;

                    if page.iter().all( |&byte| byte == 0 ) {
                        address -= page_size;
                    } else {
                        break;
                    }
                }

                fp.seek( SeekFrom::Start( region.start ) )?;
                let mut current = region.start;

                while current < address {
                    let chunk_size = min( buffer.len(), (address - current) as usize );
                    fp.read_exact( &mut buffer[ 0..chunk_size ] )?;
                    let data = &buffer[ 0..chunk_size ];
                    Event::MemoryDump {
                        address: current,
                        length: chunk_size as u64,
                        data
                    }.write_to_stream( LittleEndian, serializer )?;
                    current += chunk_size as u64;
                }
        */
    }

    serializer.flush()?;
    Ok(())
}

pub fn write_memory_dump<U: Write>(serializer: &mut U) -> io::Result<()> {
    info!("Writing a memory dump...");
    serializer.flush()?;

    let pid = unsafe { libc::fork() };
    if pid == 0 {
        let result = memory_dump_body(serializer);
        syscall::exit(if result.is_err() { 1 } else { 0 });
    } else {
        info!("Waiting for child to finish...");
        unsafe {
            libc::waitpid(pid, ptr::null_mut(), 0);
        }
    }

    info!("Memory dump finished");
    Ok(())
}
