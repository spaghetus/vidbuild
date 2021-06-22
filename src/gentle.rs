use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct GentleResponse {
	pub status: Option<String>,
	pub transcript: String,
	pub words: Vec<GentleWord>,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
pub struct GentleWord {
	pub case: String,
	pub start: Option<f64>,
	pub word: Option<String>,
	pub startOffset: Option<usize>,
	pub endOffset: Option<usize>,
	pub phones: Option<Vec<GentlePhoneme>>,
	pub end: Option<f64>,
	pub alignedWord: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct GentlePhoneme {
	pub duration: f64,
	pub phone: String,
}
