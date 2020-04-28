use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use rustfft::{FFT, FFTplanner};
use std::char;
use std::cmp::Ordering::Equal;
use std::cmp::min;
use std::io::{self, Write};
use std::sync::mpsc::sync_channel;
use std::time::Instant;

fn main() {
    let (client, _status) =
        jack::Client::new("jackdots", jack::ClientOptions::NO_START_SERVER).unwrap();

    let in_a = client
        .register_port("in_1", jack::AudioIn::default())
        .unwrap();

    let (sender, receiver) = sync_channel(64);
    let process_callback = move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        let _ = sender.try_send(in_a.as_slice(ps).to_vec());
        jack::Control::Continue
    };
    let process = jack::ClosureProcessHandler::new(process_callback);

    let active_client = client.activate_async(Notifications, process).unwrap();

    std::thread::spawn(move || {
        let inverse = false;
        let mut planner = FFTplanner::new(inverse);
        let size = 4096;
        let fft = planner.plan_fft(size);
        let mut signal = Vec::<Complex<f32>>::with_capacity(size);

        let mut last_print = Instant::now();
        for v in receiver.iter() {
            signal.extend(v.into_iter().map(|f| Complex::new(f, 0.0)));
            let mut spectrum = vec![Complex::<f32>::new(0.0, 0.0); size];
            let len = signal.len();
            if len >= size {
                signal.rotate_left(len - size);
                signal.truncate(size);
                if last_print.elapsed() >= std::time::Duration::from_millis(100) {
                    fft.process(&mut signal, &mut spectrum);
                    let mut block = Line::new(78);
                    for (i, c) in spectrum.iter().take(block.width).enumerate() {
                        let mag = 20.0 * (2.0*(c.re*c.re + c.im*c.im).sqrt() / size as f32).log10();
                        let height = if mag < -40.0 {
                            0
                        } else if mag < -30.0 {
                            1
                        } else if mag < -20.0 {
                            2
                        } else if mag < -10.0 {
                            3
                        } else {
                            4
                        };
                        for y in 0..height {
                            block.set(i, 3-y)
                        }
                    }
                    print!("\r{}", block);
                    io::stdout().flush();
                    last_print = Instant::now();
                }
            }
        }
    });

    println!("\u{28FF} Press return to quit {}", std::char::from_u32(0x28FF).unwrap());
    let mut user_input = String::new();
    io::stdin().read_line(&mut user_input).ok();

    active_client.deactivate().unwrap();
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
        println!("JACK: thread init");
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
