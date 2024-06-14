use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{self};
use eyre::{eyre, ContextCompat};
use modfile::ptmf::{self, SampleInfo};

use clap::Parser;

use std::fs::File;
use std::io::BufReader;

use playmod::*;

#[derive(clap::Parser)]
#[clap(author, version)]
/// Switch monitor input source from command-line
pub struct Args {
    pub modfile: String,
    #[clap(value_enum)]
    pub note: Note,
    pub sample_no: u8,
}

fn main() -> eyre::Result<()> {
    let args = Args::parse();

    let file = File::open(args.modfile)?;

    let mut reader = BufReader::new(&file);
    let module = ptmf::read_mod(&mut reader, false).unwrap();

    if args.sample_no < 1 || args.sample_no > 31 {
        return Err(eyre!("sample number must be between 1 and 31"));
    }

    let sample = &module.sample_info[args.sample_no as usize - 1];

    if sample.length == 0 {
        return Err(eyre!("sample seems to be unimplemented"));
    }

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .wrap_err("failed to find output device")
        .unwrap();
    let config = device.default_output_config().unwrap();
    let sample_rate = config.sample_rate().0;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into())?,
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into())?,
        cpal::SampleFormat::U16 => unimplemented!(), /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    };

    stream.play()?;

    let mut state = SampleState {
        looped_yet: false,
        sample_offset: 0,
        sample_frac: 0,
    };

    let freq = 7093789.2 / (args.note as u16 as f32 * 2.0);
    let sample_rate_ = sample_rate as f32;

    let inc_rate = (((freq / sample_rate_) * 256.0) as u32 >> 8) as u16;
    let inc_rate_frac: u8 = (((freq / sample_rate_) * 256.0) as u32 % 256) as u8;
    let host_samples_per_tick = (sample_rate_ / 50.0) as u16;

    let mut mixing_buf = vec![0i16; host_samples_per_tick as usize];
    println!("{}, {}, {}, {}", freq, sample_rate_, inc_rate, inc_rate_frac);
    println!("{}, {}, {}", sample.length, sample.repeat_start, sample.repeat_length);
    for _ in 0..200 {
        play_tick(&mut mixing_buf, &mut state, sample, args.note, sample_rate);
    }

    Ok(())
}


pub fn play_tick(mixing_buf: &mut Vec<i16>, state: &mut SampleState, sample: &SampleInfo, period: Note, sample_rate: u32) {
    mixing_buf.fill(0);
    mix_sample_for_tick(
        mixing_buf,
        state,
        sample,
        period as u16,
        sample_rate,
    );
    dump_buf(&mixing_buf);
}
