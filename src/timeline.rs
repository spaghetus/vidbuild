use crate::spec::Spec;
use png::Encoder;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use std::{
	collections::HashMap,
	fs,
	sync::{Arc, RwLock},
	time::Duration,
};
use subprocess::{Popen, PopenConfig};

pub struct Timeline {
	pub events: Vec<Event>,
	pub rate: usize,
	pub length: f64,
	pub resolution: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct Event {
	pub timestamp: f64,
	pub uuid: String,
	pub info: EventType,
}

#[derive(Deserialize, Clone, Debug)]
pub enum EventType {
	JsStart(Value),
	ImgStart(String, (usize, usize, usize, usize)),
	JsEnd,
	ImgEnd,
}

#[derive(Clone)]
pub struct Frame {
	assets: Arc<RwLock<HashMap<String, Vec<u8>>>>,
	contents: Vec<Event>,
	resolution: (usize, usize),
}
#[allow(dead_code)]
impl Frame {
	pub fn new(resolution: (usize, usize)) -> Frame {
		Frame {
			assets: Arc::new(RwLock::new(HashMap::new())),
			contents: vec![],
			resolution,
		}
	}
	pub fn apply(&mut self, used: HashMap<String, String>, event: &Event) -> () {
		match &event.info {
			EventType::JsStart(_) => todo!(),
			EventType::ImgStart(slug, _) => {
				if self.assets.read().unwrap().get(slug).is_none() {
					self.assets.write().unwrap().insert(
						slug.to_string(),
						fs::read(used.get(slug).expect("Bad reference to undefined resource"))
							.expect("Illegible resource"),
					);
				}
				self.contents.push(event.clone())
			}
			EventType::ImgEnd | EventType::JsEnd => self.contents.retain(|v| v.uuid != event.uuid),
		}
	}
}

#[allow(dead_code)]
impl Timeline {
	pub fn render(&self, spec: Spec) {
		let mut base = Frame::new(self.resolution);
		let events = self.events.clone();
		let mut cursor = 0usize;
		println!("Rendering...");
		// // Build the encoder
		// let mut encoder = VideoEncoder::builder("libvpx")
		// 	.expect("Couldn't build encoder")
		// 	.width(spec.resolution.0)
		// 	.height(spec.resolution.1)
		// 	.time_base(TimeBase::new(1, spec.rate as u32))
		// 	.pixel_format(get_pixel_format("rgb24"))
		// 	.build()
		// 	.expect("Couldn't build encoder");
		// mkdir
		fs::create_dir_all(spec.work.clone()).expect("Couldn't mkdir");
		// Render to PNGs on disk
		(1usize..)
			.into_iter()
			// Write time as float.
			.map(|frame| (frame, frame as f64 / self.rate as f64))
			// Limit to frames within the length of the video.
			.take_while(|(_, time)| time < &self.length)
			// Find events that have occurred before the current frame and haven't been consumed yet.
			.map(|(frame, time)| {
				let events_this_frame = events
					.iter()
					.skip(cursor)
					.take_while(|v| v.timestamp < time)
					.collect::<Vec<&Event>>();
				// Mark the consumed events as consumed.
				cursor += events_this_frame.len();
				// Apply the consumed events to the frame state.
				for i in events_this_frame {
					base.apply(spec.used.clone(), i)
				}
				(frame, time, base.clone())
			})
			// Comment this line when debugging, make sure it's not commented in commits.
			.par_bridge()
			// Draw the frame.
			.map(|(frame_count, _time, frame)| {
				let mut result = vec![0u8; spec.resolution.0 * spec.resolution.1 * 4];
				for event in frame.contents {
					event.render(frame.assets.clone(), &mut result, spec.resolution);
				}
				println!("{}", frame_count);
				(frame_count, result)
			})
			// Write the frame to a PNG file.
			.for_each(|(frame_count, pixel_data)| {
				let file = fs::OpenOptions::new()
					.write(true)
					.create(true)
					.truncate(true)
					.open(format!("{}/{}.png", spec.work, {
						let mut count = frame_count.to_string();
						while count.len() < 10 {
							count = "0".to_owned() + &count;
						}
						count
					}))
					.expect("Couldn't open output png");
				let mut encoder =
					Encoder::new(file, spec.resolution.0 as u32, spec.resolution.1 as u32);
				encoder.set_color(png::ColorType::RGBA);
				encoder.set_depth(png::BitDepth::Eight);
				let mut writer = encoder
					.write_header()
					.expect("Couldn't write output png header");
				writer
					.write_image_data(&pixel_data)
					.expect("Couldn't write image data");
			});
		println!("Transcoding...");
		let cmd = [
			"ffmpeg",
			"-y",
			"-r",
			&spec.rate.to_string(),
			"-i",
			&format!("{}/%10d.png", spec.work),
			"-i",
			&spec.audio,
			&spec.output,
		];
		let mut ffmpeg =
			Popen::create(&cmd, PopenConfig::default()).expect("Couldn't start ffmpeg");
		loop {
			match ffmpeg
				.wait_timeout(Duration::from_secs(600))
				.expect("Error waiting for ffmpeg")
			{
				Some(_) => {
					println!("Done!");
					break;
				}
				None => {
					println!("ffmpeg is taking a while...")
				}
			}
		}
	}
}

#[allow(dead_code)]
impl Event {
	pub fn render(
		&self,
		assets: Arc<RwLock<HashMap<String, Vec<u8>>>>,
		frame: &mut Vec<u8>,
		resolution: (usize, usize),
	) {
		match &self.info {
			EventType::JsStart(_) => todo!(),
			EventType::ImgStart(slug, (x, y, w, h)) => {
				// Load the image.
				let assets = assets.read().unwrap();
				let decoder = png::Decoder::new(assets.get(slug).unwrap().as_slice());
				// Get its metadata.
				let (png_info, mut png_reader) =
					decoder.read_info().expect("Couldn't read png header");
				let (iw, ih) = (png_info.width, png_info.height);
				let (fw, _fh) = resolution;
				// Load the PNG into memory.
				let mut png_vec = vec![0u8; (iw * ih * 4) as usize];
				png_reader
					.next_frame(png_vec.as_mut_slice())
					.expect("Couldn't load png into memory");
				// Write the PNG onto the frame.
				for (index, rgb) in frame.chunks_exact_mut(4).enumerate() {
					if let [r, g, b, a] = rgb {
						// Coordinates in pixel-space
						let px = index % fw;
						let py = (index as f64 / fw as f64).floor() as usize;
						// Coordinates in multiplicative image-space
						let rx = (px as f64 - *x as f64) / *w as f64;
						let ry = (py as f64 - *y as f64) / *h as f64;
						if (1.0 < rx || rx < 0.0) || (ry > 1.0 || ry < 0.0) {
							continue;
						}
						// Coordinates in image-space
						let ix = (rx * iw as f64) as u32;
						let iy = (ry * ih as f64) as u32;
						// Pixel data offset
						let pd = (ix as usize + (iy as usize * iw as usize)) * 4;
						// Write pixel data
						if pd >= png_vec.len() {
							continue;
						}
						*a = 255;
						let a = png_vec[pd + 3];
						*r = (png_vec[pd] as f64 * (a as f64 / 255.0)) as u8
							+ (*r as f64 * (1.0 - (a as f64 / 255.0))) as u8;
						*g = (png_vec[pd + 1] as f64 * (a as f64 / 255.0)) as u8
							+ (*g as f64 * (1.0 - (a as f64 / 255.0))) as u8;
						*b = (png_vec[pd + 2] as f64 * (a as f64 / 255.0)) as u8
							+ (*b as f64 * (1.0 - (a as f64 / 255.0))) as u8;
					}
				}
			}
			_ => todo!(),
		}
	}
}
