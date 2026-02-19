mod documents;
pub(crate) mod parsing;
#[cfg(test)]
mod tests_advanced;
#[cfg(test)]
mod tests_basic;

pub use documents::result_to_documents;
