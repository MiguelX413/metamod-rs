use metamod_sys::{Log, PluginInfo, raw::MetamodServerPrint};
use std::{
    ffi::CStr,
    fmt,
    marker::PhantomData,
    num::{NonZeroI32, NonZeroU64},
};

/// A one-based engine slot. GoldSrc can reuse it for a different player.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlayerSlot(NonZeroI32);

impl PlayerSlot {
    /// Construct a slot from GoldSrc's signed, one-based index. Zero and
    /// negative indices return `None`. Use `Context::player_at` to check occupancy.
    pub const fn new(index: i32) -> Option<Self> {
        match NonZeroI32::new(index) {
            Some(index) if index.get() > 0 => Some(Self(index)),
            _ => None,
        }
    }

    /// Return the positive, one-based GoldSrc slot index.
    pub const fn get(self) -> i32 {
        self.0.get()
    }
}

impl fmt::Display for PlayerSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.get(), f)
    }
}

/// The supplied index was zero or negative.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidPlayerSlot(i32);

impl fmt::Display for InvalidPlayerSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "player slot index must be positive, got {}", self.0)
    }
}

impl std::error::Error for InvalidPlayerSlot {}

impl TryFrom<i32> for PlayerSlot {
    type Error = InvalidPlayerSlot;

    fn try_from(index: i32) -> Result<Self, Self::Error> {
        Self::new(index).ok_or(InvalidPlayerSlot(index))
    }
}

impl From<PlayerSlot> for i32 {
    fn from(slot: PlayerSlot) -> Self {
        slot.get()
    }
}

/// A positive, server-assigned GoldSrc user ID. This is separate from an
/// edict slot or a map-scoped player session handle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlayerUserId(NonZeroI32);

impl PlayerUserId {
    /// Construct an ID from GoldSrc's integer value. Zero and negatives
    /// represent unavailable or invalid IDs.
    pub const fn new(value: i32) -> Option<Self> {
        match NonZeroI32::new(value) {
            Some(value) if value.get() > 0 => Some(Self(value)),
            _ => None,
        }
    }

    pub const fn get(self) -> i32 {
        self.0.get()
    }
}

impl fmt::Display for PlayerUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.get(), f)
    }
}

/// The supplied GoldSrc user ID was zero or negative.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidPlayerUserId(i32);

impl fmt::Display for InvalidPlayerUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "player user ID must be positive, got {}", self.0)
    }
}

impl std::error::Error for InvalidPlayerUserId {}

impl TryFrom<i32> for PlayerUserId {
    type Error = InvalidPlayerUserId;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidPlayerUserId(value))
    }
}

impl From<PlayerUserId> for i32 {
    fn from(id: PlayerUserId) -> Self {
        id.get()
    }
}

/// A player session observed by this plugin, scoped to a map.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlayerId {
    slot: PlayerSlot,
    serial: NonZeroU64,
}

impl PlayerId {
    pub(crate) fn new(slot: PlayerSlot, serial: NonZeroU64) -> Self {
        Self { slot, serial }
    }

    pub fn slot(self) -> PlayerSlot {
        self.slot
    }

    /// The one-based GoldSrc slot index. Use the full ID for identity comparisons.
    pub fn get(self) -> i32 {
        self.slot.get()
    }
}

/// Access to plugin services during a callback on the GoldSrc server thread.
pub struct Context<'callback> {
    id: *mut PluginInfo,
    log: Log,
    console: Log,
    print: MetamodServerPrint,
    pub(crate) command: crate::engine::CommandCallback,
    scope: PhantomData<&'callback ()>,
}

/// Scoped access to server operations.
pub struct Server<'context, 'callback> {
    pub(crate) context: &'context Context<'callback>,
}

impl<'callback> Context<'callback> {
    pub(crate) fn new(
        _: &'callback (),
        id: *mut PluginInfo,
        log: Log,
        console: Log,
        print: MetamodServerPrint,
        command: crate::engine::CommandCallback,
    ) -> Self {
        Self {
            id,
            log,
            console,
            print,
            command,
            scope: PhantomData,
        }
    }

    pub fn server(&self) -> Server<'_, 'callback> {
        Server { context: self }
    }

    /// Look up the currently observed session in a reusable slot.
    pub fn player_at(&self, slot: PlayerSlot) -> Option<PlayerId> {
        crate::__ffi::player_at(slot)
    }

    /// Whether this exact session is still tracked on the current map.
    pub fn is_player_current(&self, player: PlayerId) -> bool {
        self.player_at(player.slot()) == Some(player)
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
        if crate::__ffi::engine_available()
            && let Some(log) = callback
        {
            // SAFETY: The loader supplied the callback and plugin ID.
            unsafe { log(self.id, c"%s".as_ptr(), message.as_ptr()) };
        }
    }
}

impl Server<'_, '_> {
    /// Pass native bytes directly to the engine console. Include a newline
    /// for line output. This does nothing when the engine callback is unavailable.
    pub fn print(&self, message: &CStr) {
        if crate::__ffi::engine_available()
            && let Some(print) = self.context.print
        {
            // SAFETY: The engine callback is valid on the server thread,
            // and the C string lives through the synchronous call.
            unsafe { print(message.as_ptr()) };
        }
    }
}
