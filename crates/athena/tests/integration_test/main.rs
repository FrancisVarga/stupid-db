//! Integration tests for stupid-athena crate.
//!
//! These tests verify the integration of all athena modules without requiring AWS credentials.
//! Tests marked with `#[ignore]` require AWS credentials and must be run explicitly.

mod config;
mod convert;
mod query_step;
mod result;
