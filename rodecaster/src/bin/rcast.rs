use std::time::{Duration, Instant};

use rodecaster::{Device, Event, Recorder};

fn usage() -> ! {
    eprintln!(
        "\
Usage: rcast <command>

Commands:
  profile                   channel profile as the device reports it
  watch                     stream mute changes until interrupted
  mute <channel> on|off     set a channel's mute (channel is 0-based, as `profile` lists)
  dump                      complete device state as lossless typed JSON
  record status             the desk's own recorder: stopped, paused or recording, and whether
                            an SD card or USB drive is there to write to
  record start|stop         ask the desk's own recorder to start or stop, then print what it
                            reports for a few seconds

Examples:
  rcast profile
  rcast mute 0 on
  rcast watch
  rcast record status"
    );
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or_else(|| usage());

    let mut device = match Device::open() {
        Ok(device) => device,
        Err(e) => {
            eprintln!("rcast: {e}");
            std::process::exit(2);
        }
    };
    eprintln!("connected to {}", device.model());

    match command {
        "profile" => profile(&device),
        "dump" => dump(&device),
        "watch" => watch(&mut device),
        "record" => record(&mut device, &args[1..]),
        "mute" => {
            let [channel, state] = &args[1..] else {
                usage()
            };
            let Ok(channel) = channel.parse::<usize>() else {
                usage()
            };
            let mute = match state.as_str() {
                "on" => true,
                "off" => false,
                _ => usage(),
            };
            if let Err(e) = device.set_mute(channel, mute) {
                eprintln!("rcast: {e}");
                std::process::exit(2);
            }
            watch(&mut device);
        }
        _ => usage(),
    }
}

fn profile(device: &Device) {
    let profile = match device.profile() {
        Ok(profile) => profile,
        Err(e) => {
            eprintln!("rcast: {e}");
            std::process::exit(2);
        }
    };
    println!("{}", profile.model);
    for channel in &profile.channels {
        println!(
            "\n[{}] {}  level {:.2}  {}{}",
            channel.channel,
            channel.label,
            channel.level,
            if channel.mute { "MUTED" } else { "live" },
            if channel.cue { "  CUE" } else { "" }
        );
        if !channel.processing.is_empty() {
            println!("    on: {}", channel.processing.join(", "));
        }
        for field in &channel.settings {
            println!("    {:<38} {}", field.name, field.value);
        }
    }
}

fn dump(device: &Device) {
    match device.dump().and_then(|dump| {
        serde_json::to_string_pretty(&dump).map_err(|e| rodecaster::Error::Device(e.to_string()))
    }) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("rcast: {e}");
            std::process::exit(2);
        }
    }
}

fn record(device: &mut Device, args: &[String]) {
    let [action] = args else { usage() };
    let on = match action.as_str() {
        "status" => {
            match device.recorder() {
                Ok(recorder) => println!("{}", describe(recorder)),
                Err(e) => fail(&e),
            }
            return;
        }
        "start" => true,
        "stop" => false,
        _ => usage(),
    };
    if let Err(e) = device.set_recording(on) {
        fail(&e);
    }
    eprintln!("requested {action}; watching for what the desk reports for 6 s");
    let until = Instant::now() + Duration::from_secs(6);
    while Instant::now() < until {
        match device.poll(Duration::from_millis(200)) {
            Event::Recorder(recorder) => println!("{}", describe(recorder)),
            Event::Disconnected(reason) => {
                eprintln!("rcast: {reason}");
                std::process::exit(2);
            }
            Event::Mute(_) | Event::Resynced | Event::Idle => {}
        }
    }
    match device.recorder() {
        Ok(recorder) => println!("after 6 s: {}", describe(recorder)),
        Err(e) => fail(&e),
    }
}

fn describe(recorder: Recorder) -> String {
    format!(
        "{:?}, {}",
        recorder.state,
        if recorder.media_present {
            "card or drive present"
        } else {
            "no card or drive"
        }
    )
}

fn fail(error: &rodecaster::Error) -> ! {
    eprintln!("rcast: {error}");
    std::process::exit(2);
}

fn watch(device: &mut Device) {
    eprintln!("watching for mute changes — ctrl-c to stop");
    loop {
        match device.poll(Duration::from_millis(200)) {
            Event::Mute(change) => println!(
                "channel {} {}",
                change.channel,
                if change.muted { "muted" } else { "unmuted" }
            ),
            Event::Recorder(recorder) => println!("recorder: {}", describe(recorder)),
            Event::Resynced => eprintln!("device resent its state"),
            Event::Idle => {}
            Event::Disconnected(reason) => {
                eprintln!("rcast: {reason}");
                std::process::exit(2);
            }
        }
    }
}
