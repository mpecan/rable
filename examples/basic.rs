//! Basic usage example for the Rable bash parser.
//!
//! Run with: `cargo run --example basic`

#![allow(clippy::expect_used)]

use rable::{NodeKind, parse};

fn main() {
    // Parse a simple pipeline
    let source = "echo $USER | grep root";
    let nodes = parse(source, false).expect("valid bash");

    println!("Source: {source}");
    println!("S-expression: {}", nodes[0]);
    println!();

    // Inspect the AST
    if let NodeKind::Pipeline { commands, .. } = &nodes[0].kind {
        println!("Pipeline with {} commands:", commands.len());
        for (i, cmd) in commands.iter().enumerate() {
            if let NodeKind::Command {
                words, redirects, ..
            } = &cmd.kind
            {
                let word_values: Vec<_> = words
                    .iter()
                    .filter_map(|w| {
                        if let NodeKind::Word { value, .. } = &w.kind {
                            Some(value.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                println!(
                    "  Command {}: {:?} ({} redirects)",
                    i,
                    word_values,
                    redirects.len()
                );
            }
        }
    }

    println!();

    // Parse a compound command
    let source = r#"if [ -f /etc/passwd ]; then echo "exists"; fi"#;
    let nodes = parse(source, false).expect("valid bash");
    println!("Source: {source}");
    println!("S-expression: {}", nodes[0]);
    println!();

    // Error handling
    match parse("if", false) {
        Ok(_) => println!("Unexpectedly parsed"),
        Err(e) => {
            println!(
                "Parse error at line {}, pos {}: {}",
                e.line(),
                e.pos(),
                e.message()
            );
        }
    }
}
