use std::{fs, time};

use spec::Spec;
use timeline::{Event, Frame, Timeline};

mod gentle;
mod spec;
mod timeline;

fn main() {
	let spec: Spec = Spec::load();
	let timeline: Timeline = spec.clone().into();
	timeline.render(spec);
}
