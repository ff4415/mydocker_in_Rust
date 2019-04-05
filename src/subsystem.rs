use std::fs::{File, Permissions};
use std::fs::{remove_dir, set_permissions};
use std::os::unix::fs::{PermissionsExt};
use std::io::{BufReader, BufRead};
use std::path::{Path, PathBuf}
use libc::pid_t;

pub struct ResourceConfig {
    pub memory_limit: String,
    pub cpu_share: String,
    pub cpu_set: String
}

pub trait Subsystem {
    pub fn name() -> String;
    pub fn set(&self, cgroup_path: &Path, res: &ResourceConfig) -> Result<_, String> {
        let subsys_cgroup_path = get_cgroup_path(self.name(), cgroup_path, true)?;
        if res.cpu_share != "" {
            let p = subsys_cgroup_path.join(Path::new("cpu.shares"));
            set_permissions(&p, Permissions::from_mode(0o644)).map_err(|e| format!("Error setting permissions: {}", e));
            let f = File::create(&p)?;
            f.write_all(&res.cpu_share.into_bytes()).map_err(|e| format!("set cgroup {} share fail {}", self.name(), e))?;
            Ok(_)
        }
        Err(_)
    }
    fn apply(path: &Path, pid: pid_t) -> Result<_, String> {
        let subsys_cgroup_path = get_cgroup_path(self.name(), cgroup_path, false)?;
        let p = subsys_cgroup_path.join(Path::new("tasks"));
        set_permissions(&p, Permissions::from_mode(0o644)).map_err(|e| format!("Error setting permissions: {}", e));
        let f = File::create(&p)?;
        f.write_all(pid.to_string().into_bytes()).map_err(|e| format!("set cgroup {} share fail {}", self.name(), e))?;
        // f.write_all([..pid]).map_err(|e| format!("set cgroup {} share fail {}", self.name(), e))?;

    }
    fn remove(path: &Path) -> Result<_, String> {
        let subsys_cgroup_path = get_cgroup_path(self.name(), cgroup_path, false)?;
        remove_dir(subsys_cgroup_path)?;
        Ok(_)
    }
}

fn find_cgroup_mountpoint(subsystem: &Path) -> Result<&Path, String> {
    let file = File::open(&Path::new("/proc/self/mountinfo"))
        .map_err(|e| format!("Can't open /proc/self/mountinfo : {}", e))?;
    let file = BufReader::new(file);
    for line in file.lines() {
        let line = line.map_err(|e| format!("Error reading requirements: {}", e))?;
        let fields: Vec<&str> = line.split(" ").collect();
        fields.last().expect("fields is empty.")?.split(",").for_each(|opt| {
            if some(opt) == subsystem.to_str() {
                fields[4]
            }
        })
    }
}

pub fn get_cgroup_path(subsystem: &Path, cgroup_path: &Path, auto_create: bool) -> Result<&Path, String> {
    let cgroup_root = find_cgroup_mountpoint(subsystem);
    let p = cgroup_root.join(cgroup_path);
    if p.exists() || auto_create {
        if p.exists() == false {
            create_dir(p)?;
            return Ok(p);
        }
        return Ok(p);
    }
    return Err("error create cgroup");
}

pub struct CpuSubSystem {}

impl Subsystem for CpuSubSystem {
    pub fn name() -> String {
        "cpu".to_string()
    }
}

pub struct CpusetSubSystem {}

impl Subsystem for CpusetSubSystem {
    pub fn name() -> String {
        "cpuseet".to_string()
    }
}

pub struct MemorySubSystem {}

impl Subsystem for MemorySubSystem {
    pub fn name() -> String {
        "memory".to_string()
    }
}

pub let subsystems_ins = [&CpuSubSystem{}, &CpusetSubSystem{}, &MemorySubSystem{}]
