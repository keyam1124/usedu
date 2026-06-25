use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct BulkEntry {
    pub path: PathBuf,
    pub name: OsString,
    pub kind: BulkEntryKind,
    pub used_bytes: u64,
    pub error: Option<io::Error>,
}

#[derive(Debug)]
pub struct BulkAggregate {
    pub entries_seen: u64,
    pub used_bytes: u64,
    pub file_count: u64,
    pub dir_children: Vec<PathBuf>,
    pub errors: Vec<BulkError>,
}

#[derive(Debug)]
pub struct BulkError {
    pub path: PathBuf,
    pub error: io::Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkEntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

pub fn read_dir_fast(path: &Path) -> io::Result<Option<Vec<BulkEntry>>> {
    imp::read_dir_fast(path)
}

pub fn read_dir_fast_aggregate(path: &Path) -> io::Result<Option<BulkAggregate>> {
    imp::read_dir_fast_aggregate(path)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{BulkAggregate, BulkEntry, BulkEntryKind, BulkError};
    use std::cell::RefCell;
    use std::ffi::CString;
    use std::ffi::OsString;
    use std::fs::File;
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    const BUFFER_SIZE: usize = 256 * 1024;
    const ATTR_CMN_ERROR: libc::attrgroup_t = 0x20000000;
    const VREG: u32 = 1;
    const VDIR: u32 = 2;
    const VLNK: u32 = 5;

    thread_local! {
        static BULK_BUFFER: RefCell<Vec<u8>> = RefCell::new(vec![0_u8; BUFFER_SIZE]);
    }

    pub fn read_dir_fast(path: &Path) -> io::Result<Option<Vec<BulkEntry>>> {
        let file = open_directory(path)?;
        let mut attr_list = fast_attr_list();
        let mut entries = Vec::new();

        BULK_BUFFER.with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            loop {
                let count = read_bulk(file.as_raw_fd(), &mut attr_list, &mut buffer)?;
                if count == 0 {
                    break;
                }

                let parsed = parse_entries(path, &buffer, count as usize)?;
                entries.extend(parsed);
            }

            Ok(Some(entries))
        })
    }

    pub fn read_dir_fast_aggregate(path: &Path) -> io::Result<Option<BulkAggregate>> {
        let file = open_directory(path)?;
        let mut attr_list = fast_attr_list();
        let mut aggregate = BulkAggregate {
            entries_seen: 0,
            used_bytes: 0,
            file_count: 0,
            dir_children: Vec::new(),
            errors: Vec::new(),
        };

        BULK_BUFFER.with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            loop {
                let count = read_bulk(file.as_raw_fd(), &mut attr_list, &mut buffer)?;
                if count == 0 {
                    break;
                }

                parse_entries_into_aggregate(path, &buffer, count as usize, &mut aggregate)?;
            }

            Ok(Some(aggregate))
        })
    }

    fn open_directory(path: &Path) -> io::Result<File> {
        let c_path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL byte"))?;
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW;
        // SAFETY: `c_path` is a valid NUL-terminated path and `flags` request
        // a read-only directory descriptor for immediate ownership by `File`.
        let fd = unsafe { libc::open(c_path.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` is a fresh descriptor returned by `open`; `File` takes
        // ownership and will close it exactly once.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn fast_attr_list() -> libc::attrlist {
        let commonattr = libc::ATTR_CMN_RETURNED_ATTRS
            | libc::ATTR_CMN_NAME
            | ATTR_CMN_ERROR
            | libc::ATTR_CMN_OBJTYPE;
        libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr,
            volattr: 0,
            dirattr: 0,
            fileattr: libc::ATTR_FILE_ALLOCSIZE,
            forkattr: 0,
        }
    }

    fn read_bulk(
        dir_fd: libc::c_int,
        attr_list: &mut libc::attrlist,
        buffer: &mut Vec<u8>,
    ) -> io::Result<libc::c_int> {
        // SAFETY: `dir_fd` is an open directory descriptor, and `attr_list` and
        // `buffer` are valid writable objects for the duration of the call.
        let count = unsafe {
            libc::getattrlistbulk(
                dir_fd,
                attr_list as *mut _ as *mut libc::c_void,
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
                u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
            )
        };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(count)
    }

    fn parse_entries(parent: &Path, buffer: &[u8], count: usize) -> io::Result<Vec<BulkEntry>> {
        let mut entries = Vec::with_capacity(count);
        let mut offset = 0_usize;
        for _ in 0..count {
            let start = offset;
            let length = read_u32(buffer, &mut offset, buffer.len())? as usize;
            let end = start.checked_add(length).ok_or_else(parse_error)?;
            if length < 4 || end > buffer.len() {
                return Err(parse_error());
            }

            let returned_common = read_u32(buffer, &mut offset, end)?;
            let _returned_vol = read_u32(buffer, &mut offset, end)?;
            let _returned_dir = read_u32(buffer, &mut offset, end)?;
            let returned_file = read_u32(buffer, &mut offset, end)?;
            let _returned_fork = read_u32(buffer, &mut offset, end)?;

            let error_code = if returned_common & ATTR_CMN_ERROR != 0 {
                read_u32(buffer, &mut offset, end)?
            } else {
                0
            };

            let name_ref_offset = offset;
            let name_data_offset = read_i32(buffer, &mut offset, end)?;
            let name_length = read_u32(buffer, &mut offset, end)? as usize;
            let obj_type = if returned_common & libc::ATTR_CMN_OBJTYPE != 0 {
                read_u32(buffer, &mut offset, end)?
            } else {
                0
            };
            let used_bytes = if returned_file & libc::ATTR_FILE_ALLOCSIZE != 0 {
                read_i64(buffer, &mut offset, end)?.max(0) as u64
            } else {
                0
            };

            let name = read_name(buffer, name_ref_offset, name_data_offset, name_length, end)?;
            let path = parent.join(Path::new(&name));
            entries.push(BulkEntry {
                path,
                name,
                kind: bulk_kind(obj_type),
                used_bytes,
                error: (error_code != 0).then(|| io::Error::from_raw_os_error(error_code as i32)),
            });

            offset = end;
        }
        Ok(entries)
    }

    fn parse_entries_into_aggregate(
        parent: &Path,
        buffer: &[u8],
        count: usize,
        aggregate: &mut BulkAggregate,
    ) -> io::Result<()> {
        let mut offset = 0_usize;
        for _ in 0..count {
            let start = offset;
            let length = read_u32(buffer, &mut offset, buffer.len())? as usize;
            let end = start.checked_add(length).ok_or_else(parse_error)?;
            if length < 4 || end > buffer.len() {
                return Err(parse_error());
            }

            let returned_common = read_u32(buffer, &mut offset, end)?;
            let _returned_vol = read_u32(buffer, &mut offset, end)?;
            let _returned_dir = read_u32(buffer, &mut offset, end)?;
            let returned_file = read_u32(buffer, &mut offset, end)?;
            let _returned_fork = read_u32(buffer, &mut offset, end)?;

            let error_code = if returned_common & ATTR_CMN_ERROR != 0 {
                read_u32(buffer, &mut offset, end)?
            } else {
                0
            };

            let name_ref_offset = offset;
            let name_data_offset = read_i32(buffer, &mut offset, end)?;
            let name_length = read_u32(buffer, &mut offset, end)? as usize;
            let obj_type = if returned_common & libc::ATTR_CMN_OBJTYPE != 0 {
                read_u32(buffer, &mut offset, end)?
            } else {
                0
            };
            let used_bytes = if returned_file & libc::ATTR_FILE_ALLOCSIZE != 0 {
                read_i64(buffer, &mut offset, end)?.max(0) as u64
            } else {
                0
            };

            if error_code != 0 {
                let path = read_entry_path(
                    parent,
                    buffer,
                    name_ref_offset,
                    name_data_offset,
                    name_length,
                    end,
                )
                .unwrap_or_else(|_| parent.to_path_buf());
                aggregate.errors.push(BulkError {
                    path,
                    error: io::Error::from_raw_os_error(error_code as i32),
                });
            } else if obj_type == VDIR {
                aggregate.dir_children.push(read_entry_path(
                    parent,
                    buffer,
                    name_ref_offset,
                    name_data_offset,
                    name_length,
                    end,
                )?);
                aggregate.entries_seen = aggregate.entries_seen.saturating_add(1);
            } else {
                aggregate.file_count = aggregate.file_count.saturating_add(1);
                aggregate.used_bytes = aggregate.used_bytes.saturating_add(used_bytes);
                aggregate.entries_seen = aggregate.entries_seen.saturating_add(1);
            }

            offset = end;
        }
        Ok(())
    }

    fn read_u32(buffer: &[u8], offset: &mut usize, end: usize) -> io::Result<u32> {
        let value = read_array::<4>(buffer, *offset, end)?;
        *offset += 4;
        Ok(u32::from_ne_bytes(value))
    }

    fn read_i32(buffer: &[u8], offset: &mut usize, end: usize) -> io::Result<i32> {
        let value = read_array::<4>(buffer, *offset, end)?;
        *offset += 4;
        Ok(i32::from_ne_bytes(value))
    }

    fn read_i64(buffer: &[u8], offset: &mut usize, end: usize) -> io::Result<i64> {
        let value = read_array::<8>(buffer, *offset, end)?;
        *offset += 8;
        Ok(i64::from_ne_bytes(value))
    }

    fn read_array<const N: usize>(buffer: &[u8], offset: usize, end: usize) -> io::Result<[u8; N]> {
        let next = offset.checked_add(N).ok_or_else(parse_error)?;
        if next > end || next > buffer.len() {
            return Err(parse_error());
        }
        buffer[offset..next].try_into().map_err(|_| parse_error())
    }

    fn read_name(
        buffer: &[u8],
        ref_offset: usize,
        data_offset: i32,
        length: usize,
        entry_end: usize,
    ) -> io::Result<OsString> {
        if data_offset < 0 {
            return Err(parse_error());
        }
        let start = ref_offset
            .checked_add(data_offset as usize)
            .ok_or_else(parse_error)?;
        let end = start.checked_add(length).ok_or_else(parse_error)?;
        if start > entry_end || end > entry_end || end > buffer.len() {
            return Err(parse_error());
        }

        let mut bytes = buffer[start..end].to_vec();
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        Ok(OsString::from_vec(bytes))
    }

    fn read_entry_path(
        parent: &Path,
        buffer: &[u8],
        ref_offset: usize,
        data_offset: i32,
        length: usize,
        entry_end: usize,
    ) -> io::Result<PathBuf> {
        let name = read_name(buffer, ref_offset, data_offset, length, entry_end)?;
        Ok(parent.join(Path::new(&name)))
    }

    fn bulk_kind(obj_type: u32) -> BulkEntryKind {
        match obj_type {
            VDIR => BulkEntryKind::Dir,
            VREG => BulkEntryKind::File,
            VLNK => BulkEntryKind::Symlink,
            _ => BulkEntryKind::Other,
        }
    }

    fn parse_error() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, "invalid getattrlistbulk buffer")
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{BulkAggregate, BulkEntry};
    use std::io;
    use std::path::Path;

    pub fn read_dir_fast(_path: &Path) -> io::Result<Option<Vec<BulkEntry>>> {
        Ok(None)
    }

    pub fn read_dir_fast_aggregate(_path: &Path) -> io::Result<Option<BulkAggregate>> {
        Ok(None)
    }
}
