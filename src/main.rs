use spec::Spec;
use timeline::Timeline;
mod gentle;
mod spec;
mod timeline;

fn main() {
	let spec: Spec = Spec::load();
	let timeline: Timeline = spec.clone().into();
	timeline.render(spec);
}
