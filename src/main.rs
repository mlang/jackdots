use clap::{App, Arg, crate_authors, crate_description, crate_name, crate_version};
use std::char;
use std::cmp::Ordering::Equal;
use std::io::{self, Write};
use std::sync::mpsc;
use std::time::Instant;

enum Mode {
    PeakMeter, Spectrum
}

impl Default for Mode { fn default() -> Self { Self::PeakMeter } }

enum Message {
    Peak(f32), Quit
}

fn main() {
    let params = App::new(crate_name!())
        .author(crate_authors!())
        .version(crate_version!())
        .about(crate_description!())
        .arg(Arg::with_name("PORT"))
        .get_matches();
    let mode = Mode::default();
    let (client, _status) = jack::Client::new(
        crate_name!(), jack::ClientOptions::NO_START_SERVER
    ).expect("JACK");

    let in_1 = client
        .register_port("in_1", jack::AudioIn::default())
        .unwrap();

    if let Some(port) = params.value_of("PORT") {
        if let Some(port) = client.port_by_name(port) {
            client.connect_ports(&port, &in_1);
        }
    }

    let (sender, receiver) = mpsc::sync_channel(64);
    let controller = sender.clone();
    let callback = move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        let samples = in_1.as_slice(ps);
        let partial_cmp = |a: &f32, b: &f32| a.partial_cmp(b).unwrap_or(Equal);
        match mode {
            Mode::PeakMeter => {
                let peak = Message::Peak(
                    samples.iter().map(|v| v.abs()).max_by(partial_cmp).unwrap()
                );
                let _ = sender.try_send(peak);
            },
            Mode::Spectrum => {
            }
        };
        jack::Control::Continue
    };
    let active_client = client.activate_async((),
        jack::ClosureProcessHandler::new(callback)
    ).unwrap();

    std::thread::spawn(move || {
        let mut display = PeakDisplay::new(68);
        for v in receiver.iter() {
            match v {
                Message::Peak(p) => display.update(p),
                Message::Quit => break
            }
        }
        println!("thxbye!")
    });

    println!("\u{28FF} Press return to quit \u{28FF}");
    let mut user_input = String::new();
    io::stdin().read_line(&mut user_input).ok();

    controller.send(Message::Quit).unwrap();

    active_client.deactivate().unwrap();
}

struct PeakDisplay {
    peak: f32,
    dpeak: usize,
    dtime: std::time::Instant,
    last_print: std::time::Instant,
    interval: std::time::Duration
}

impl PeakDisplay {
    pub fn new(cells: usize) -> Self {
        Self {
            peak: 0.0,
            dpeak: 0,
            dtime: Instant::now(),
            last_print: Instant::now(),
            interval: std::time::Duration::from_millis(100)
        }
    }
    pub fn update(&mut self, peak: f32) {
        if peak > self.peak {
            self.peak = peak;
        }
        if self.last_print.elapsed() >= self.interval {
            let decibel = 20.0 * self.peak.log10();
            let mut line = Line::new(68);
            for mark in vec![0.0, -5.0, -10.0, -15.0, -20.0, -25.0, -30.0, -35.0, -40.0, -50.0, -60.0].into_iter() {
                line.set(iec_scale(mark, line.width)-1, 0)
            }
            let size = iec_scale(decibel, line.width);
            if size > self.dpeak {
                self.dpeak = size;
                self.dtime = Instant::now();
            } else if self.dtime.elapsed() > std::time::Duration::from_millis(1600) {
                self.dpeak = size
            }
            for x in 0..size {
                line.set(x, 2); line.set(x, 3);
            }
            if self.dpeak > 0 {
                for y in 1..=3 {
                    line.set(self.dpeak - 1, y)
                }
            }
            print!("\r{} {} dB   ", line, decibel.round());
            io::stdout().flush();
            self.peak = 0.0f32;
            self.last_print = Instant::now();
        }
    }
}

