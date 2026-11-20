use metamod_sys::{Log, PluginInfo, raw::MetamodServerPrint};
use std::{
    ffi::{CStr, CString},
    marker::PhantomData,
    num::NonZeroU32,
};

/// An engine player slot. The server can reuse this number after disconnect.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlayerId(NonZeroU32);

impl PlayerId {
    pub(crate) fn from_index(index: i32) -> Option<Self> {
        Some(Self(NonZeroU32::new(u32::try_from(index).ok()?)?))
    }

    /// The one-based slot reported by GoldSrc.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Access to plugin services during a callback on the GoldSrc server thread.
pub struct Context<'callback> {
    id: *mut PluginInfo,
    log: Log,
    console: Log,
    print: MetamodServerPrint,
    scope: PhantomData<&'callback ()>,
}

/// Scoped access to server operations.
pub struct Server<'context, 'callback> {
    context: &'context Context<'callback>,
}

impl<'callback> Context<'callback> {
    pub(crate) fn new(
        _: &'callback (),
        id: *mut PluginInfo,
        log: Log,
        console: Log,
        print: MetamodServerPrint,
    ) -> Self {
        Self {
            id,
            log,
            console,
            print,
            scope: PhantomData,
        }
    }

    pub fn server(&self) -> Server<'_, 'callback> {
        Server { context: self }
    }

    /// Write a message through Metamod's server log utility when available.
    pub fn log(&self, message: &CStr) {
        self.write(self.log, message);
    }

    /// Print directly to the server console through Metamod.
    pub fn print_console(&self, message: &CStr) {
        self.write(self.console, message);
    }

    fn write(&self, callback: Log, message: &CStr) {
        if let Some(log) = callback {
            // SAFETY: The loader supplied the callback and plugin ID.
            unsafe { log(self.id, c"%s".as_ptr(), message.as_ptr()) };
        }
    }
}

impl Server<'_, '_> {
    /// Print a line to the server console. Embedded NULs become replacement
    /// characters. Falls back to Metamod's console utility if needed.
    pub fn print(&self, message: &str) {
        let mut bytes = message.replace('\0', "�").into_bytes();
        if let Some(print) = self.context.print {
            if !bytes.ends_with(b"\n") {
                bytes.push(b'\n');
            }
            if let Ok(line) = CString::new(bytes) {
                // SAFETY: The engine callback is valid on the server thread,
                // and the C string lives through the synchronous call.
                unsafe { print(line.as_ptr()) };
            }
        } else if let Ok(line) = CString::new(bytes) {
            self.context.print_console(&line);
        }
    }
}
