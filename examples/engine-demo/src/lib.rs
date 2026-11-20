use metamod_rs::prelude::*;

const MESSAGE: &str = "rust_hello_message";

#[derive(Default)]
struct EngineDemo {
    first_frame_seen: bool,
}

#[metamod_plugin(
    name = "Rust Engine Demo",
    version = "0.1.0",
    author = "metamod-rs contributors",
    log_tag = "RUSTENGINE"
)]
impl Plugin for EngineDemo {
    const LAG_COMPENSATION_PRE: bool = true;
    const LAG_COMPENSATION_POST: bool = true;

    fn allow_lag_compensation(&mut self, _: &mut Context<'_>) -> HookResult<bool> {
        HookResult::Handled
    }

    fn allow_lag_compensation_post(
        &mut self,
        _: &mut Context<'_>,
        original: Option<bool>,
    ) -> PostHookResult<bool> {
        match original {
            Some(_) => PostHookResult::Handled,
            None => PostHookResult::Ignored,
        }
    }

    fn initialize(&mut self, ctx: &mut Context<'_>) -> bool {
        ctx.server().print("Rust Engine Demo loaded");
        true
    }

    fn start_frame(&mut self, ctx: &mut Context<'_>) {
        if !self.first_frame_seen {
            self.first_frame_seen = true;
            let server = ctx.server();
            match server.cvar(MESSAGE) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    if let Err(error) = server.register_cvar(MESSAGE, "Hello from Rust!") {
                        server.print(&format!("Could not register {MESSAGE}: {error}"));
                    }
                }
                Err(error) => server.print(&format!("Could not read {MESSAGE}: {error}")),
            }
            if let Err(error) = server.register_command("rust_hello") {
                server.print(&format!("Could not register rust_hello: {error}"));
            }
            server.print("Rust Engine Demo first frame");
        }
    }

    fn server_command(&mut self, ctx: &mut Context<'_>, command: &Command) {
        let server = ctx.server();
        if let Some(argument) = command.arg(0) {
            let Ok(argument) = argument.to_str() else {
                server.print("rust_hello expects a UTF-8 message");
                return;
            };
            if let Err(error) = server.set_cvar(MESSAGE, argument) {
                server.print(&format!("Could not update {MESSAGE}: {error}"));
                return;
            }
        }
        match server.cvar(MESSAGE) {
            Ok(Some(message)) => server.print(&message.to_string_lossy()),
            Ok(None) => server.print("rust_hello_message is unavailable"),
            Err(error) => server.print(&format!("Could not read {MESSAGE}: {error}")),
        }
    }

    fn player_join(&mut self, ctx: &mut Context<'_>, player: PlayerId) {
        let info = ctx.player_info(player);
        if !ctx.is_player_current(player) {
            return;
        }
        let slot = player.slot();
        let server = ctx.server();
        if let Some(info) = info
            && let Some(user_id) = info.user_id
        {
            server.print(&format!(
                "Hello, player in slot {slot} (user ID {user_id})!"
            ));
        } else {
            server.print(&format!("Hello, player in slot {slot}!"));
        }
    }
}