fn iec_scale(decibel: f32, width: usize) -> usize {
    let deflection = if decibel < -70.0 {
        0.0
    } else if decibel < -60.0 {
        (decibel + 70.0) * 0.25
    } else if decibel < -50.0 {
        (decibel + 60.0) * 0.5 + 2.5
    } else if decibel < -40.0 {
        (decibel + 50.0) * 0.75 + 7.5
    } else if decibel < -30.0 {
        (decibel + 40.0) * 1.5 + 15.0
    } else if decibel < -20.0 {
        (decibel + 30.0) * 2.0 + 30.0
    } else if decibel < 0.0 {
        (decibel + 20.0) * 2.5 + 50.0
    } else {
        100.0
    };
    ((deflection / 100.0) * width as f32) as usize
}

struct Line {
    pub width: usize,
    data: Vec<u8>
}

impl Line {
    pub fn new(width: usize) -> Self {
        let mut data = Vec::new();
        for _ in 0..width {
            data.push(0);
        }
        let width = width * 2;
        Self { width, data }
    }
    pub fn set(&mut self, x: usize, y: usize) {
        self.data[x / 2] |= match (x % 2, y % 4) {
            (0, 0) => 0b00_000_001,
            (0, 1) => 0b00_000_010,
            (0, 2) => 0b00_000_100,
            (1, 0) => 0b00_001_000,
            (1, 1) => 0b00_010_000,
            (1, 2) => 0b00_100_000,
            (0, 3) => 0b01_000_000,
            (1, 3) => 0b10_000_000,
            _ => panic!()
        }
    }
}

impl std::fmt::Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let line = self.data.iter().map(
            |&dots| char::from_u32(0x2800 | dots as u32).unwrap()
        ).collect::<String>();
        write!(f, "{}", line)
    }
}

struct Notifications;

impl jack::NotificationHandler for Notifications {
    fn thread_init(&self, _: &jack::Client) {
    }

    fn shutdown(&mut self, status: jack::ClientStatus, reason: &str) {
        println!(
            "JACK: shutdown with status {:?} because \"{}\"",
            status, reason
        );
    }

    fn freewheel(&mut self, _: &jack::Client, is_enabled: bool) {
        println!(
            "JACK: freewheel mode is {}",
            if is_enabled { "on" } else { "off" }
        );
    }

    fn buffer_size(&mut self, _: &jack::Client, sz: jack::Frames) -> jack::Control {
        println!("JACK: buffer size changed to {}", sz);
        jack::Control::Continue
    }

    fn sample_rate(&mut self, _: &jack::Client, srate: jack::Frames) -> jack::Control {
        println!("JACK: sample rate changed to {}", srate);
        jack::Control::Continue
    }

    fn client_registration(&mut self, _: &jack::Client, name: &str, is_reg: bool) {
        println!(
            "JACK: {} client with name \"{}\"",
            if is_reg { "registered" } else { "unregistered" },
            name
        );
    }

    fn port_registration(&mut self, _: &jack::Client, port_id: jack::PortId, is_reg: bool) {
        println!(
            "JACK: {} port with id {}",
            if is_reg { "registered" } else { "unregistered" },
            port_id
        );
    }

    fn port_rename(
        &mut self,
        _: &jack::Client,
        port_id: jack::PortId,
        old_name: &str,
        new_name: &str,
    ) -> jack::Control {
        println!(
            "JACK: port with id {} renamed from {} to {}",
            port_id, old_name, new_name
        );
        jack::Control::Continue
    }

    fn ports_connected(
        &mut self,
        _: &jack::Client,
        port_id_a: jack::PortId,
        port_id_b: jack::PortId,
        are_connected: bool,
    ) {
        println!(
            "JACK: ports with id {} and {} are {}",
            port_id_a,
            port_id_b,
            if are_connected {
                "connected"
            } else {
                "disconnected"
            }
        );
    }

    fn graph_reorder(&mut self, _: &jack::Client) -> jack::Control {
        println!("JACK: graph reordered");
        jack::Control::Continue
    }

    fn xrun(&mut self, _: &jack::Client) -> jack::Control {
        println!("JACK: xrun occurred");
        jack::Control::Continue
    }

    fn latency(&mut self, _: &jack::Client, mode: jack::LatencyType) {
        println!(
            "JACK: {} latency has changed",
            match mode {
                jack::LatencyType::Capture => "capture",
                jack::LatencyType::Playback => "playback",
            }
        );
    }
}
