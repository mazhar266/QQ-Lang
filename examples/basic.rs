//! Minimal Rust consumer.
//!
//! ```bash
//! cargo run --example basic -- "Q:2:1-3,255;B:1:1;"
//! ```

fn main() -> Result<(), qql::Error> {
    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Q:2:255;B:1:1;".to_string());

    let mut ctx = qql::Context::new("sources");

    for record in ctx.execute(&query)? {
        println!("{} [{}]", record.collection, record.source);
        println!("  {}", record.ar);
        println!("  {}", record.en);
        println!();
    }

    // `execute_json` is the total version — it serializes errors instead of
    // returning them, so it never fails.
    println!("{}", ctx.execute_json("Q:2:5-1"));

    Ok(())
}
