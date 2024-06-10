use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{self, Stream};
use eyre::{eyre, ContextCompat, Result};
use modfile::ptmf::{self, Channel, SampleInfo};

use std::env;
use std::fs::File;
use std::io::BufReader;
use std::time::Duration;

use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use playmod::*;

static BUFFER: Lazy<Arc<Mutex<VecDeque<i16>>>> =
    Lazy::new(|| Arc::new(Mutex::new(VecDeque::new())));

#[derive(Debug)]
struct ChannelState {
    pub state: SampleState,
    pub num: u8,
    pub period: u16
}

impl ChannelState {
    pub fn new() -> Self {
        Self {
            state: SampleState {
                looped_yet: false,
                sample_offset: 0,
                sample_frac: 0
            },
            num: 0,
            period: 0
        }
    }

    pub fn new_sample(&mut self, num: u8) {
        self.state = SampleState {
            looped_yet: false,
            sample_offset: 0,
            sample_frac: 0
        };
        self.num = num;
    }
}

fn main() -> eyre::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        return Err(eyre!("Mod filename required."));
    }

    let ref filename = args[1];
    let file = File::open(filename)?;

    let mut reader = BufReader::new(&file);
    let mut module = ptmf::read_mod(&mut reader, false).unwrap();

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .wrap_err("failed to find output device")?;
    let config = device.default_output_config()?;
    let sample_rate = config.sample_rate().0;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()),
        cpal::SampleFormat::U16 => unimplemented!(), /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    };
    

    let mut speed = 6;

    let mut channel_states = Vec::new();
    for _ in 0..4 {
        channel_states.push(ChannelState::new())
    }
    let mut mixing_buf = vec![0i16; 960];

    let stream = stream.unwrap();
    stream.play().unwrap();

    'all: for pat in module
        .positions
        .data
        .map(|order| &module.patterns[order as usize]).iter().take(module.length as usize)
    {
        for row in pat.rows.iter() {
            let mut tick = 0;

            for (cstate, chan) in channel_states.iter_mut().zip(row.channels.iter()).take(4) {
                if chan.sample_number != 0 {
                    cstate.new_sample(chan.sample_number);
                }

                if chan.period != 0 {
                    cstate.period = chan.period;
                }
            }

            while tick < speed {
                mixing_buf.fill(0);
                for (cstate, chan) in channel_states.iter_mut().zip(row.channels.iter()).take(4) {
                    mix_sample_for_tick(&mut mixing_buf, &mut cstate.state, &module.sample_info[(cstate.num - 1) as usize], cstate.period, sample_rate);
                }

                dump_buf(&mixing_buf);
                tick += 1;
            }

            for (cstate, chan) in channel_states.iter_mut().zip(row.channels.iter()).take(4) {
                if chan.effect & 0x0f00 == 0x0f00 {
                    speed = chan.effect & 0x001f;
                }
            }
        }
        // break 'all;
    }
    // std::thread::sleep(Duration::from_millis(1000));

    Ok(())
}
