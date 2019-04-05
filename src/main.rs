use std::env;
use std::process::exit;

// use libc::{getuid, kill, c_int, pid_t};
use subsystem::ResourceConfig;
use container::{new_parent_process, delete_work_space, run_container_init_process};
use cgroup::CgroupManager;

extern crate env_logger;
extern crate argparse;
extern crate libc;
extern crate env_logger;
extern crate rand;

// #[macro_use] extern crate quick_error;
#[macro_use] extern crate log;
#[cfg(feature="containers")] extern crate unshare;

use argparse::{ArgumentParser, StoreFalse, Store};
use rand::{thread_rng, Rng};
use unshare::{Command, Stdio, Fd, Child, PipeWriter};

#[cfg(feature="containers")]
fn main() {
    env_logger::init();

    let mut args: Vec<String> = env::args().collect().remove(0);
    let ep = args.get(1).unwrap_or(false);

    let code = match &ep[..] {
        "run" => run(args[2..]),
        "init" => init_process(args[2..]),
        // "stop" => stop_command(args[2..]),
        // "exec" => exec_command(args[2..]),
        // "rm" => remove_command(args[2..]),
        // "commit" => commit_command(args[2..]),
        // "network" => network_command(args),
        _ => run(args[2..]),
    }
    exit(code);
}

fn init_process(input_args: Vec<String>) {
    match run_container_init_process() {
        Ok(_) => {
            info!("parent process init ok");
            return
        }
        Err(e) => {
            error!("parent process init failed: {}", e);
            return
        }
    }
}

// fn stop_command(input_args: Vec<String>) {
//     if input_args.len() < 1 {
//         error!("Missing container command");
//         return;
//     }
//     let container_name = input_args[0];
//     stop_container(container_name);
// }
//
// fn stop_container(container_name: String) {
//
// }

// pub fn send_signal(sig: Signal, pid: pid_t, cmd_name: &String) {
//     if unsafe { kill(pid, sig as c_int) } < 0 {
//         let e = io::Error::last_os_error();
//         error!("Error sending {:?} to {:?}: {}", sig, cmd_name, e);
//     }
// }

fn run(input_args: Vec<String>) {
    let mut create_tty = false;
    let mut detach = false;
    let mut res_conf = ResourceConfig{};
    let mut container_name: String;
    let mut volume: String;
    let mut network: String;
    let mut env_slice: String;
    let mut portmapping: String;

    let mut image_name: String;
    let mut cname: String;

    if input_args.len() < 1 {
        error!("Missing container command");
        return;
    }

    let mut ap = ArgumentParser::new();
    ap.refer(&mut create_tty).add_option(&["-ti"], StoreFalse, "enable tty");
    ap.refer(&mut detach).add_option(&["-d"], StoreFalse, "detach container");
    ap.refer(&mut res_conf.memory_limit).add_option(&["-m"], Store, "memory limit");
    ap.refer(&mut res_conf.cpu_share).add_option(&["--cpushare"], Store, "cpushare limit");
    ap.refer(&mut res_conf.cpu_set).add_option(&["--cpuset"], Store, "cpuset limit");
    ap.refer(&mut container_name).add_option(&["--name"], Store, "container name");
    ap.refer(&mut volume).add_option(&["-v"], Store, "volume");
    ap.refer(&mut env_slice).add_option(&["-e"], Store, "set environment");
    ap.refer(&mut network).add_option(&["--net"], Store, "container network");
    ap.refer(&mut portmapping).add_option(&["-p"], Store, "port mapping");
    ap.refer(&mut image_name).add_argument("image_name", Store, "image name");
    ap.refer(&mut cname).add_argument("command", Store, "command");

    if create_tty && detach {
        error!("ti and d paramter can not both provided");
        return;
    }

	let container_id := rand_string_bytes(10);
    if container_name = "" {
        container_name = container_id;
    }
    let (&cmd, piped_writer) = match new_parent_process(create_tty, &container_name, &volume, &image_name, &env_slice) {
        Ok((&cmd, piped_writer)) => (&cmd, piped_writer),
        Err(e) => {
            error!("New parent process error: {}", e);
            return
        }
    }

    // recordContainerInfo(&cmd.pid, &input_args, &container_id, &volume);
    let mut child = cmd.spawn().map_err(|e| error!("New parent process error: {}", e)).unwrap();
    let child_pid = child.pid;

    let cgroup_manager = CgroupManager::new_cgroup_manager(input_args[0]);
    cgroup_manager.set(res_conf);
    cgroup_manager.apply(child_pid);

    send_init_command(input_args, piped_writer);

    if let Err(e) = piped_writer.write_all(input_args.as_bytes()) {
        error!("pipe write error: {}", e);
        return
    }

    if create_tty {
        match child.wait() {
            Ok(status) => {
                if status.success() {
                    delete_work_space(volume, container_name);
                }
            }
        }
    }
}

fn rand_string_bytes(n :u32) -> String {
    let mut rng = thread_rng();
    let b = vec![0; n];
    for index in (0..n) {
        b[index] = rng.gen_range(0, 10);
    }
    String::from_utf8(b)
}
