use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{self};
use eyre::{eyre, ContextCompat};
use modfile::ptmf::{self};

use std::env;
use std::fs::File;
use std::io::BufReader;
use std::num::NonZeroU8;

use playmod::*;

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

    let mut sink = match config.sample_format() {
        cpal::SampleFormat::F32 => Sink::new::<f32>(&device, &config.into())?,
        cpal::SampleFormat::I16 => Sink::new::<i16>(&device, &config.into())?,
        cpal::SampleFormat::U16 => unimplemented!(), /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    };

    let mut speed = 6;

    let mut channel_states = Vec::new();
    for _ in 0..4 {
        channel_states.push(ChannelState::new())
    }
    let mut mixing_buf = vec![0i16; 960];

    sink.start()?;

    let mut jump_offset = 0;
    'all: for pat in module
        .positions
        .data
        .map(|order| &module.patterns[order as usize])
        .iter()
        .take(module.length as usize)
    {
        for row in pat.rows.iter().skip(jump_offset) {
            jump_offset = 0;

            mixing_buf.fill(0);
            let res = drive_row(&mut sink, &mut mixing_buf, row, &mut channel_states, &module.sample_info, &mut speed, sample_rate);

            match res {
                NextAction::Continue => {},
                NextAction::Jump(offs) => {
                    jump_offset = offs;
                    continue 'all;
                }
            }
        }
    }

    Ok(())
}
