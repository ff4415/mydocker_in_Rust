use std::path::{Path, PathBuf};
use std::fs::File;
use std::fs::{create_dir_all, remove_dir_all};
use std::ffi::OsStr;
use std::ffi::CString;
use std::env::{vars_os, current_dir};
use std::os::unix::fs::{PermissionsExt};
use std::os::unix::process::CommandExt;
use std::os::unix::io::FromRawFd;
use std::collections::HashMap;
use libc::chdir;
use libc::{pid_t, c_ulong, c_int};
use unshare::{Command, Stdio, Fd, Child, PipeWriter};

static ref FRIENDLY_STYLE: unshare::Style = unshare::Style::short();
static ref DEBUG_STYLE: unshare::Style = unshare::Style::debug();
static MS_NOSUID: c_ulong = 2;                /* Ignore suid and sgid bits.  */
static MS_NODEV: c_ulong = 4;                 /* Disallow access to device special files.  */
static MS_NOEXEC: c_ulong = 8;                /* Disallow program execution.  */
static MS_BIND: c_ulong = 4096;               /* Bind directory at different place.  */
static MS_REC: c_ulong = 16384;
static MS_STRICTATIME: c_ulong = 1 << 24;     /* Always perform atime updates.  */
static MNT_DETACH: c_int = 2;          /* Just detach from the tree.  */

extern {
    fn mount(source: *const u8, target: *const u8,
            filesystemtype: *const u8, flags: c_ulong,
            data: *const u8) -> c_int;
    fn umount(target: *const u8) -> c_int;
    fn pivot_root(new_root: *const c_char, put_old: *const c_char) -> c_int;
}

pub fn cmd_show(cmd: &Command) -> unshare::Printer {
    cmd.display(&FRIENDLY_STYLE)
}

pub fn cmd_debug(cmd: &Command) -> unshare::Printer {
    cmd.display(&DEBUG_STYLE)
}

pub fn cmd_err<E: fmt::Display>(cmd: &Command, err: E) -> String {
   format!("Error running {}: {}", cmd_debug(cmd), err)
}

pub struct container_info {
    pid: pid_t,
    id: u32,
    name: String,
    command: String,
    create_time: String,
    status: String,
    volume: String,
    port_mapping: Vec<u32>
}

pub fn new_parent_process(tty: bool,container_name: &str,
                            volume: &str, image_name: &str,
                            env_slice: &HashMap<OsStr, OsStr>) -> Result<(&Command, PipeWriter), String> {

    let mut cmd = Command::new("/proc/self/exe");
    cmd.arg("init");
    cmd.env_clear();
    for (k,v) in env::vars_os() {
        cmd.env(k, v);
    }
    for (k,v) in env_slice {
        cmd.env(k, v);
    }
    cmd.unshare(&[Namespace::Mount, Namespace::Ipc, Namespace::Pid, Namespace::Net, Namespace::Uts]);

    if !tty {
        let dir_url = Path::new(format!("/var/run/mydocker/{}/", container_name));
        set_permissions(&dir_url, Permissions::from_mode(0o622)).map_err(|e| format!("Error setting permissions: {}", e));
        let f = File::create(dir_url.join("container.log"))?.unwrap();
        cmd.stdout(Stdio::from_file(f));
    }
    cmd.file_descriptor(3, Fd::piped_write());
    info!("Running {}", cmd_show(&cmd));
    new_work_space(volume, image_name, container_name);
    let mut child = cmd.spawn().map_err(|e| format!("Command {}: {}", cmd_debug(&cmd), e))?;
    Ok((&cmd, child.take_pipe_writer(3).upwrap()))
}

fn new_work_space(volume: &str, image_name: &str, container_name: &str) {
    create_readonly_layer(image_name);
    create_write_layer(container_name);
    create_mount_point(container_name, image_name);
    if volume != "" {
        let volume_urls: Vec<&str> = volume.split(":").collect();
        if volume_urls.len() == 2 && volume_urls[0] != "" && volume_urls[1] != "" {
            mount_volume(volume_urls, container_name);
            info!("NewWorkSpace volume urls {}", volume_urls);
        } else {
            info!("Volume parameter input is not correct.");
        }
    }
}

