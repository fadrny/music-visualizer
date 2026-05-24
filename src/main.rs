const APP_NAME: &str = "Vinyl Audio Visualizer";
const STATUS: &str = "Rust rewrite baseline is ready.";

fn startup_message() -> String {
    format!("{} - {}", APP_NAME, STATUS)
}

fn main() {
    println!("{}", startup_message());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_message_mentions_rust_rewrite() {
        let message = startup_message();
        assert!(message.contains(APP_NAME));
        assert!(message.contains("Rust rewrite"));
    }
}
