use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{self};
use eyre::{eyre, ContextCompat};
use modfile::ptmf::{self};

use std::env;
use std::fs::File;
use std::io::BufReader;
use std::num::NonZeroU8;

use once_cell::sync::Lazy;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use playmod::*;

static BUFFER: Lazy<Arc<Mutex<VecDeque<i16>>>> =
    Lazy::new(|| Arc::new(Mutex::new(VecDeque::new())));

#[derive(Debug)]
struct ChannelState {
    pub state: SampleState,
    pub num: Option<NonZeroU8>,
    pub period: u16,
}

impl ChannelState {
    pub fn new() -> Self {
        Self {
            state: SampleState {
                looped_yet: false,
                sample_offset: 0,
                sample_frac: 0,
            },
            num: None,
            period: 0,
        }
    }

    pub fn new_sample(&mut self, num: NonZeroU8) {
        self.state = SampleState {
            looped_yet: false,
            sample_offset: 0,
            sample_frac: 0,
        };
        self.num = Some(num);
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
    let module = ptmf::read_mod(&mut reader, false).unwrap();

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
        .map(|order| &module.patterns[order as usize])
        .iter()
        .take(module.length as usize)
    {
        for row in pat.rows.iter() {
            let mut tick = 0;

            for (cstate, chan) in channel_states.iter_mut().zip(row.channels.iter()).take(4) {
                if let Some(sample_number) = NonZeroU8::new(chan.sample_number) {
                    cstate.new_sample(sample_number);
                }

                if chan.period != 0 {
                    cstate.period = chan.period;
                }
            }

            while tick < speed {
                mixing_buf.fill(0);
                for (cstate, chan) in channel_states.iter_mut().zip(row.channels.iter()).filter(|(cs,_)| cs.num.is_some()) {
                    mix_sample_for_tick(
                        &mut mixing_buf,
                        &mut cstate.state,
                        &module.sample_info[(cstate.num.unwrap().get() - 1) as usize],
                        cstate.period,
                        sample_rate,
                    );
                }

                dump_buf(&mixing_buf);
                tick += 1;
            }

            for (cstate, chan) in channel_states.iter_mut().zip(row.channels.iter()).take(4) {
                if chan.effect & 0x0f00 == 0x0e00 {
                    println!("Extended Effect {}: arg {}", chan.effect & 0x00f0 >> 4, chan.effect & 0x000f);
                } else if chan.effect & 0x0f00 == 0x0f00 {
                    println!("Change speed to {}", chan.effect & 0x001f);
                    speed = chan.effect & 0x001f;
                }
            }
        }
        // break 'all;
    }
    // std::thread::sleep(Duration::from_millis(1000));

    Ok(())
}
