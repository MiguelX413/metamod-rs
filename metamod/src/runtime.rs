use metamod_sys::{Log, PluginInfo};
use std::{ffi::CStr, marker::PhantomData};

/// Access to plugin services during a callback on the GoldSrc server thread.
pub struct Context<'callback> {
    id: *mut PluginInfo,
    log: Option<Log>,
    console: Option<Log>,
    scope: PhantomData<&'callback ()>,
}

impl<'callback> Context<'callback> {
    pub(crate) fn new(
        _: &'callback (),
        id: *mut PluginInfo,
        log: Option<Log>,
        console: Option<Log>,
    ) -> Self {
        Self {
            id,
            log,
            console,
            scope: PhantomData,
        }
    }

    /// Write a message through Metamod's server log utility when available.
    pub fn log(&self, message: &CStr) {
        self.write(self.log, message);
    }

    /// Print directly to the server console through Metamod.
    pub fn print_console(&self, message: &CStr) {
        self.write(self.console, message);
    }

    fn write(&self, callback: Option<Log>, message: &CStr) {
        if let Some(log) = callback {
            // SAFETY: These are the loader's callback and plugin ID. The
            // message lives through the synchronous varargs call.
            unsafe { log(self.id, c"%s".as_ptr(), message.as_ptr()) };
        }
    }
}
