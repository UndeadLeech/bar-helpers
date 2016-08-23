extern crate time;
extern crate rand;
extern crate toml;
extern crate i3ipc;
extern crate regex;

mod config;

use std::thread;
use regex::Regex;
use std::time::Duration;
use i3ipc::I3Connection;
use std::io::prelude::*;
use std::process::{Command, Stdio};
use std::os::unix::net::UnixStream;
use config::{Config, Executables, Colors};
use std::os::unix::io::{FromRawFd, IntoRawFd};


struct Screen {
    name: String,
    xres: String,
    xoffset: String
}


fn add_reset(input: &String) -> String {
    format!("{}%{{B-}}%{{F-}}%{{T-}}", input)
}

fn get_ws(screen: &String, config: &Config, colors: &Colors, exec: &Executables,
          display_count: &i32, workspaces: &Vec<i3ipc::reply::Workspace>) -> String {
    let mut result_str = String::new();

    for (i, icon) in config.workspace_icons.chars().enumerate() {
        let mut ws_index = None;
        for (x, workspace) in workspaces.iter().enumerate() {
            if &workspace.output == screen {
                let normed_ws_num = (workspace.num - 1) / display_count;
                if normed_ws_num == i as i32 {
                    ws_index = Some(x);
                }
            }
        }

        let ws_script = format!("{} {}", exec.ws, i + 1);

        if ws_index.is_none() {
            result_str = format!("{}%{{B{}}}%{{F{}}}%{{A:{}:}}{}{}{}%{{A}}",
                            result_str, colors.bg_col, colors.bg_sec,
                            ws_script, config.ws_pad, icon, config.ws_pad);
        }
        else {
            let ws_index = ws_index.unwrap();
            if workspaces[ws_index].visible {
                result_str = format!("{}%{{B{}}}%{{F{}}}%{{A:{}:}}{}{}{}%{{A}}",
                                result_str, colors.bg_sec, colors.fg_col,
                                ws_script, config.ws_pad, icon, config.ws_pad);
            }
            else {
                if workspaces[ws_index].urgent {
                    result_str = format!("{}%{{B{}}}%{{F{}}}%{{A:{}:}}{}{}{}%{{A}}",
                                    result_str, colors.bg_col, colors.hl_col,
                                    ws_script, config.ws_pad, icon, config.ws_pad);
                }
                else {
                    result_str = format!("{}%{{B{}}}%{{F{}}}%{{A:{}:}}{}{}{}%{{A}}",
                                    result_str, colors.bg_col, colors.fg_sec,
                                    ws_script, config.ws_pad, icon, config.ws_pad);
                }
            }
        }
    }
    add_reset(&result_str)
}

fn get_date(config: &Config, colors: &Colors) -> String {
    let curr_time = time::now();
    let curr_time_clock = match curr_time.strftime("%H:%M") {
        Ok(fmt) => fmt,
        Err(_) => return String::new(),
    };
    add_reset(&format!("%{{B{}}}%{{F{}}}{}{}{}",
                       colors.bg_sec, colors.fg_col, config.dat_pad, curr_time_clock, config.dat_pad))
}

fn get_not(screen: &String, config: &Config, colors: &Colors, exec: &Executables) -> String {
    // Connect to server and check for message
    let mut stream = match UnixStream::connect("/tmp/leechnot.sock") {
        Ok(us) => us,
        Err(_) => return String::new(),
    };
    let _ = stream.write_all(b"show");
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    if response.starts_with("{") {
        let not_script = format!("{} {} {} &", exec.not, screen, config.height);
        return add_reset(&format!("%{{B{}}}%{{F{}}}%{{A:{}:}}{}{}%{{A}}",
                                  colors.hl_col, colors.bg_col, not_script, config.not_pad, config.not_pad));
    }
    String::new()
}

fn get_vol(screen: &String, config: &Config, colors: &Colors, exec: &Executables) -> String {
    let cmd_out = Command::new("amixer")
        .args(&["-D", "pulse", "get", "Master"])
        .output();
    match cmd_out {
        Ok(out) => {
            let out_str = String::from_utf8_lossy(&out.stdout);
            let vol_end = &out_str[..match out_str.find("%") {
                Some(pos) => pos,
                None => return String::new(),
            }];
            let vol = format!("{:>3}", &vol_end[match vol_end.rfind("[") {
                Some(pos) => pos,
                None => return String::new(),
            } +1..]);
            let vol_script = format!("{} {} {} &", exec.vol, screen, config.height);
            add_reset(&format!("%{{B{}}}%{{F{}}}%{{A:{}:}}{} {}{}%{{A}}",
                               colors.bg_sec, colors.fg_col, vol_script, config.vol_pad, vol, config.vol_pad))
        },
        Err(_) => String::new(),
    }
}

