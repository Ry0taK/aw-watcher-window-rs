#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use chrono::{Duration, Utc};
use clap::Parser;
use regex::Regex;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::{thread, time};
use winsafe::{co, prelude::*, HPROCESS, HWND};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(
        long,
        default_value = "localhost",
        help = "The hostname of the ActivityWatch server to connect to"
    )]
    host: String,

    #[arg(
        long,
        default_value_t = 5600,
        help = "The port of the ActivityWatch server to connect to"
    )]
    port: u16,

    #[arg(long, default_value_t = false, help = "Disable title reporting")]
    exclude_title: bool,

    #[arg(short, long, num_args = 1.., value_delimiter = ',', help = "Comma-separated list of regex patterns that matches process names (e.g., Firefox.exe) to exclude titles from")]
    exclude_title_processes: Vec<String>,

    #[arg(short, long, num_args = 1.., value_delimiter = ',', help = "Override the exclusion rule for processes with regex patterns")]
    include_title_processes: Vec<String>,

    #[arg(long, default_value_t = 5000, help = "Poll time in milliseconds")]
    poll_time: u32,

    #[arg(long, default_value_t = false, help = "Enable debug logging")]
    debug: bool,
}

fn main() {
    let args = Args::parse();
    let hostname = gethostname::gethostname()
        .into_string()
        .expect("Failed to get hostname");
    let client =
        aw_client_rust::blocking::AwClient::new(&args.host, args.port, "aw-watcher-window-rs")
            .expect("Failed to create a client");
    let exclude_title_processes = args
        .exclude_title_processes
        .iter()
        .map(|s| Regex::new(s).unwrap_or_else(|_| Regex::new(regex::escape(s).as_str()).unwrap()))
        .collect::<Vec<Regex>>();
    let include_title_processes = args
        .include_title_processes
        .iter()
        .map(|s| Regex::new(s).unwrap_or_else(|_| Regex::new(regex::escape(s).as_str()).unwrap()))
        .collect::<Vec<Regex>>();
    let window_bucket = format!("aw-watcher-window-rs_{}", hostname);

    loop {
        match client.create_bucket_simple(&window_bucket, "currentwindow") {
            Ok(_) => break,
            Err(e) => {
                eprintln!("Failed to create bucket: {}. Retrying...", e);
                thread::sleep(time::Duration::from_millis(1000));
            }
        }
    }

    let mut prev_app = String::new();
    let mut prev_title = String::new();

    loop {
        thread::sleep(time::Duration::from_millis(args.poll_time.into()));
        let active_window = match HWND::GetForegroundWindow() {
            Some(hwnd) => hwnd,
            None => {
                if args.debug {
                    println!("No active window found");
                }
                thread::sleep(time::Duration::from_millis(args.poll_time.into()));
                continue;
            }
        };
        let (_, process_id) = active_window.GetWindowThreadProcessId();

        let process_handle =
            match HPROCESS::OpenProcess(co::PROCESS::QUERY_INFORMATION, false, process_id) {
                Ok(handle) => handle,
                Err(e) => {
                    eprintln!("Failed to open process handle: {}", e);
                    continue;
                }
            };

        let process_fullpath =
            match process_handle.QueryFullProcessImageName(co::PROCESS_NAME::WIN32) {
                Ok(path) => path,
                Err(e) => {
                    eprintln!("Failed to query process path: {}", e);
                    continue;
                }
            };

        let pathbuf = PathBuf::from(process_fullpath);
        let process_name = match pathbuf.file_name().unwrap().to_str() {
            Some(s) => s,
            None => {
                eprintln!("Failed to convert process name to string");
                continue;
            }
        };

        let window_title = match active_window.GetWindowText() {
            Ok(title) => title,
            Err(e) => {
                eprintln!("Failed to get window title: {}", e);
                continue;
            }
        };

        let app = process_name.to_string();
        let title = if (args.exclude_title
            || exclude_title_processes
                .iter()
                .any(|r| r.is_match(&process_name.to_string())))
            && !include_title_processes
                .iter()
                .any(|r| r.is_match(&process_name.to_string()))
        {
            process_name.to_string()
        } else {
            window_title
        };

        if app == prev_app && title == prev_title {
            let mut data = Map::new();
            data.insert("app".to_string(), Value::String(app.clone()));
            data.insert("title".to_string(), Value::String(title.clone()));
            match ping(data, &client, &window_bucket, Utc::now(), &args) {
                Ok(_) => (),
                Err(e) => eprintln!("Failed to send heartbeat: {}", e),
            }
            continue;
        }

        let mut prev_data = Map::new();
        prev_data.insert("app".to_string(), Value::String(prev_app.clone()));
        prev_data.insert("title".to_string(), Value::String(prev_title.clone()));

        let now = Utc::now();

        match ping(
            prev_data,
            &client,
            &window_bucket,
            now - Duration::milliseconds(1),
            &args,
        ) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to send heartbeat: {}", e);
                continue;
            }
        }

        let mut new_data = Map::new();
        new_data.insert("app".to_string(), Value::String(app.clone()));
        new_data.insert("title".to_string(), Value::String(title.clone()));
        match ping(new_data, &client, &window_bucket, now, &args) {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to send heartbeat: {}", e);
                continue;
            }
        }

        prev_app = app;
        prev_title = title;
    }
}

fn ping(
    data: Map<String, Value>,
    client: &aw_client_rust::blocking::AwClient,
    bucket: &str,
    timestamp: chrono::DateTime<Utc>,
    args: &Args,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.debug {
        println!("Logging event: {:?}", data);
    }
    let event = aw_client_rust::Event {
        id: None,
        timestamp,
        duration: Duration::seconds(0),
        data,
    };
    client.heartbeat(bucket, &event, (args.poll_time + 1000) as f64)?;
    Ok(())
}
