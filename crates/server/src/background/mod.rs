mod catalog;
mod compute;
mod discovery;
mod loader;

pub(crate) use discovery::discover_segments;
pub(crate) use loader::background_load;
