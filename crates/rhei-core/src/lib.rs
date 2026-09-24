//! Core plan model and primitives for the Rhei agent runtime.
//!
//! This crate owns the shared pieces that runtime drivers build on:
//! - token definitions and a tokenizer in [`crate::tokens`] and [`crate::lexer`]
//! - AST types in [`crate::ast`]
//! - the markdown plan parser in [`parse`]
//! - callback context and workspace helpers used by execution commands
//!
//! Most consumers only need [`parse`] plus the public AST types
//! from [`crate::ast`].

pub mod ast;
pub mod blocks;
/// The one serialized account beneath every neural start.
/// §AR-neural-admission
pub mod budget;
pub mod callback;
pub mod lexer;
pub mod parser;
pub mod platform;
pub mod source;
pub mod state_machine;
pub(crate) mod text;
pub mod tokens;
pub mod workspace;

pub use lexer::{tokenize, Tokenizer};
pub use parser::parse;
pub use tokens::Token;

/// Returns the crate version reported by Cargo metadata.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Returns a short human-readable help string for compatibility surfaces.
///
/// The main command-line experience lives in the [`rhei-cli`](../../rhei-cli/src/main.rs)
/// binary crate.
pub fn help_text() -> String {
    "Rhei - agent runtime for governed Markdown workflows\n\nUsage:\n  rhei [OPTIONS]\n\nFor now, use --help and --version."
        .to_string()
}
pub mod root_access;
pub mod transition_history;
