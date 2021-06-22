use crate::spec::Spec;

mod gentle;
mod spec;
mod timeline;

fn main() {
	let spec = Spec::load();
	println!("{}", spec.cleaned_transcript(&spec.read_transcript()));
}
