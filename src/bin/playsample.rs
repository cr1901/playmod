use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{self};
use eyre::{eyre, ContextCompat};
use modfile::ptmf::{self, SampleInfo};

use clap::Parser;

use std::fs::File;
use std::io::BufReader;
use std::num::{NonZero, NonZeroU8};

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

    let mut sink = match config.sample_format() {
        cpal::SampleFormat::F32 => Sink::new::<f32>(&device, &config.into())?,
        cpal::SampleFormat::I16 => Sink::new::<i16>(&device, &config.into())?,
        cpal::SampleFormat::U16 => unimplemented!(), /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    };

    sink.start()?;

    let freq = 7093789.2 / (args.note as u16 as f32 * 2.0);
    let sample_rate_ = sample_rate as f32;

    let inc_rate = (((freq / sample_rate_) * 256.0) as u32 >> 8) as u16;
    let inc_rate_frac: u8 = (((freq / sample_rate_) * 256.0) as u32 % 256) as u8;
    let host_samples_per_tick = (sample_rate_ / 50.0) as u16;

    let mut mixing_buf = vec![0i16; host_samples_per_tick as usize];
    println!("{}, {}, {}, {}", freq, sample_rate_, inc_rate, inc_rate_frac);

    let mut cstate = ChannelState::new();
    cstate.new_sample(NonZeroU8::new(args.sample_no).unwrap());
    cstate.set_volume(64);
    cstate.set_period(args.note as u16);

    for _ in 0..200 {
        play_tick(&mut sink,&mut mixing_buf, &mut cstate, sample,sample_rate);
    }

    Ok(())
}


pub fn play_tick<S>(sink: &mut S, mixing_buf: &mut Vec<i16>, cstate: &mut ChannelState, sample: &SampleInfo, sample_rate: u32) where 
S: PushSamples {
    mixing_buf.fill(0);
    cstate.mix_sample_for_tick(
        mixing_buf,
        sample,
        sample_rate
    );
    sink.push_samples(&mixing_buf);
}
