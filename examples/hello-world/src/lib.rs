use metamod::prelude::*;

#[derive(Default)]
struct HelloWorld {
    first_frame_seen: bool,
}

#[metamod_plugin(cargo)]
impl Plugin for HelloWorld {
    fn initialize(&mut self, ctx: &mut Context<'_>) -> bool {
        ctx.server().print("Hello from Rust Metamod");
        true
    }

    fn start_frame(&mut self, ctx: &mut Context<'_>) {
        if !self.first_frame_seen {
            self.first_frame_seen = true;
            ctx.server().print("Rust StartFrame hook ran");
        }
    }

    fn player_join(&mut self, ctx: &mut Context<'_>, player: PlayerId) {
        if ctx.is_player_current(player) {
            ctx.server()
                .print(&format!("Hello, player in slot {}!", player.slot().get()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_uses_cargo_defaults_and_manifest_overrides() {
        let metadata = HelloWorld::METADATA;
        assert_eq!(metadata.name, c"Rust Hello World");
        assert_eq!(metadata.log_tag, c"RUSTHELLO");
        assert_eq!(
            metadata.version.to_bytes(),
            env!("CARGO_PKG_VERSION").as_bytes()
        );
        assert_eq!(
            metadata.author.to_bytes(),
            env!("CARGO_PKG_AUTHORS").as_bytes()
        );
        assert_eq!(
            metadata.url.to_bytes(),
            env!("CARGO_PKG_REPOSITORY").as_bytes()
        );
    }
}
