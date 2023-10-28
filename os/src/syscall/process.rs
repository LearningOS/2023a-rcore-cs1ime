//! Process management syscalls
use core::mem::transmute;

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    task::{
        change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus, get_current_syscall_times, get_current_running_time, get_current_status, current_user_token, current_do_mmap, current_do_munmap,
    }, mm::translated_byte_buffer,
    timer::{get_time_us, get_time_ms},
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

// struct SafeAccessor
// {
//     token: usize,
//     ptr: *const u8,
//     len: usize,
//     slices: Vec<&'static mut [u8]>
// }
// impl SafeAccessor
// {
//     pub fn SafeAccessor(&mut self,_token: usize, _ptr: *const u8, _len: usize)
//     {
//         self.token=_token;
//         self.ptr=_ptr;
//         self.len=_len;
//         self.slices = translated_byte_buffer(self.token, self.ptr, self.len);
//     }
//     pub fn 

// }

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let sec = get_time_ms()/1_000;
    let usec = us % 1_000_000;

    let tv = TimeVal
    {
        sec,usec
    };

    let byte_tv: &[u8]= unsafe {
        let slice = core::slice::from_raw_parts(&tv as *const _ as *const u8,core::mem::size_of::<TimeVal>());
        transmute(slice)
    };

    let buffers = translated_byte_buffer(current_user_token(), _ts as *mut u8, core::mem::size_of::<TimeVal>());
    let mut idx = 0;
    for bs in buffers
    {
        bs.copy_from_slice(&byte_tv[idx..(idx+bs.len())]);
        idx+=bs.len();
    }


    trace!("kernel: sys_get_time");
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    
    let syscall_times = get_current_syscall_times();
    let time = get_current_running_time();
    let status = get_current_status();

    let kti = TaskInfo
    {
        syscall_times,
        time,
        status
    };
    let byte_ti: &[u8]= unsafe {
        let slice = core::slice::from_raw_parts(&kti as *const _ as *const u8,core::mem::size_of::<TaskInfo>());
        transmute(slice)
    };

    let buffers = translated_byte_buffer(current_user_token(), _ti as *mut u8, core::mem::size_of::<TaskInfo>());
    let mut idx = 0;
    for bs in buffers
    {
        bs.copy_from_slice(&byte_ti[idx..(idx+bs.len())]);
        idx+=bs.len();
    }
    
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");
    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    
    current_do_mmap(_start,_len,_port)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    if _start & (PAGE_SIZE-1) != 0
        {
            return -1;
        }
    current_do_munmap(_start, _len)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
