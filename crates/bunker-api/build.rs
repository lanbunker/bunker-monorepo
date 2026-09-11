// `sqlx::migrate!` embeds the migrations directory at compile time. Without this
// hint, cargo does not rebuild when a migration file is added, and the tests run
// against a schema that lacks the new tables.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
