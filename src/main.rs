use cpal::{self, Stream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use eyre::{ContextCompat, Result, eyre};
use modfile::ptmf::{self, SampleInfo};

use std::env;
use std::fs::File;
use std::io::BufReader;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

static BUFFER: Lazy<Arc<Mutex<VecDeque<i16>>>> = Lazy::new(|| Arc::new(Mutex::new(VecDeque::new())));


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
    let device = host.default_output_device().wrap_err("failed to find output device")?;
    let config = device.default_output_config()?;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()),
        cpal::SampleFormat::U16 => unimplemented!()
        /* cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), module.sample_info), */
    };

    let mut speed = 6;

    let mut channel0 = vec![0; 500]; // 20000 / 400
    let mut channel0_next = 0;
    let mut channel0_last_period = 0;
    let mut channel0_last_sample = 0;

    let stream = stream.unwrap();
    stream.play().unwrap();

    for pat in module.positions.data.map(|order| &module.patterns[order as usize]) {
        for row in pat.rows.iter() {
            let mut tick = 0;

            while tick < speed {

                let ch0_hz = if row.channels[0].period == 0 {
                    // println!("Div 0 guard {}", channel0_last_period);
                    7093789.2 / (channel0_last_period as f32 * 2.0)
                } else {
                    7093789.2 / (row.channels[0].period as f32 * 2.0)
                };

                let ch0_sample = if row.channels[0].sample_number == 0 {
                    // println!("Div 0 guard {}", channel0_last_period);
                    channel0_last_sample
                } else {
                    row.channels[0].sample_number
                };

                println!("sample number {}, {}", ch0_sample, tick );
                
                let ch1_hz = 7093789.2 / (row.channels[1].period as f32 * 2.0);
                let ch2_hz = 7093789.2 / (row.channels[2].period as f32 * 2.0);
                let ch3_hz = 7093789.2 / (row.channels[3].period as f32 * 2.0);

                let ch0_samples_this_tick = (ch0_hz / 50.0) as u16;

                let ch0_sample = &module.sample_info[ch0_sample as usize];

                // println!("{}, {}", channel0_next, ch0_samples_this_tick);
                if channel0_next + ch0_samples_this_tick >= ch0_sample.length {
                    let (rest, start) = ch0_sample.data.split_at(channel0_next as usize);
                    // println!("Split: {}, {}, {}", rest.len(), start.len(), ch0_sample.length );


                    // start.len() does not reflect actual number of samples left.
                    let to_end_len = ch0_sample.length - channel0_next;
                    // println!("to_end_len: {}", to_end_len);
                    let remaining_len = ch0_samples_this_tick - to_end_len;
                    channel0_next = remaining_len as u16;

                    channel0[0..to_end_len as usize].copy_from_slice(&start[..to_end_len as usize]);
                    channel0[to_end_len as usize..(to_end_len + remaining_len) as usize].copy_from_slice(&rest[..remaining_len as usize]);

                    println!("channel0 wrap {:?} {}", channel0, to_end_len);
                } else {
                    channel0[0..ch0_samples_this_tick as usize].copy_from_slice(&ch0_sample.data[channel0_next as usize..channel0_next as usize + ch0_samples_this_tick as usize]);
                    println!("channel0 no wrap {:?}", &channel0[..ch0_samples_this_tick as usize]);
                }

                channel0_next += ch0_samples_this_tick;
                channel0_next %= ch0_sample.length;


                'outer: loop {
                    let mut deque = BUFFER.lock().unwrap();
                    if deque.len() > 1000 {
                        continue;
                    } else {
                        // println!("{}", channel0.len());
                        for b in &channel0[0..ch0_samples_this_tick as usize] {
                            deque.push_back(*b as i8 as i16);
                        }

                        
                        break 'outer;
                    }
                }


                // println!("{:?}, {}, {}, {}, {}, {}, {}, {:?}, {}", row, tick, ch0_hz, ch1_hz, ch2_hz, ch3_hz, ch0_samples_this_tick, channel0, channel0_last_period);
                tick += 1;
            }

            if row.channels[0].period != 0 {
                channel0_last_period = row.channels[0].period;
            }

            if row.channels[0].sample_number != 0 {
                channel0_last_sample = row.channels[0].sample_number;
            }
        }
        break;
    }

    Ok(())
}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<Stream>
where
    T: cpal::Sample + From<i16>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // println!("In callback");
            write_data(data, channels)
        },
        err_fn,
    )?;
    Ok(stream)

    // std::thread::spawn(move || {
    //     // Produce a sinusoid of maximum amplitude.
        // let mut sample_clock = 0f32;
        // let mut freq = 440.0;

        // loop {
        //     let mut deque = BUFFER.lock().unwrap();

        //     if sample_clock == 48000.0*1.0 {
        //         freq = 880.0;
        //         deque.clear();
        //     }

        //     if deque.len() < 10000 {
        //         deque.push_back((1024.0*((sample_clock * freq * 2.0 * std::f32::consts::PI / sample_rate).sin())) as i16);
        //         sample_clock += 1.0;
        //     }
        // }
    // });

    // std::thread::sleep(std::time::Duration::from_millis(2000));
}

fn write_data<T>(output: &mut [T], channels: usize, /* next_sample: &mut dyn FnMut() -> i16 */)
where
    T: cpal::Sample + From<i16>
{
    let mut deque = BUFFER.lock().unwrap();
    let mut count = 0;
    let (buf0, buf1) = deque.as_slices();

    // println!("first {:?}, rest: {:?}", buf0, buf1);
    let mut sample_bufs = buf0.iter().chain(buf1);

    // println!("Here {}", deque.len());
    for frame in output.chunks_mut(channels) {
        let raw = match sample_bufs.next() {
            Some(i) => {
                count += 1;
                i
            },
            None => {
                // This is hopefully rare, but at least will prevent panic
                // from draining elements that weren't actually used.
                &0
            }
        };

        let value: T = cpal::Sample::from::<i16>(raw);
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }

    // println!("{}", deque.capacity());
    deque.drain(0..count);
}
