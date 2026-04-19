use libc;

pub fn is_pid_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        true
    } else if let Some(errno) = std::io::Error::last_os_error().raw_os_error() {
        errno != libc::ESRCH
    } else {
        false
    }
}