fn create_readonly_layer(image_name: &str) -> Result<(), String> {
    let untar_folder = format!("/root/{}", image_name);
    let untar_folder_path = Path::new(untar_folder);
    set_permissions(&untar_folder_path, Permissions::from_mode(0o622)).map_err(|e| format!("Error setting permissions: {}", e))?;
    if !untar_folder_path.exists() {
        create_dir_all(&untar_folder_path)?;
        let image_url = format!("/root/{}.tar", image_name);
        let mut cmd = Command::new("/proc/self/exe");
        match cmd.args(&["tar", "-xvf", image_url, "-C", untar_folder]).status() {
            Ok(ref st) if st.success() => Ok(()),
            Ok(status) => Err(cmd_err(&cmd, status)),
            Err(err) => Err(cmd_err(&cmd, err)),
        }
    }
}

fn create_write_layer(container_name &str) {
    let write_url = Path::new(format!("/root/writeLayer/{}", container_name));
    set_permissions(&write_url, Permissions::from_mode(0o777)).map_err(|e| format!("Error setting permissions: {}", e))?;
    create_dir_all(&write_url)?;
}

fn mount_volume(volume_urls: Vec<String>, container_name: String) -> Result<(), String> {
    let parent_path = Path::new(&volume_urls[0]);
    set_permissions(&parent_path, Permissions::from_mode(0o777)).map_err(|e| format!("Error setting permissions: {}", e))?;
    create_dir_all(&parent_path)?;

    let mnt_url = format!("/root/mnt/{}", container_name);
    let container_volume_url = format!("{}/{}",mnt_url,volume_urls[1]);
    let container_volume_path = Path::new(&container_volume_url);
    set_permissions(&container_volume_path, Permissions::from_mode(0o777)).map_err(|e| format!("Error setting permissions: {}", e))?;
    create_dir_all(&container_volume_path)?;

    let dirs = format!("dirs={}", volume_urls[0]);
    let mut cmd = Command::new("/proc/self/exe");
    match cmd.args(&["mount", "-t", "aufs", "-o", dirs, "none", container_volume_url]).status() {
        Ok(ref st) if st.success() => Ok(()),
        Ok(status) => Err(cmd_err(&cmd, status)),
        Err(err) => Err(cmd_err(&cmd, err)),
    }
}

fn create_mount_point(container_name: String, image_name: String) -> Result<(), String> {
    let mnt_url = format!("/root/mnt/{}", container_name);
    let mnt_path = Path::new(mnt_url);
    set_permissions(&mnt_path, Permissions::from_mode(0o777)).map_err(|e| format!("Error setting permissions: {}", e))?;
    create_dir_all(&mnt_path)?;

    let tmp_write_layer = format!("/root/writeLayer/{}", container_name);
    let tmp_image_location = format!("/root/{}", image_name);
    let dirs = format!("dirs={}:{}", tmp_write_layer, tmp_image_location);
    let mut cmd = Command::new("/proc/self/exe");
    match cmd.args(&["mount", "-t", "aufs", "-o", dirs, "none", mnt_url]).status() {
        Ok(ref st) if st.success() => Ok(()),
        Ok(status) => Err(cmd_err(&cmd, status)),
        Err(err) => Err(cmd_err(&cmd, err)),
    }
}

fn delete_work_space(volume: String, container_name: String) {
    if volume != "" {
        let volume_urls: Vec<&str> = volume.split(":").collect();
        if volume_urls.len() == 2 && volume_urls[0] != "" && volume_urls[1] != "" {
            delete_volume(volume_urls, container_name);
        }
    }
    delete_mount_point(container_name);
    delete_write_layer(container_name);
}

fn delete_mount_point(container_name: String) -> Result<(), String> {
    let mnt_url = format!("/root/mnt/{}", container_name);
    let mut cmd = Command::new("/proc/self/exe");
    cmd.args(&["unmount", mnt_url]).status()?
    remove_dir_all(mnt_url)?
    Ok(())
}

fn delete_volume(volume_urls: &str, container_name: &str) -> Result<(), String> {
    let mnt_url = format!("/root/mnt/{}", container_name);
    let container_url = format!("{}/{}", mnt_url, volume_urls[1]);
    let mut cmd = Command::new("/proc/self/exe");
    cmd.args(&["unmount", container_url]).status()?
    Ok(())
}

