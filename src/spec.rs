use reqwest::blocking::{multipart, Client};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, fs};

use crate::{
	gentle::GentleResponse,
	timeline::{Event, EventType, Timeline},
};

/// A file that specifies the locations and configurations of used files.
#[derive(Deserialize, Clone)]
pub struct Spec {
	pub used: HashMap<String, String>,
	pub audio: String,
	pub transcript: String,
	pub output: String,
	pub work: String,
	pub rate: usize,
	pub resolution: (usize, usize),
}

#[derive(Deserialize, Clone)]
pub enum Asset {
	Js { path: String, args: Value },
	Img(String),
}

#[derive(Deserialize, Debug)]
pub struct IntermediateEvent {
	#[serde(skip_deserializing)]
	offset: usize,
	uuid: String,
	info: EventType,
}

impl Into<Timeline> for Spec {
	fn into(self) -> Timeline {
		println!("Loading transcript");
		let transcript = self.read_transcript();
		// Build a spoken-word-only transcript for Gentle.
		println!("Filtering transcript for Gentle");
		let cleaned_transcript = self.cleaned_transcript(&transcript);
		// Build the list of intermediate events with character offsets.
		println!("Parsing transcript for our purposes");
		let events: Vec<IntermediateEvent> = {
			let mut offset = 0usize;
			let mut json_accumulator = String::from("");
			let mut json_depth = 0usize;
			let mut output = vec![];
			for char in transcript.chars() {
				if char == '{' {
					json_depth += 1;
				}
				if json_depth > 0 {
					json_accumulator.push(char);
				}
				if char == '}' {
					json_depth -= 1;
				}
				if json_depth == 0 && json_accumulator.len() > 0 {
					let mut new_event: IntermediateEvent =
						match serde_json::from_str(&json_accumulator) {
							Ok(v) => v,
							Err(_) => panic!("Bad JSON at character {} of transcript", offset),
						};
					new_event.offset = offset;
					json_accumulator = String::from("");
					output.push(new_event)
				}
				if char == cleaned_transcript.chars().nth(offset + 1).unwrap_or(' ') {
					offset += 1;
				}
			}
			if json_depth != 0 || json_accumulator.len() != 0 {
				panic!("Unterminated JSON")
			}
			output
		};
		// Get Gentle's alignment of our transcript
		println!("Aligning with Gentle (takes a while)");
		println!("> Loading audio");
		let form = multipart::Form::new()
			.text("transcript", cleaned_transcript)
			.file("audio", self.audio)
			.unwrap();
		println!("> Sending request");
		let client = Client::new();
		let response: GentleResponse = {
			let response = client
				.post(
					std::env::var("GENTLE_LOCATION")
						.unwrap_or("http://localhost:8765/transcriptions?async=false".to_string()),
				)
				.multipart(form)
				.send()
				.expect("Failed to contact Gentle");
			println!("Parsing");
			let text = &response.text().unwrap();
			serde_json::from_str(text).expect("Couldn't interpret Gentle's response")
		};
		println!("Re-associating our events with Gentle's timings");
		let words = response.words.iter().filter(|word| {
			word.case == "success"
				&& word.start.is_some()
				&& word.end.is_some()
				&& word.endOffset.is_some()
				&& word.startOffset.is_some()
		});
		let mut events = events.iter();
		let mut output: Vec<Event> = vec![];
		loop {
			let this_event = match events.next() {
				Some(v) => v,
				None => break,
			};
			let corresponding_word = match words
				.clone()
				.filter(|word| {
					word.startOffset.unwrap() < this_event.offset
						&& word.endOffset.unwrap() + 2 > this_event.offset
				})
				.next()
			{
				Some(v) => v,
				None => {
					println!(
						"Beware: We couldn't find a Gentle timing that corresponds with {:?}",
						this_event
					);
					continue;
				}
			};
			let new_event = Event {
				timestamp: corresponding_word.start.unwrap(),
				uuid: this_event.uuid.clone(),
				info: this_event.info.clone(),
			};
			output.push(new_event)
		}
		let length = words.clone().last().unwrap().end.unwrap() + 2f64;
		println!("Done building Timeline");
		Timeline {
			events: output,
			rate: self.rate,
			length,
			resolution: self.resolution,
		}
	}
}

impl Spec {
	pub fn load() -> Spec {
		serde_json::from_str(&fs::read_to_string("./spec.json").expect("Couldn't read spec"))
			.expect("Couldn't interpret spec")
	}
	pub fn cleaned_transcript(&self, transcript: &str) -> String {
		let mut result: Vec<char> = vec![];
		let mut json_depth = 0usize;
		for char in transcript.chars() {
			match char {
				'{' => json_depth += 1,
				'}' => json_depth -= 1,
				' ' if *result.last().unwrap_or(&'a') == ' ' => {}
				'\n' if *result.last().unwrap_or(&'a') == '\n' => {}
				c if json_depth == 0 => result.push(c),
				_ => {}
			}
		}
		result.iter().collect::<String>()
	}
	pub fn read_transcript(&self) -> String {
		fs::read_to_string(self.transcript.clone()).expect("Couldn't read transcript")
	}
}
