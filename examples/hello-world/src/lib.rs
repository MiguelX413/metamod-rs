use metamod::{Context, Metadata, Plugin, export_plugin};
use std::sync::atomic::{AtomicBool, Ordering};

struct HelloWorld;

impl Plugin for HelloWorld {
    const METADATA: Metadata = Metadata {
        name: c"Rust Hello World",
        version: c"0.1.0",
        date: c"",
        author: c"metamod-rs contributors",
        url: c"",
        log_tag: c"RUSTHELLO",
    };

    fn initialize(context: &Context<'_>) -> bool {
        context.print_console(c"Hello from Rust Metamod");
        true
    }

    fn start_frame(context: &Context<'_>) {
        static FIRST_FRAME: AtomicBool = AtomicBool::new(true);
        if FIRST_FRAME.swap(false, Ordering::Relaxed) {
            context.print_console(c"Rust StartFrame hook ran");
        }
    }
}

export_plugin!(HelloWorld);
