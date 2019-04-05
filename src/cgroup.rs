use std::path::{Path, PathBuf};
use subsystem::{ResourceConfig, subsystems_ins};
use libc::pid_t;

pub struct CgroupManager {
    pub path: &Path,
    resource: &ResourceConfig
}

pub impl CgroupManager {
    pub fn new_cgroup_manager(path: &Path) -> &cgroup_manager {
        &cgroup_manager{ path: path }
    }

    pub fn apply(&self, pid: pid_t) -> Result<(), String> {
        for sub_sys_ins in &subsystems_ins {
            sub_sys_ins.apply(self.path, pid)?;
        }
        Ok(())
    }

    pub fn set(&self, res &ResourceConfig) -> Result<(), String> {
        for sub_sys_ins in subsystems_ins {
            sub_sys_ins.set(self.path, res)?;
        }
        Ok(())
    }

    pub fn destroy(&self) -> Result<(), String> {
        for sub_sys_ins in subsystems_ins {
                sub_sys_ins.remove(self.path)?;
        }
        Ok(())
    }
}