fn get_pow(screen: &String, config: &Config, colors: &Colors, exec: &Executables) -> String {
    let pow_script = format!("{} {} {} &", exec.pow, screen, config.height);
    add_reset(&format!("%{{B{}}}%{{F{}}}%{{A:{}:}}{}{}{}%{{A}}",
                       colors.bg_sec, colors.fg_col, pow_script, config.pow_pad, config.power_icon, config.pow_pad))
}

fn get_screens() -> Vec<Screen> {
    let mut screens = Vec::new();
    let xrandr_out = match Command::new("xrandr").output() {
        Ok(out) => out,
        Err(_) => return Vec::new(),
    };
    let xrandr_str = String::from_utf8_lossy(&xrandr_out.stdout);
    let screen_re = Regex::new("([a-zA-Z0-9-]*) connected ([0-9]*)x[^+]*\\+([0-9]*)")
        .unwrap();
    for caps in screen_re.captures_iter(&xrandr_str) {
        screens.push(Screen {
            name: caps.at(1).unwrap().to_owned(),
            xres: caps.at(2).unwrap().to_owned(),
            xoffset: caps.at(3).unwrap().to_owned()
        });
    }
    screens
}

fn i3ipc_get_workspaces(i3con: &mut I3Connection) -> Vec<i3ipc::reply::Workspace> {
    match i3con.get_workspaces() {
        Ok(gw) => gw.workspaces,
        Err(_) => {
            *i3con = match I3Connection::connect() {
                Ok(i3c) => i3c,
                Err(_) => return Vec::new(),
            };
            match i3con.get_workspaces() {
                Ok(gw) => gw.workspaces,
                Err(_) => Vec::new(),
            }
        }
    }
}

fn main() {
    let screens = get_screens();
    let display_count = screens.len() as i32;

    let mut bar_threads = Vec::new();
    for screen in screens.iter() {
        // Load user settings from file
        let config = config::get_config();
        let colors = config::get_colors();
        let exec = config::get_executables();

        // Clone screen props so they're accessible by all threads
        let name = screen.name.clone();
        let xres = screen.xres.clone();
        let xoffset = screen.xoffset.clone();

        // Start i3ipc connection
        let mut i3con = I3Connection::connect().unwrap();

        // Get static pow block
        let pow_block = get_pow(&name, &config, &colors, &exec);

        // Start lemonbar
        let rect = format!("{}x{}+{}+0", xres, config.height, xoffset);
        let mut lemonbar = Command::new("lemonbar")
            .args(&["-g", &rect[..],
                  "-F", &colors.fg_col[..], "-B", &colors.bg_col[..],
                  "-f", &config.font[..], "-f", &config.icon_font[..]])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();

        // Thread that controls executing lemonbar stdout
        let stdout = lemonbar.stdout.take().unwrap();
        thread::spawn(move || {
            unsafe {
                let _ = Command::new("sh")
                                .stdin(Stdio::from_raw_fd(stdout.into_raw_fd()))
                                .spawn();
            }
        });

        // Thread that writes to lemonbar stdin
        bar_threads.push(thread::spawn(move || {
            let stdin = lemonbar.stdin.as_mut().unwrap();
            loop {
                // Get workspaces from i3ipc, restablish connection if necessary
                let workspaces = i3ipc_get_workspaces(&mut i3con);

                let date_block = get_date(&config, &colors);
                let ws_block = get_ws(&name, &config, &colors, &exec,
                                      &display_count, &workspaces);
                let not_block = get_not(&name, &config, &colors, &exec);
                let vol_block = get_vol(&name, &config, &colors, &exec);

                let bar_string = format!("{}{}{}%{{c}}{}%{{r}}{}{}{}\n",
                                    pow_block, config.gen_pad, ws_block, date_block,
                                    not_block, config.gen_pad, vol_block);
                let _ = stdin.write((&bar_string[..]).as_bytes());

                thread::sleep(Duration::from_millis(100));
            }
        }));
    }

    for bar_thread in bar_threads {
        let _ = bar_thread.join();
    }
}
