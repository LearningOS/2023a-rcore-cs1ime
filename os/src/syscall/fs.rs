//! File and filesystem-related syscalls
use core::mem::transmute;

use crate::fs::{create_link,open_file, OpenFlags, Stat, delete_link};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if _fd >= inner.fd_table.len() {
        return -1;
    }
    
    if let Some(file) = &inner.fd_table[_fd] {
        let file = file.clone();
        println!("fetching...");
        let nlink = file.nlinks();
        let ino = file.get_ino() as u64;
        let mode = file.get_mode();
        let dev = 0 as u64;
        println!("fetched");
        // let pad :[u64;7]=[0;7];
        let st = Stat::new (
            dev,
            ino,
            mode,
            nlink,
        );

        println!("trans");
        let byte_st: &[u8]=unsafe{
            let slice = core::slice::from_raw_parts(&st as *const _ as *const u8,
                core::mem::size_of::<Stat>());
            transmute(slice)
        };

        let buffers = translated_byte_buffer(token, _st as *mut u8, 
        core::mem::size_of::<Stat>());

        println!("writing...");
        let mut idx = 0;
        for bs in buffers
        {
            bs.copy_from_slice(&byte_st[idx..(idx+bs.len())]);
            idx+=bs.len();
        }

        println!("return!");

        // drop(inner);

        return 0;
    }
    return -1;

}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    // let task = current_task().unwrap();
    let token = current_user_token();
    let old = translated_str(token, _old_name);
    let new = translated_str(token, _new_name);

    
    create_link(&old,&new)
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {

    let token = current_user_token();
    let name = translated_str(token, _name);

    delete_link(&name)
}
