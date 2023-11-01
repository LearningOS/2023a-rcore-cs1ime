//! Process management syscalls
//!
use core::mem::transmute;

use alloc::sync::Arc;

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE},
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str, VirtAddr, MapPermission, translated_byte_buffer},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus,
    }, timer::{get_time_ms, get_time_us},
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

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
    // println!("1");
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let usec = get_time_us()%1_000_000;
    let sec = get_time_us()/1_000_000;

    let tv = TimeVal {
        sec,usec
    };
    let byte_tv: &[u8]=unsafe{
        let slice = core::slice::from_raw_parts(&tv as *const _ as *const u8,
            core::mem::size_of::<TimeVal>());
        transmute(slice)
    };
    let buffers = translated_byte_buffer(current_user_token(), _ts as *mut u8, 
        core::mem::size_of::<TimeVal>());
    let mut idx = 0;
    for bs in buffers
    {
        bs.copy_from_slice(&byte_tv[idx..(idx+bs.len())]);
        idx+=bs.len();
    }

    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    let task = current_task().unwrap();
    let current = 
        task.inner_exclusive_access();
    let status = current.task_status;
    let time = get_time_ms() - current.start_time;
    let syscall_times: [u32; 500] = current.sys_times;

    let ti: TaskInfo = TaskInfo {
        status,time,syscall_times
    };

    let byte_ti: &[u8]=unsafe{
        let slice = core::slice::from_raw_parts(&ti as *const _ as *const u8,
            core::mem::size_of::<TaskInfo>());
        transmute(slice)
    };

    let buffers = translated_byte_buffer(current_user_token(), _ti as *mut u8, 
        core::mem::size_of::<TaskInfo>());
    let mut idx = 0;

    for bs in buffers
    {
        bs.copy_from_slice(&byte_ti[idx..(idx+bs.len())]);
        idx+=bs.len();
    }
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    let task = current_task().unwrap();
    let mut current = 
        task.inner_exclusive_access();
    let memory_set = &mut current.memory_set;

    if _start & (PAGE_SIZE-1) != 0
    {
        return -1;
    }
    if _port == 0 || (_port & !(7 as usize) != 0)
    {
        return -1;
    }
    if memory_set.is_conflit_range(VirtAddr(_start), VirtAddr(_start+_len))
    {
        return -1;
    }

    if (_port & 1) == 0
    {
        return 0;
    }
    let mut perm  = MapPermission::R;
    if (_port & 2) != 0
    {
        perm |= MapPermission::W;
    }
    if (_port & 4) != 0
    {
        perm |= MapPermission::X;
    }

    perm |= MapPermission::U;

    memory_set.insert_framed_area(VirtAddr(_start), VirtAddr(_start+_len), perm);
    
    
    0
}
/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    if _start & (PAGE_SIZE-1) != 0
    {
        return -1;
    }

    let task = current_task().unwrap();
    let mut current = 
        task.inner_exclusive_access();
    let memory_set = &mut current.memory_set;

    memory_set.unmap_area(VirtAddr(_start), VirtAddr(_start+_len))
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current = current_task().unwrap();
    let new_task = current.fork();
    let new_pid = new_task.pid.0;
    
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    trap_cx.x[10] = 0;
   
    {
        let token = current_user_token();
        let path = translated_str(token, _path);
        if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
            let all_data = app_inode.read_all();
            new_task.exec(all_data.as_slice());
        }
    }

    add_task(new_task);

    new_pid as isize
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    if _prio <= 1
    {
        return -1;
    }
    let task = current_task().unwrap();
    let mut current = 
        task.inner_exclusive_access();
    current.priority=_prio;
    
    _prio
}
