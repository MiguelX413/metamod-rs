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
mod tests;
