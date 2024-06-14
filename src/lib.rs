#![cfg_attr(not(feature = "std"), no_std)]

use cpal::traits::DeviceTrait;
use cpal::{self, Stream};
use eyre::Result;
use modfile::ptmf::SampleInfo;

use clap::ValueEnum;

#[cfg(feature = "std")]
mod hosted;
#[cfg(feature = "std")]
pub use hosted::*;

#[derive(Copy, Clone)]
#[cfg_attr(feature = "std", derive(ValueEnum))]
pub enum Note {
    #[cfg_attr(feature = "clap", clap(name = "C1"))]
    C1 = 856,
    #[cfg_attr(feature = "clap", clap(name = "C#1"))]
    CS1 = 808,
    #[cfg_attr(feature = "clap", clap(name = "D1"))]
    D1 = 762,
    #[cfg_attr(feature = "clap", clap(name = "D#1"))]
    DS1 = 720,
    #[cfg_attr(feature = "clap", clap(name = "E1"))]
    E1 = 678,
    #[cfg_attr(feature = "clap", clap(name = "F1"))]
    F1 = 640,
    #[cfg_attr(feature = "clap", clap(name = "F#1"))]
    FS1 = 604,
    #[cfg_attr(feature = "clap", clap(name = "G1"))]
    G1 = 570,
    #[cfg_attr(feature = "clap", clap(name = "G#1"))]
    GS1 = 538,
    #[cfg_attr(feature = "clap", clap(name = "A1"))]
    A1 = 508,
    #[cfg_attr(feature = "clap", clap(name = "A#1"))]
    AS1 = 480,
    #[cfg_attr(feature = "clap", clap(name = "B1"))]
    B1 = 453,
    #[cfg_attr(feature = "clap", clap(name = "C2"))]
    C2 = 428,
    #[cfg_attr(feature = "clap", clap(name = "C#2"))]
    CS2 = 404,
    #[cfg_attr(feature = "clap", clap(name = "D2"))]
    D2 = 381,
    #[cfg_attr(feature = "clap", clap(name = "D#2"))]
    DS2 = 360,
    #[cfg_attr(feature = "clap", clap(name = "E2"))]
    E2 = 339,
    #[cfg_attr(feature = "clap", clap(name = "F2"))]
    F2 = 320,
    #[cfg_attr(feature = "clap", clap(name = "F#2"))]
    FS2 = 302,
    #[cfg_attr(feature = "clap", clap(name = "G2"))]
    G2 = 285,
    #[cfg_attr(feature = "clap", clap(name = "G#2"))]
    GS2 = 269,
    #[cfg_attr(feature = "clap", clap(name = "A2"))]
    A2 = 254,
    #[cfg_attr(feature = "clap", clap(name = "A#2"))]
    AS2 = 240,
    #[cfg_attr(feature = "clap", clap(name = "B2"))]
    B2 = 226,
    #[cfg_attr(feature = "clap", clap(name = "C3"))]
    C3 = 214,
    #[cfg_attr(feature = "clap", clap(name = "C#3"))]
    CS3 = 202,
    #[cfg_attr(feature = "clap", clap(name = "D3"))]
    D3 = 190,
    #[cfg_attr(feature = "clap", clap(name = "D#3"))]
    DS3 = 180,
    #[cfg_attr(feature = "clap", clap(name = "E3"))]
    E3 = 170,
    #[cfg_attr(feature = "clap", clap(name = "F3"))]
    F3 = 160,
    #[cfg_attr(feature = "clap", clap(name = "F#3"))]
    FS3 = 151,
    #[cfg_attr(feature = "clap", clap(name = "G3"))]
    G3 = 143,
    #[cfg_attr(feature = "clap", clap(name = "G#3"))]
    GS3 = 135,
    #[cfg_attr(feature = "clap", clap(name = "A3"))]
    A3 = 127,
    #[cfg_attr(feature = "clap", clap(name = "A#3"))]
    AS3 = 120,
    #[cfg_attr(feature = "clap", clap(name = "B3"))]
    B3 = 113,
}

pub trait PushSamples {
    fn push_samples(&mut self, buf: &[i16]);
}

#[derive(Debug)]
pub struct SampleState {
    looped_yet: bool,
    sample_offset: u16,
    sample_frac: u8,
}

impl SampleState {
    pub fn new() -> Self {
        Self {
            looped_yet: false,
            sample_offset: 0,
            sample_frac: 0
        }
    }
}

pub fn mix_sample_for_tick<P>(
    buf: &mut Vec<i16>,
    state: &mut SampleState,
    sample: &SampleInfo,
    period: P,
    sample_rate: u32,
) where
    P: Into<u16>,
{
    // FIXME: Get rid of floating point, I don't want it... used fixed-point
    // increments if we have to.
    // 7159090.5 for NTSC
    let freq = 7093789.2 / (period.into() as f32 * 2.0);
    let sample_rate = sample_rate as f32;

    let inc_rate = (((freq / sample_rate) * 256.0) as u32 >> 8) as u16;
    let inc_rate_frac: u8 = (((freq / sample_rate) * 256.0) as u32 % 256) as u8;

    // 60.0 for NTSC
    let host_samples_per_tick = (sample_rate / 50.0) as u16;
    buf.truncate(host_samples_per_tick as usize);

    for i in 0..host_samples_per_tick {
        if sample.repeat_length <= 2 && state.looped_yet {
            break;
        }

        let (new_frac, carry) = state.sample_frac.overflowing_add(inc_rate_frac);
        state.sample_frac = new_frac;

        state.sample_offset += inc_rate + carry as u16;

        if (state.sample_offset >= sample.length * 2) && !state.looped_yet {
            // println!("At {}, going to {} (repeat start {})", state.sample_offset, sample.repeat_start * 2, sample.repeat_start * 2);
            state.looped_yet = true;
            state.sample_offset = sample.repeat_start * 2 + (state.sample_offset - sample.length * 2);
        } else if state.looped_yet && state.sample_offset >= sample.repeat_start * 2 + sample.repeat_length * 2 {
            // println!("At {}, going to {} (repeat start {})", state.sample_offset, state.sample_offset - sample.repeat_length * 2, sample.repeat_start * 2);
            state.sample_offset -= sample.repeat_length * 2;
        }

        let curr_sample_val = sample.data[state.sample_offset as usize] as i8 as i16;
        buf[i as usize] += curr_sample_val << 3; // Raw values are a bit too quiet.
    }
}