fn delete_write_layer(container_name: &str) {
    let write_url = Path::new(format!("/root/writeLayer/{}", container_name));
    if let Err(err) = remove_dir_all(mnt_url) {
        info!("Remove writeLayer dir {} error {}", write_url, err);
    }
}

pub fn run_container_init_process() -> Result<(), String> {
    let cmd_array: Vec<&str> = read_user_command();
    setup_mount();
    if let Some(cmdpath) = env_path_find(cmd_array[0]) {
        let mut cmd = Command::new(cmdpath);
        let err = cmd.args(&cmd_array[1..]).exec();
        if err {
            Err("failed in exec {}", err);
        }
    }
    Ok(())
}

fn read_user_command() -> Vec<&str> {
    let mut buf = vec!();
    let mut fd3 = unsafe { File::from_raw_fd(3) };
    match fd3.read_to_end(&mut buf) {
        Ok(_) => return String::from_utf8(buf).unwrap().split(" ").collect(),
        Err(e) => {
            error!("Error reading from fd 3: {}", e);
            exit(1);
        }
    }
}

fn setup_mount() {
    match env::current_dir() {
        Ok(pwd) => root_pivot(pwd),
        Err(e) => {
            error!("Get current location error {}", e);
            return
        }
    }

    unsafe { mount( "proc".to_cstring().as_bytes().as_ptr(),
                    "/proc".to_cstring().as_bytes().as_ptr(),
                    "proc".to_cstring().as_bytes().as_ptr(), MS_NOEXEC | MS_NOSUID | MS_NODEV, null()) };

    unsafe { mount( "tmpfs".to_cstring().as_bytes().as_ptr(),
                    "/dev".to_cstring().as_bytes().as_ptr(),
                    "tmpfs".to_cstring().as_bytes().as_ptr(), MS_NOSUID | MS_STRICTATIME, "mode=755".to_cstring().as_bytes().as_ptr()) };
}

fn root_pivot(root &PathBuf) {

    let rc = unsafe { mount( root.to_cstring().as_bytes().as_ptr(),
                root.to_cstring().as_bytes().as_ptr(),
                "bind".to_cstring().as_bytes().as_ptr(), MS_BIND | MS_REC, null()) };
    if rc != 0 {
        let err = IoError::last_os_error();
        error!("Can't mount {:?} : {}", root, err);
        return
    }

    let mut pivot_dir: PathBuf = root.join("./pivot_root").unwrap();
    set_permissions(&pivot_dir, Permissions::from_mode(0o777)).map_err(|e| format!("Error setting permissions: {}", e))?;

    if let Err(e) = create_dir_all(&pivot_dir) {
        error!("create_dir error {}", e);
        return
    }

    if unsafe { pivot_root(root.to_cstring().as_ptr(), pivot_dir.to_cstring().as_ptr()) } != 0 {
        error!("Error pivot_root to {:?}: {}", new_root, IoError::last_os_error());
        return
    }

    let c_root = CString::new("/").unwrap();
    if unsafe { chdir(c_root.as_ptr()) } != 0 {
        error!("Error chdir to root: {}", IoError::last_os_error());
        return
    }

    pivot_dir = Path::new("/").join(".pivot_root");
    if unsafe { umount2(pivot_dir.to_cstring().as_bytes().as_ptr(), MNT_DETACH) } != 0 {
        let err = IoError::last_os_error();
        error!("Can't unmount {:?} : {}", pivot_dir, err);
        return
    };
    if let Err(err) = remove_dir_all(pivot_dir) {
        error!("Can't remove_dir {:?} : {}", pivot_dir, err);
        return
    }
}

fn env_path_find<P: AsRef<Path>>(cmd: P) -> Option<PathBuf> {
    env::var("PATH").map(|v| path_find(&cmd, &v[..]))
        .unwrap_or_else(|_| path_find(&cmd, DEFAULT_PATH))
}

fn path_find<P: AsRef<Path>>(cmd: P, path: &str) -> Option<PathBuf> {
    let cmd = cmd.as_ref();
    trace!("Path search {:?} in {:?}", cmd, path);
    if cmd.is_absolute() {
        return Some(cmd.to_path_buf())
    }
    for prefix in path.split(":") {
        let tmppath = PathBuf::from(prefix).join(cmd);
        if tmppath.exists() {
            trace!("Path resolved {:?} is {:?}", cmd, tmppath);
            return Some(tmppath);
        }
    }
    None
}
