use reqwest::blocking::{multipart, Client};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, fs};

use crate::{
	gentle::{GentleResponse, GentleWord},
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
	timestamp: usize,
	absolute: Option<f64>,
	relative: Option<f64>,
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
			let mut time = 0usize;
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
							Err(_) => panic!("Bad JSON at character {} of transcript", time),
						};
					new_event.timestamp = time;
					json_accumulator = String::from("");
					output.push(new_event)
				}
				if char == cleaned_transcript.chars().nth(time + 1).unwrap_or(' ') {
					time += 1;
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
		let mut last_good_time = 0.0;
		let mut last_good_offset: usize = 0;
		loop {
			let this_event = match events.next() {
				Some(v) => v,
				None => break,
			};
			let new_event = match (this_event.absolute, this_event.relative) {
				(None, rel) => {
					let corresponding_word: GentleWord = match words
						.clone()
						.filter(|word| {
							word.startOffset.unwrap() < this_event.timestamp
								&& word.endOffset.unwrap() + 2 > this_event.timestamp
						})
						.next()
					{
						Some(v) => {
							last_good_offset = v.endOffset.unwrap();
							last_good_time = v.end.unwrap();
							v.clone()
						}
						None => GentleWord {
							case: "???".to_string(),
							start: Some(last_good_time),
							word: Some("???".to_string()),
							startOffset: Some(last_good_offset),
							endOffset: Some(last_good_offset),
							phones: Some(vec![]),
							end: Some(last_good_time),
							alignedWord: Some("???".to_string()),
						},
					};
					Event {
						timestamp: corresponding_word.start.unwrap() + rel.unwrap_or(0.0),
						uuid: this_event.uuid.clone(),
						info: this_event.info.clone(),
					}
				}
				(Some(abs), rel) => Event {
					timestamp: abs + rel.unwrap_or(0.0),
					uuid: this_event.uuid.clone(),
					info: this_event.info.clone(),
				},
			};
			output.push(new_event)
		}
		output.sort_by(|a, b| {
			a.timestamp
				.partial_cmp(&b.timestamp)
				.unwrap_or(std::cmp::Ordering::Equal)
		});
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
